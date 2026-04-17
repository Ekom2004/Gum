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
    let addr: SocketAddr = "127.0.0.1:8000".parse().map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid listen address: {error}"),
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
