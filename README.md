# Meme Battle Backend

Backend service for the **Meme Battle** game written in Rust. It utilizes Axum, PostgreSQL (via SQLx), and Centrifugo for real-time WebSocket communication.

---

## Running Tests

### The Command

To run the full test suite successfully, **you must execute the tests sequentially using a single thread**:

```bash
cargo test -- --test-threads=1
```

---

## Running Specific Tests

If you only want to run a specific test target, you can target individual integration files:

* **HTTP Router & Game Rules validation**:
  ```bash
  cargo test --test test_game_routes -- --test-threads=1
  ```

* **WebSocket Broadcast & Centrifugo Lobbies channel updates**:
  ```bash
  cargo test --test test_centrifugo_websocket -- --test-threads=1
  ```

* **Full Gameplay flow through Centrifugo loops**:
  ```bash
  cargo test --test test_real_game_centrifugo -- --test-threads=1
  ```
