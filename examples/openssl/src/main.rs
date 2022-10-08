use salvo::listener::openssl::{Keycert, OpensslConfig};
use salvo::prelude::*;

#[handler]
async fn hello_world(res: &mut Response) {
    res.render(Text::Plain("Hello World"));
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt().init();

    let router = Router::new().get(hello_world);
    let config = OpensslConfig::new(
        Keycert::new()
            .with_cert(include_bytes!("../certs/cert.pem").as_ref())
            .with_key(include_bytes!("../certs/key.pem").as_ref()),
    );
    tracing::info!("Listening on https://127.0.0.1:7878");
    let listener = OpensslListener::with_config(config).bind("127.0.0.1:7878");
    Server::new(listener).serve(router).await;
}
