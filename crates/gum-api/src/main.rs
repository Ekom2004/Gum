use std::net::SocketAddr;

use gum_api::app_state::AppState;
use gum_api::handlers;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let state = AppState::for_dev().map_err(|message| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("failed to seed dev state: {message}"),
        )
    })?;
    let app = handlers::router(state);
    let bind_addr = std::env::var("GUM_API_BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1".to_string());
    let port = std::env::var("GUM_API_PORT")
        .ok()
        .and_then(|value| value.parse::<u16>().ok())
        .unwrap_or(8000);
    let addr: SocketAddr = format!("{bind_addr}:{port}").parse().map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid listen address '{bind_addr}:{port}': {error}"),
        )
    })?;

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("failed to build tokio runtime: {error}"),
            )
        })?;

    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(addr).await.map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::AddrNotAvailable,
                format!("failed to bind {addr}: {error}"),
            )
        })?;

        tracing::info!("gum-api listening on http://{}", addr);
        axum::serve(listener, app).await.map_err(|error| {
            std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("server exited with error: {error}"),
            )
        })
    })?;

    Ok(())
}
