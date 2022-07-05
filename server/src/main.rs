use anyhow::{Error, Result};
use axum::extract::ws::{Message, WebSocket};
use axum::extract::WebSocketUpgrade;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Extension, Router};
use generator::TwitchMessage;
use rand::prelude::*;
use serde_derive::Deserialize;
use std::sync::atomic::{self, AtomicUsize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio::task::yield_now;
use tokio::time::sleep;
use tower_http::trace::TraceLayer;

use tracing_subscriber::EnvFilter;

#[derive(Debug)]
enum TwitchThreadCommand {
    NewSubscriber(oneshot::Sender<TwitchHandle>),
    UserMessage(String),
}

#[derive(Debug)]
struct TwitchHandle {
    rx: broadcast::Receiver<TwitchMessage>,
    num_subscribers: Arc<AtomicUsize>,
}

impl TwitchHandle {
    pub fn new(rx: broadcast::Receiver<TwitchMessage>, num_subscribers: Arc<AtomicUsize>) -> Self {
        Self {
            rx,
            num_subscribers,
        }
    }

    pub async fn recv(&mut self) -> Result<TwitchMessage, broadcast::error::RecvError> {
        self.rx.recv().await
    }
}

impl Drop for TwitchHandle {
    fn drop(&mut self) {
        self.num_subscribers.fetch_sub(1, atomic::Ordering::AcqRel);
    }
}

#[tracing::instrument(skip(cmd_rx))]
async fn start_twitch_emulator(mut cmd_rx: mpsc::Receiver<TwitchThreadCommand>) -> Result<()> {
    let (tx, _) = broadcast::channel::<TwitchMessage>(100);
    let (gen_tx, mut gen_rx) = mpsc::channel::<TwitchMessage>(100);
    let (msg_tx, mut msg_rx) = mpsc::channel::<String>(100);
    let num_subscribers = Arc::new(AtomicUsize::new(0));

    let counter = num_subscribers.clone();
    let generator_task = tokio::spawn(async move {
        loop {
            if counter.load(atomic::Ordering::Relaxed) > 0 {
                while let Ok(msg) = msg_rx.try_recv() {
                    if gen_tx
                        .send(TwitchMessage::Owned {
                            author: "szczenaTheMonke",
                            message: msg,
                        })
                        .await
                        .is_err()
                    {
                        tracing::warn!("Failed to send user message back to listening sockets");
                    }
                }

                let messages;
                let delay_after;
                {
                    let mut rng = thread_rng();
                    messages = (1..=5).choose(&mut rng).expect("non-empty messages range");
                    delay_after = (100..=1500)
                        .choose(&mut rng)
                        .expect("non-empty delay range");
                }

                for _ in 0..messages {
                    let message = generator::generate_message();
                    if gen_tx.send(message).await.is_err() {
                        tracing::error!(
                            "Possible bug - trying to send when there is no subscribers at all."
                        );
                        break;
                    };
                }

                sleep(Duration::from_millis(delay_after)).await;
            } else {
                yield_now().await
            }
        }
    });

    loop {
        tokio::select! {
            command = cmd_rx.recv() => {
                match command {
                    None => { break; }
                    Some(command) => {
                        match command {
                            TwitchThreadCommand::NewSubscriber(chan) => {
                                num_subscribers.fetch_add(1, atomic::Ordering::AcqRel);
                                if chan.send(TwitchHandle::new(tx.clone().subscribe(), num_subscribers.clone())).is_err() {
                                    tracing::warn!("Failed to send twitch handle to subscriber.");
                                };
                            }
                            TwitchThreadCommand::UserMessage(msg) => {
                                if msg_tx.send(msg).await.is_err() {
                                    tracing::warn!("Failed to send user message from WebSocket to emulator.");
                                };
                            }
                        }
                    }
                }
            },
            message = gen_rx.recv() => {
                match message {
                    Some(message) => {
                        if tx.send(message).is_err() {
                            tracing::warn!("Failed to send twitch message to subscribers - possibly bug in implementation.");
                        };
                    }
                    None => { break; }
                }
            }
        }
    }

    generator_task.await?;
    Ok(())
}

#[derive(Deserialize)]
struct UserMessage {
    message: String,
}

async fn get_handle(twitch_tx: mpsc::Sender<TwitchThreadCommand>) -> Result<TwitchHandle> {
    let (tx, rx) = oneshot::channel::<TwitchHandle>();
    twitch_tx
        .send(TwitchThreadCommand::NewSubscriber(tx))
        .await?;
    rx.await.map_err(Error::msg)
}

#[tracing::instrument(skip(socket, twitch_tx))]
async fn handle_socket(mut socket: WebSocket, twitch_tx: mpsc::Sender<TwitchThreadCommand>) {
    let handle = get_handle(twitch_tx.clone()).await;

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
                                    if (twitch_tx
                                        .send(TwitchThreadCommand::UserMessage(message))
                                        .await)
                                        .is_err()
                                    {
                                        tracing::warn!("Failed to send message to Twitch chat emulator.");
                                    }
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
    let handle = tokio::spawn(start_twitch_emulator(rx));

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
