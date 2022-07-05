# Twitch Chat Replica

Small, WebSocket-based Twitch chat replica based on [Axum](https://github.com/tokio-rs/axum). Written as a help to exercises for my friend who learns how to program, as well as a practice in Rust async code.

## Installation

You need to have [Rust toolchain](https://rustup.rs/) installed. The app has been tested on Rust 1.62.0 stable.

```
git clone https://github.com/Killavus/twitch-chat-replica.git
cd twitch-chat-replica
cargo build # --release if building for production
# or to run directly:
RUST_LOG="server=debug,tower_http=debug" LISTEN_ADDR="127.0.0.1:8080" cargo run
```

## Configuration

Configuration is done via environment variables:

- `RUST_LOG` - set up logging verbosity. `server=debug,tower_http=debug` is a good default for this variable.
- `LISTEN_ADDR` - on which host/port the server should listen. Defaults to `0.0.0.0:8080` if not provided.

## Crates

- `generator` - simple fake Twitch messages generator. It provides one public method `generate_message` which is used to get a randomly generated message from pool of possible messages.
- `server` - Axum-based service responsible for exposing WebSocket interface of the chat. It manages Twitch emulator task (responsible for generating messages in random intervals and quantity) and WebSocket server.

## License

MIT. See [LICENSE](./LICENSE) for details.
