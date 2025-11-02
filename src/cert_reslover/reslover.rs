// cert reslover for domain names
use dashmap::DashMap;
use rustls::crypto::aws_lc_rs::sign::any_supported_type;
use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use rustls::server::{ClientHello, ResolvesServerCert};
use rustls::sign::CertifiedKey;
use std::sync::Arc;

#[derive(Debug, Default)]
pub struct DynamicResolver {
    pub certs: DashMap<String, Arc<CertifiedKey>>, // dashmap for thread safe
}

impl ResolvesServerCert for DynamicResolver {
    fn resolve(&self, client_hello: ClientHello) -> Option<Arc<CertifiedKey>> {
        client_hello
            .server_name()
            .and_then(|name| self.certs.get(name).map(|v| v.clone()))
    }
}
impl DynamicResolver {
    pub fn insert(
        &mut self,
        host: String,
        private: String,
        fullchain: String,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let key = PrivateKeyDer::from_pem_file(private)?;
        let certs = CertificateDer::pem_file_iter(fullchain)?.collect::<Result<Vec<_>, _>>()?;
        let signing_key = any_supported_type(&key)?; // Arc<dyn SigningKey> zaten
        let cert_key = Arc::new(CertifiedKey::new(certs, signing_key));
        self.certs.insert(host, cert_key);
        Ok(())
    }
}
