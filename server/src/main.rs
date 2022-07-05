use anyhow::{Error, Result};
use tokio::sync::mpsc;
use tracing_subscriber::EnvFilter;

mod api;
mod emulator;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    color_eyre::install().map_err(Error::msg)?;

    let (tx, rx) = mpsc::channel(10);
    let handle = tokio::spawn(emulator::task(rx));

    tracing::info!("Starting the app");
    let app = api::router(tx).await;

    let (handle_err, server_err) = tokio::join!(
        handle,
        axum::Server::bind(&"0.0.0.0:8080".parse().unwrap()).serve(app.into_make_service()),
    );

    handle_err??;
    server_err?;

    Ok(())
}
