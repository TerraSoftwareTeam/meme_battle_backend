Backend service for the **Meme Battle** game written in Rust. It utilizes Axum, PostgreSQL (via SQLx), and Centrifugo for real-time WebSocket communication.

## Quick Links

- **Play the Game (Web Client):** [https://meme.skyfly.hackclub.app/](https://meme.skyfly.hackclub.app/)
- **API Documentation (Swagger UI):** [https://api.meme.skyfly.hackclub.app/docs](https://api.meme.skyfly.hackclub.app/docs)
- **Grafana Live Metrics:** [https://grafana.meme.skyfly.hackclub.app/](https://grafana.meme.skyfly.hackclub.app/public-dashboards/3739305316f64458b926c1ee294ec394)

---

## Architecture Overview

### 1. Centrifugo (Real-Time WebSockets)
* **Why it's used:** Manages live player connections so the Rust backend doesn't have to keep thousands of WebSockets open in memory.
* **How it works:**
  * Handles client WebSocket connections and authentication tokens.
  * **Channels:**
    * `lobbies` — Global channel for lobby discovery (`LobbyCreated`, `LobbyUpdated`, `LobbyRemoved`).
    * `game:{game_id}` — Room channel for match events (`PlayerJoined`, `PlayerLeft`, `PlayerReadyChanged`, `GameStarted`, `RoundStarted`, `SubmissionReceived`, `RoundPhaseChanged`, `VoteReceived`, `RoundFinished`, `GameFinished`). Timers are calculated by clients from `ends_at` timestamps sent in phase events.
    * `personal:#{user_id}` — Private channel for user-specific events (e.g. `HandUpdated` with dealt cards).
  * **Reliable Delivery (Transactional Outbox):** Events are saved to a PostgreSQL `realtime_outbox` table in the same transaction as game state changes. A background worker picks them up via `LISTEN/NOTIFY` and sends them to Centrifugo with automatic retries if Centrifugo is temporarily unreachable.

### 2. PostgreSQL (Database)
* **Why it's used:** Stores all persistent data reliably with strict consistency.
* **How it works:**
  * Stores user accounts, passwords (Argon2 hashes), meme/situation card packs, active games, and match scores.
  * Uses database row locking (`FOR UPDATE`) to keep state safe, plus worker claims (`claimed_at`, `claimed_by`) prepared for future horizontal scaling if multiple worker instances are added to process round timeouts.

### 3. Observability Stack (Loki, Tempo, Grafana)
* **OpenTelemetry Collector:** Gathers logs and traces from the backend and forwards them to Loki and Tempo.
* **Grafana Loki:** Collects backend logs with extra metadata (`latency_ms`, `status_code`, `client_ip`, `request_id`).
* **Grafana Tempo:** Tracks each request from start to finish using a unique `trace_id`.
* **Grafana:** Displays live metrics on the dashboard.

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
