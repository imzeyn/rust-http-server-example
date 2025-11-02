use base64::{Engine as _, engine::general_purpose};
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt, stream::unfold};
use http_body_util::{BodyExt, Empty, Full, StreamBody, combinators::BoxBody};
use hyper_util::rt::TokioIo;
use sha1::{Digest, Sha1};

use hyper::{
    Request, Response, StatusCode,
    body::{Frame, Incoming},
    header::{CONNECTION, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_KEY, UPGRADE},
    upgrade::Upgraded,
};

use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{WebSocketStream, tungstenite::protocol::Message};

pub async fn handle_http1(
    req: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    // websocket request
    if req.headers().contains_key(UPGRADE)
        && req.headers().get(UPGRADE).unwrap() == "websocket"
        && req
            .headers()
            .get(CONNECTION)
            .map(|v| v.to_str().unwrap_or("").to_lowercase().contains("upgrade"))
            .unwrap_or(false)
    {
        let sec_websocket_key = match req.headers().get(SEC_WEBSOCKET_KEY) {
            Some(key) => key.clone(),
            None => {
                return Ok(Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .body(full_body("Missing Sec-WebSocket-Key"))
                    .unwrap());
            }
        };

        // Sec-WebSocket-Key + magic string hash
        // Do not change this value.
        // This is the fixed GUID defined by the WebSocket protocol (RFC 6455, Section 1.3).
        // It is appended to the client key to compute the Sec-WebSocket-Accept header.
        let magic = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";
        let mut hasher = Sha1::new();
        hasher.update(sec_websocket_key.as_bytes());
        hasher.update(magic.as_bytes());
        let result = hasher.finalize();
        let accept_key = general_purpose::STANDARD.encode(result);

        // Spawn task for the upgraded connection
        tokio::task::spawn(async move {
            match hyper::upgrade::on(req).await {
                Ok(upgraded) => {
                    websocket_handler(upgraded).await;
                }
                Err(e) => eprintln!("upgrade error: {}", e),
            }
        });

        // 101 Switching Protocols
        let mut res = Response::new(empty_body());
        *res.status_mut() = StatusCode::SWITCHING_PROTOCOLS;
        let headers = res.headers_mut();
        headers.insert(UPGRADE, "websocket".parse().unwrap());
        headers.insert(CONNECTION, "Upgrade".parse().unwrap());
        headers.insert(SEC_WEBSOCKET_ACCEPT, accept_key.parse().unwrap());

        return Ok(res);
    }

    if req.uri().path() == "/stream" {
        return simple_array_send().await;
    }

    let mut home_page = Response::new(full_body("Hello"));
    *home_page.status_mut() = StatusCode::OK;
    Ok(home_page)
}

// Standard WebSocket connections are not directly supported over HTTP/2.
// You can still use HTTP/2 streams for bidirectional communication with custom protocols.
pub async fn handle_http2(
    req: Request<Incoming>,
) -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    if req.uri().path() == "/stream" {
        return simple_array_send().await;
    }

    let mut home_page = Response::new(full_body("Hello from http2"));
    *home_page.status_mut() = StatusCode::OK;
    Ok(home_page)
}

fn empty_body() -> BoxBody<Bytes, hyper::Error> {
    Empty::<Bytes>::new()
        .map_err(|never| match never {})
        .boxed()
}

fn full_body<T: Into<Bytes>>(chunk: T) -> BoxBody<Bytes, hyper::Error> {
    Full::new(chunk.into())
        .map_err(|never| match never {})
        .boxed()
}

async fn websocket_handler(upgraded: Upgraded) {
    let io = TokioIo::new(upgraded);

    let mut ws_stream = WebSocketStream::from_raw_socket(
        io,
        tokio_tungstenite::tungstenite::protocol::Role::Server,
        None,
    )
    .await;

    ws_stream
        .send(Message::Text("Hello WebSocket".into()))
        .await
        .unwrap();

    // listen client
    while let Some(msg) = ws_stream.next().await {
        let msg = msg.unwrap();
        println!("Recivred: {:?}", msg);
    }
}

pub async fn simple_array_send() -> Result<Response<BoxBody<Bytes, hyper::Error>>, hyper::Error> {
    let chunks = vec!["This", "is", "a", "just an stream data"];

    let stream = unfold(0, move |mut idx| {
        let chunks = chunks.clone(); // move closure içinde kullanmak için
        async move {
            if idx >= chunks.len() {
                return None;
            }
            let data = chunks[idx];
            idx += 1;
            sleep(Duration::from_secs(1)).await;
            Some((Ok(Frame::data(Bytes::from(data))), idx))
        }
    });

    let stream_body = StreamBody::new(stream);
    let body = http_body_util::BodyExt::boxed(stream_body);

    let response = Response::builder()
        .status(StatusCode::OK)
        .body(body)
        .unwrap();

    Ok(response)
}
