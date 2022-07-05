use anyhow::{Error, Result};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use emulator::TwitchThreadCommand;
use serde_derive::Deserialize;
use tokio::sync::mpsc;
use tower_http::trace::TraceLayer;
use tracing_subscriber::EnvFilter;

mod api;
mod emulator;

#[derive(Deserialize)]
struct UserMessage {
    message: String,
}

#[tracing::instrument(skip(socket, twitch_tx))]
async fn handle_socket(mut socket: WebSocket, twitch_tx: mpsc::Sender<TwitchThreadCommand>) {
    let handle = emulator::get_handle(twitch_tx.clone()).await;

    if let Err(err) = handle {
        tracing::warn!("Failed to retrieve Twitch emulator handle: {}", err);
        return;
    }

    let mut handle = handle.unwrap();

    loop {
        tokio::select!(
            msg = socket.recv() => {
                if let Some(msg) = msg {
                    match msg {
                        Err(err) => {
                            tracing::warn!("Failed to read from WebSocket: {}", err);
                            break;
                        }
                        Ok(msg) => match msg {
                            Message::Text(text) => {
                                let user_msg: Option<UserMessage> = serde_json::from_str(&text).ok();
                                if let Some(UserMessage { message }) = user_msg {
                                    emulator::send_message(twitch_tx.clone(), message).await;
                                }
                            }
                            Message::Close(_) => {
                                break;
                            }
                            _ => {}
                        },
                    }
                }
            },
            chat_msg = handle.recv() => {
                match chat_msg {
                    Ok(chat_msg) => {
                        let msg = serde_json::to_string(&chat_msg);

                        if let Ok(msg) = msg {
                            if socket.send(Message::Text(msg)).await.is_err() {
                                tracing::warn!("Failed to send chat message from Twitch emulator through WebSocket");
                            };
                        }
                    },
                    Err(err) => {
                        tracing::warn!("Failed to receive chat message from Twitch emulator: {}", err);
                    }
                }
            }
        )
    }
}

async fn websocket(
    ws: WebSocketUpgrade,
    twitch_tx: Extension<mpsc::Sender<TwitchThreadCommand>>,
) -> impl IntoResponse {
    ws.on_upgrade(|sock| async move { handle_socket(sock, twitch_tx.0).await })
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    color_eyre::install().map_err(Error::msg)?;

    let (tx, rx) = mpsc::channel::<TwitchThreadCommand>(10);
    let handle = tokio::spawn(emulator::task(rx));

    tracing::info!("Starting the app");
    let app = Router::new()
        .route("/", get(websocket))
        .layer(TraceLayer::new_for_http())
        .layer(Extension(tx));

    let (handle_err, server_err) = tokio::join!(
        handle,
        axum::Server::bind(&"0.0.0.0:8080".parse().unwrap()).serve(app.into_make_service()),
    );

    handle_err??;
    server_err?;

    Ok(())
}
