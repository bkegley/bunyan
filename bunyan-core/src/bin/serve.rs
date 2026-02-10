#[tokio::main]
async fn main() {
    let port: u16 = std::env::var("BUNYAN_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(3333);

    let state = bunyan_core::init_state();
    bunyan_core::server::start_server(state, port).await;
}
