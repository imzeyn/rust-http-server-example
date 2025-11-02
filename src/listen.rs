use std::{pin::Pin, sync::Arc};

use crate::{cert_reslover::DynamicResolver, handlers::*};
use hyper::{
    server::conn::{http1, http2},
    service::service_fn,
};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

pub async fn http() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:80").await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = TokioIo::new(stream);

        tokio::task::spawn(async move {
            let conn = http1::Builder::new().serve_connection(io, service_fn(handle_http1));
            let mut conn = conn.with_upgrades();
            let mut conn = Pin::new(&mut conn);

            tokio::select! {
                res = &mut conn => {
                    if let Err(err) = res {
                        println!("Error serving connection: {:?}", err);
                    }
                }
            }
        });
    }
}

#[derive(Clone)]
pub struct TokioExecutor;

impl<F> hyper::rt::Executor<F> for TokioExecutor
where
    F: std::future::Future + Send + 'static,
    F::Output: Send + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn(fut);
    }
}

pub async fn https(
    domain_name: String,
    private_key: String,
    fullchain_pem: String,
) -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:443").await?;

    // Create a separate TLS resolver instance for each domain.
    // Each resolver maps a domain name to its corresponding private key and certificate chain.
    let mut tls_reslover = DynamicResolver::default();
    tls_reslover
        .insert(domain_name, private_key, fullchain_pem)
        .unwrap();

    // Use tls config with tls reslover
    let mut tls_config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_cert_resolver(Arc::new(tls_reslover));

    // Give http2 support and http1.1 support
    tls_config.alpn_protocols = vec!["h2".into(), "http/1.1".into()];

    let tls_acceptor = TlsAcceptor::from(Arc::new(tls_config));

    loop {
        let (stream, _) = listener.accept().await.unwrap();
        let acceptor = tls_acceptor.clone();

        tokio::spawn(async move {
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    let protocol = tls_stream
                        .get_ref()
                        .1 // server connection context
                        .alpn_protocol();

                    if let Some(proto) = protocol {
                        println!("Negotiated protocol: {}", String::from_utf8_lossy(proto));
                    } else {
                        println!("No ALPN protocol negotiated");
                    }

                    match protocol {
                        Some(b"h2") => {
                            // handle http2
                            let io = TokioIo::new(tls_stream);
                            if let Err(err) = http2::Builder::new(TokioExecutor)
                                .serve_connection(io, service_fn(handle_http2))
                                .await
                            {
                                eprintln!("Error serving connection: {}", err);
                            }
                        }
                        Some(b"http/1.1") => {
                            let io = TokioIo::new(tls_stream);
                            let conn = http1::Builder::new()
                                .serve_connection(io, service_fn(handle_http1));
                            let mut conn = conn.with_upgrades();
                            let mut conn = Pin::new(&mut conn).await.unwrap();
                        }
                        _ => {
                            println!("Invalid protocol")
                        }
                    }
                }
                Err(e) => eprintln!("TLS handshake failed: {:?}", e),
            }
        });
    }
}
