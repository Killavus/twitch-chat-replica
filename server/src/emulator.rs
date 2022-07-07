use anyhow::{Error, Result};
use generator::TwitchMessage;
use rand::prelude::*;
use std::sync::{
    atomic::{self, AtomicUsize},
    Arc,
};
use std::time::Duration;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::sleep;

#[derive(Debug)]
pub enum TwitchEmulatorCommand {
    NewSubscriber(oneshot::Sender<TwitchHandle>),
    UserMessage(String),
}

#[derive(Debug)]
pub struct TwitchHandle {
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

#[tracing::instrument(skip(twitch_tx))]
pub async fn get_handle(twitch_tx: mpsc::Sender<TwitchEmulatorCommand>) -> Result<TwitchHandle> {
    let (tx, rx) = oneshot::channel::<TwitchHandle>();
    twitch_tx
        .send(TwitchEmulatorCommand::NewSubscriber(tx))
        .await?;
    rx.await.map_err(Error::msg)
}

#[tracing::instrument(skip(twitch_tx))]
pub async fn send_message(twitch_tx: mpsc::Sender<TwitchEmulatorCommand>, message: String) {
    if (twitch_tx
        .send(TwitchEmulatorCommand::UserMessage(message))
        .await)
        .is_err()
    {
        tracing::warn!("Failed to send message to Twitch chat emulator.");
    }
}

#[tracing::instrument(skip(cmd_rx))]
pub async fn task(mut cmd_rx: mpsc::Receiver<TwitchEmulatorCommand>) -> Result<()> {
    let (tx, _) = broadcast::channel::<TwitchMessage>(100);
    let (gen_tx, mut gen_rx) = mpsc::channel::<TwitchMessage>(100);
    let num_subscribers = Arc::new(AtomicUsize::new(0));

    let counter = num_subscribers.clone();
    let out_tx = gen_tx.clone();
    let generator_task = tokio::spawn(async move {
        loop {
            let messages;
            let delay_after;

            if counter.load(atomic::Ordering::Relaxed) > 0 {
                {
                    let mut rng = thread_rng();
                    messages = (1..=5).choose(&mut rng).expect("non-empty messages range");
                }

                for _ in 0..messages {
                    let message = generator::generate_message();
                    if out_tx.send(message).await.is_err() {
                        tracing::error!(
                            "Possible bug - trying to send when there is no subscribers at all."
                        );
                        break;
                    };
                }
            }

            {
                let mut rng = thread_rng();
                delay_after = (100..=1500)
                    .choose(&mut rng)
                    .expect("non-empty delay range");
            }

            sleep(Duration::from_millis(delay_after)).await;
        }
    });

    loop {
        tokio::select! {
            command = cmd_rx.recv() => {
                match command {
                    None => { break; }
                    Some(command) => {
                        match command {
                            TwitchEmulatorCommand::NewSubscriber(chan) => {
                                num_subscribers.fetch_add(1, atomic::Ordering::AcqRel);
                                if chan.send(TwitchHandle::new(tx.clone().subscribe(), num_subscribers.clone())).is_err() {
                                    tracing::warn!("Failed to send twitch handle to subscriber.");
                                };
                            }
                            TwitchEmulatorCommand::UserMessage(msg) => {
                                if gen_tx.send(TwitchMessage::Owned { message: msg, author: "szczenaTheMonke" }).await.is_err() {
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
