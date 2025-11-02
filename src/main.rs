mod cert_reslover;
mod handlers;
mod listen;

#[tokio::main]
async fn main() {
    let domain_name = "example.com".to_string();
    let private_key_path = "/path/to/private.key".to_string();

    let fullchain_path = "/path/to/fullchain.pem".to_string();

    let (https_result, http_result) = tokio::join!(
        listen::https(domain_name, private_key_path, fullchain_path),
        listen::http(),
    );

    https_result.unwrap();
    http_result.unwrap();
}
