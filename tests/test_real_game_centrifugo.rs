use axum::http::StatusCode;
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::decode;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message};
use uuid::Uuid;

use meme_battle_backend::{
    app::create_router,
    common::{
        app::{
            bootstrap::{build_app_state, run_database_migrations},
            config::Config,
        },
        http::dto::RestApiResponse,
        security::jwt::{Claims, KEYS},
    },
    features::game::api::dto::{
        CreateGameRequest, GameStateDto, ReadyRequest, SubmitCardRequest, VoteRequest, WsTokenDto,
    },
};

// ─────────────────────────────────────────────────────────────────────────────
// Helper types
// ─────────────────────────────────────────────────────────────────────────────

type WsWriter = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsReader = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

#[allow(dead_code)]
struct WsClient {
    writer: WsWriter,
    reader: WsReader,
    user_id: Uuid,
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

async fn send_multipart_upload(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    filepath: &str,
) -> i64 {
    let file_bytes = std::fs::read(filepath).unwrap();
    let filename = filepath.split('/').next_back().unwrap().to_string();
    let part = reqwest::multipart::Part::bytes(file_bytes)
        .file_name(filename)
        .mime_str("image/png")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let resp = client
        .post(format!("{}/media/upload/image", base_url))
        .bearer_auth(token)
        .multipart(form)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: RestApiResponse<Value> = resp.json().await.unwrap();
    body.0.data.unwrap().get("id").unwrap().as_i64().unwrap()
}

/// Wait for a specific reply ID in the WebSocket stream.
/// Only Text frames count toward max_msgs; Ping/Pong/Binary are skipped silently.
async fn wait_for_reply(reader: &mut WsReader, target_id: u64, max_msgs: usize) -> Value {
    let mut text_count = 0;
    loop {
        match reader.next().await {
            Some(Ok(Message::Text(text))) => {
                text_count += 1;
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if val.get("id").and_then(|v| v.as_u64()) == Some(target_id) {
                        return val;
                    }
                }
                if text_count >= max_msgs {
                    break;
                }
            }
            Some(Ok(_)) => {}
            None | Some(Err(_)) => break,
        }
    }
    panic!("Did not receive reply for id={}", target_id);
}

/// Check if a Centrifugo reply indicates success OR "already subscribed" (code 105).
fn is_subscribe_ok(resp: &Value) -> bool {
    if resp.get("subscribe").is_some() {
        return true;
    }
    // code 105 = already subscribed — treat as success since the channel IS active
    resp.get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
        .map(|code| code == 105)
        .unwrap_or(false)
}

/// Connect to Centrifugo via WebSocket, authenticate, subscribe to game and personal channels.
async fn connect_ws_client(
    centrifugo_ws_url: &str,
    game_id: Uuid,
    ws_tokens: &WsTokenDto,
    user_id: Uuid,
    id_base: usize,
) -> WsClient {
    let (ws_stream, _) = connect_async(centrifugo_ws_url)
        .await
        .expect("Failed to connect to Centrifugo WS");
    let (mut writer, mut reader) = ws_stream.split();

    // ── Connect ──────────────────────────────────────────────────────────────
    let connect_id = id_base as u64;
    writer
        .send(Message::Text(
            json!({
                "connect": { "token": ws_tokens.connection_token },
                "id": connect_id
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let resp = wait_for_reply(&mut reader, connect_id, 10).await;
    assert!(
        resp.get("connect").is_some(),
        "Expected connect ack, got: {:?}",
        resp
    );

    // ── Subscribe to game channel ─────────────────────────────────────────────
    let sub_game_id = id_base as u64 + 1;
    writer
        .send(Message::Text(
            json!({
                "subscribe": {
                    "channel": format!("game:{}", game_id),
                    "token": ws_tokens.game_subscription_token
                },
                "id": sub_game_id
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let resp = wait_for_reply(&mut reader, sub_game_id, 10).await;
    assert!(
        is_subscribe_ok(&resp),
        "Expected subscribe ack for game channel, got: {:?}",
        resp
    );

    // ── Subscribe to personal channel ─────────────────────────────────────────
    let sub_personal_id = id_base as u64 + 2;
    writer
        .send(Message::Text(
            json!({
                "subscribe": {
                    "channel": format!("personal:#{}", user_id),
                    "token": ws_tokens.personal_subscription_token
                },
                "id": sub_personal_id
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let resp = wait_for_reply(&mut reader, sub_personal_id, 10).await;
    assert!(
        is_subscribe_ok(&resp),
        "Expected subscribe ack for personal channel, got: {:?}",
        resp
    );

    WsClient {
        writer,
        reader,
        user_id,
    }
}

/// Drain up to `max_msgs` **Text** frames from reader looking for one matching predicate.
/// Non-text frames (Ping, Pong, Binary) are skipped without counting against the budget.
/// Returns the matching JSON value, or panics with `fail_msg` if not found.
async fn expect_event(
    reader: &mut WsReader,
    max_msgs: usize,
    predicate: impl Fn(&Value) -> bool,
    fail_msg: &str,
) -> Value {
    let mut text_count = 0;
    loop {
        match reader.next().await {
            Some(Ok(Message::Text(text))) => {
                text_count += 1;
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if predicate(&val) {
                        return val;
                    }
                }
                if text_count >= max_msgs {
                    break;
                }
            }
            // Ping/Pong/Binary: skip silently, don't count
            Some(Ok(_)) => {}
            // Stream closed or error
            None | Some(Err(_)) => break,
        }
    }
    panic!("{}", fail_msg);
}

/// Collect up to `count` messages matching predicate within `max_msgs` **Text** frames.
/// Non-text frames are skipped without counting against the budget.
async fn collect_events(
    reader: &mut WsReader,
    max_msgs: usize,
    count: usize,
    predicate: impl Fn(&Value) -> bool,
) -> Vec<Value> {
    let mut found = Vec::new();
    let mut text_count = 0;
    loop {
        if found.len() >= count {
            break;
        }
        match reader.next().await {
            Some(Ok(Message::Text(text))) => {
                text_count += 1;
                if let Ok(val) = serde_json::from_str::<Value>(&text) {
                    if predicate(&val) {
                        found.push(val);
                    }
                }
                if text_count >= max_msgs {
                    break;
                }
            }
            Some(Ok(_)) => {}
            None | Some(Err(_)) => break,
        }
    }
    found
}

/// Extract event_type string from a Centrifugo push message
fn push_event_type(val: &Value) -> Option<&str> {
    val.get("push")?
        .get("pub")?
        .get("data")?
        .get("event_type")?
        .as_str()
}

/// Extract channel from a Centrifugo push message
fn push_channel(val: &Value) -> Option<&str> {
    val.get("push")?.get("channel")?.as_str()
}

/// Extract data payload from a Centrifugo push message
fn push_data(val: &Value) -> Option<&Value> {
    val.get("push")?.get("pub")?.get("data")
}

fn is_event_on_channel(val: &Value, event_type: &str, channel: &str) -> bool {
    push_channel(val) == Some(channel) && push_event_type(val) == Some(event_type)
}

async fn submit_card_for_player(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    game_id: Uuid,
    round_id: Uuid,
    card_id: Uuid,
) {
    let resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/submit",
            base_url, game_id, round_id
        ))
        .bearer_auth(token)
        .json(&SubmitCardRequest { card_id })
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Submit card failed: {}",
        resp.text().await.unwrap_or_default()
    );
}

async fn vote_for_other_player(
    client: &reqwest::Client,
    base_url: &str,
    token: &str,
    voter_user_id: Uuid,
    game_id: Uuid,
    round_id: Uuid,
    pool: &PgPool,
) {
    let submissions = sqlx::query("SELECT id, user_id FROM round_submissions WHERE round_id = $1")
        .bind(round_id)
        .fetch_all(pool)
        .await
        .unwrap();

    let target = submissions
        .iter()
        .find(|row| {
            let uid: Uuid = row.get("user_id");
            uid != voter_user_id
        })
        .unwrap();
    let submission_id: Uuid = target.get("id");

    let resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/vote",
            base_url, game_id, round_id
        ))
        .bearer_auth(token)
        .json(&VoteRequest { submission_id })
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "Vote failed: {}",
        resp.text().await.unwrap_or_default()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test: Complete 2-round game with all WebSocket events validated
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_real_game_centrifugo_gameplay_flow() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // ── 1. Setup ──────────────────────────────────────────────────────────────

    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let state = build_app_state(pool.clone(), config.clone());
    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    // Start outbox processor
    state.realtime.processor.clone().start(pool.clone());

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);
    let centrifugo_ws_url = "ws://127.0.0.1:8000/connection/websocket";

    // ── 2. Register 3 guest users ─────────────────────────────────────────────

    let mut tokens = Vec::new();
    let mut user_ids = Vec::new();
    for _ in 0..3 {
        let resp = client
            .post(format!("{}/auth/guest", base_url))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: RestApiResponse<Value> = resp.json().await.unwrap();
        let token = body
            .0
            .data
            .unwrap()
            .get("access_token")
            .unwrap()
            .as_str()
            .unwrap()
            .to_string();

        let claims: Claims =
            decode::<Claims>(&token, &KEYS.decoding, &jsonwebtoken::Validation::default())
                .unwrap()
                .claims;
        user_ids.push(Uuid::parse_str(&claims.sub).unwrap());
        tokens.push(token);
    }

    // ── 3. Upload images & create packs ──────────────────────────────────────

    // Upload 12 images (alternating cat/dog)
    let mut media_ids = Vec::new();
    for i in 0..12 {
        let filepath = if i % 2 == 0 {
            "test_assets/cat.png"
        } else {
            "test_assets/dog.png"
        };
        media_ids.push(send_multipart_upload(&client, &base_url, &tokens[0], filepath).await);
    }

    // Create meme pack
    let pack_resp = client
        .post(format!("{}/games/packs/memes", base_url))
        .bearer_auth(&tokens[0])
        .json(&json!({
            "name": "Test Meme Pack",
            "description": "Integration test pack",
            "language_code": "ru",
            "safety_level": "family_friendly",
            "is_public": true,
            "media_ids": media_ids
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(pack_resp.status(), StatusCode::OK);
    let meme_pack_id = Uuid::parse_str(
        pack_resp
            .json::<RestApiResponse<Value>>()
            .await
            .unwrap()
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // Create situation pack
    let sit_resp = client
        .post(format!("{}/games/packs/situations", base_url))
        .bearer_auth(&tokens[0])
        .json(&json!({
            "name": "Test Situation Pack",
            "description": "Integration test situations",
            "language_code": "ru",
            "safety_level": "family_friendly",
            "is_public": true,
            "prompts": [
                "Когда код скомпилировался с первого раза",
                "Когда прод упал в пятницу вечером",
                "Когда дедлайн завтра а ты только начал"
            ]
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(sit_resp.status(), StatusCode::OK);
    let sit_pack_id = Uuid::parse_str(
        sit_resp
            .json::<RestApiResponse<Value>>()
            .await
            .unwrap()
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // ── 4. Create game (2 rounds, hand_size=2) ────────────────────────────────

    let game_resp = client
        .post(format!("{}/games", base_url))
        .bearer_auth(&tokens[0])
        .json(&CreateGameRequest {
            mode: meme_battle_backend::features::game::GameMode::SituationToMeme,
            selected_situation_pack_ids: vec![sit_pack_id],
            selected_meme_pack_ids: vec![meme_pack_id],
            max_rounds: 2,
            hand_size: 2,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(game_resp.status(), StatusCode::OK);
    let game_id = Uuid::parse_str(
        game_resp
            .json::<RestApiResponse<Value>>()
            .await
            .unwrap()
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // ── 5. Players 2 and 3 join ───────────────────────────────────────────────

    for token in &tokens[1..] {
        let resp = client
            .post(format!("{}/games/{}/join", base_url, game_id))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── 6. All players get WS tokens and connect ──────────────────────────────

    let mut ws_clients: Vec<WsClient> = Vec::new();

    for (idx, (token, user_id)) in tokens.iter().zip(user_ids.iter()).enumerate() {
        let token_resp = client
            .get(format!("{}/games/{}/ws-token", base_url, game_id))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(token_resp.status(), StatusCode::OK);
        let ws_tokens: WsTokenDto = token_resp
            .json::<RestApiResponse<WsTokenDto>>()
            .await
            .unwrap()
            .0
            .data
            .unwrap();

        let ws_client = connect_ws_client(
            centrifugo_ws_url,
            game_id,
            &ws_tokens,
            *user_id,
            idx * 10 + 1,
        )
        .await;
        ws_clients.push(ws_client);
    }

    // ── 7. All players set ready ──────────────────────────────────────────────

    for token in &tokens {
        let resp = client
            .post(format!("{}/games/{}/ready", base_url, game_id))
            .bearer_auth(token)
            .json(&ReadyRequest { is_ready: true })
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // Player 1 should receive 3x player_ready_changed on game channel
    {
        let reader = &mut ws_clients[0].reader;
        let game_channel = format!("game:{}", game_id);
        let events = collect_events(reader, 30, 3, |v| {
            is_event_on_channel(v, "player_ready_changed", &game_channel)
        })
        .await;
        assert_eq!(
            events.len(),
            3,
            "Expected 3 player_ready_changed events on game channel, got {}",
            events.len()
        );

        // Validate payload structure on last event
        let data = push_data(events.last().unwrap()).unwrap();
        assert!(
            data.get("payload").unwrap().get("user_id").is_some(),
            "Missing user_id in player_ready_changed"
        );
        assert!(
            data.get("payload").unwrap().get("is_ready").is_some(),
            "Missing is_ready in player_ready_changed"
        );
    }

    // ── 8. Host starts the game ───────────────────────────────────────────────

    let start_resp = client
        .post(format!("{}/games/{}/start", base_url, game_id))
        .bearer_auth(&tokens[0])
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), StatusCode::OK);

    // Try to join after game has started -> should fail with CONFLICT (409)
    let join_fail_resp = client
        .post(format!("{}/games/{}/join", base_url, game_id))
        .bearer_auth(&tokens[1])
        .send()
        .await
        .unwrap();
    assert_eq!(join_fail_resp.status(), StatusCode::CONFLICT);

    // ── 9. Verify game_started event on game channel (Player 2) ──────────────

    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[1].reader;
        let event = expect_event(
            reader,
            20,
            |v| is_event_on_channel(v, "game_started", &game_channel),
            "Player 2 did not receive game_started on game channel",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(payload.get("rounds_count").unwrap().as_i64(), Some(2));
        assert_eq!(payload.get("hand_size").unwrap().as_i64(), Some(2));
    }

    // ── 10. Verify round_started event on game channel (Player 3) ────────────

    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[2].reader;
        let event = expect_event(
            reader,
            20,
            |v| is_event_on_channel(v, "round_started", &game_channel),
            "Player 3 did not receive round_started on game channel",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(payload.get("round_number").unwrap().as_i64(), Some(1));
        assert_eq!(payload.get("phase").unwrap().as_str(), Some("submitting"));
        assert!(payload.get("round_id").is_some());
    }

    // ── 11. Each player receives hand_updated on their personal channel ───────

    let mut player_hands_r1: Vec<Vec<Uuid>> = Vec::new();

    for (idx, ws_client) in ws_clients.iter_mut().enumerate() {
        let personal_channel = format!("personal:#{}", ws_client.user_id);
        let event = expect_event(
            &mut ws_client.reader,
            30,
            |v| is_event_on_channel(v, "hand_updated", &personal_channel),
            &format!(
                "Player {} did not receive hand_updated on personal channel",
                idx + 1
            ),
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        let cards = payload.get("cards").unwrap().as_array().unwrap();
        assert_eq!(
            cards.len(),
            2,
            "Player {} hand should have 2 cards",
            idx + 1
        );

        // Ensure it came on personal channel (not game channel)
        assert_eq!(
            push_channel(&event),
            Some(personal_channel.as_str()),
            "hand_updated must arrive on personal channel only"
        );

        let hand: Vec<Uuid> = cards
            .iter()
            .map(|c| Uuid::parse_str(c.get("id").unwrap().as_str().unwrap()).unwrap())
            .collect();
        player_hands_r1.push(hand);
    }

    // ── 12. Fetch round ID ────────────────────────────────────────────────────

    let state_resp = client
        .get(format!("{}/games/{}/state", base_url, game_id))
        .bearer_auth(&tokens[0])
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp.status(), StatusCode::OK);
    let game_state = state_resp
        .json::<RestApiResponse<GameStateDto>>()
        .await
        .unwrap()
        .0
        .data
        .unwrap();
    let round1_id = game_state.round.unwrap().id;

    // ── 13. Players submit cards (Round 1) ────────────────────────────────────

    // Try to submit an invalid card (not in hand) -> should fail with BAD_REQUEST (400)
    let invalid_card_resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/submit",
            base_url, game_id, round1_id
        ))
        .bearer_auth(&tokens[0])
        .json(&SubmitCardRequest {
            card_id: Uuid::new_v4(),
        })
        .send()
        .await
        .unwrap();
    assert_eq!(invalid_card_resp.status(), StatusCode::BAD_REQUEST);

    // Players 1 and 2 submit - should not yet trigger phase change
    for idx in 0..2 {
        submit_card_for_player(
            &client,
            &base_url,
            &tokens[idx],
            game_id,
            round1_id,
            player_hands_r1[idx][0],
        )
        .await;
    }

    // Verify submission_received events on game channel after player 1 submits
    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[0].reader;
        let events = collect_events(reader, 15, 2, |v| {
            is_event_on_channel(v, "submission_received", &game_channel)
        })
        .await;
        assert_eq!(
            events.len(),
            2,
            "Expected 2 submission_received events after 2 submissions"
        );

        // Validate payload structure
        let data = push_data(&events[0]).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(
            payload.get("round_id").unwrap().as_str(),
            Some(round1_id.to_string().as_str())
        );
        assert!(payload.get("user_id").is_some());
    }

    // Player 3 submits - triggers phase change to voting
    submit_card_for_player(
        &client,
        &base_url,
        &tokens[2],
        game_id,
        round1_id,
        player_hands_r1[2][0],
    )
    .await;

    // ── 14. Verify round_phase_changed → voting on game channel ──────────────

    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[1].reader;
        let event = expect_event(
            reader,
            20,
            |v| is_event_on_channel(v, "round_phase_changed", &game_channel),
            "Player 2 did not receive round_phase_changed on game channel",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(payload.get("phase").unwrap().as_str(), Some("voting"));
        assert_eq!(
            payload.get("round_id").unwrap().as_str(),
            Some(round1_id.to_string().as_str())
        );
    }

    // Try to submit another card in the voting phase -> should fail with CONFLICT (409)
    let late_submit_resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/submit",
            base_url, game_id, round1_id
        ))
        .bearer_auth(&tokens[0])
        .json(&SubmitCardRequest {
            card_id: player_hands_r1[0][1],
        })
        .send()
        .await
        .unwrap();
    assert_eq!(late_submit_resp.status(), StatusCode::CONFLICT);

    // ── 15. Players vote (Round 1) ────────────────────────────────────────────

    // Try to vote for own card (voter_id = submitter_id) -> should fail with BAD_REQUEST (400)
    let submissions = sqlx::query("SELECT id, user_id FROM round_submissions WHERE round_id = $1")
        .bind(round1_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    let own_submission = submissions
        .iter()
        .find(|row| {
            let uid: Uuid = row.get("user_id");
            uid == user_ids[0]
        })
        .unwrap();
    let own_submission_id: Uuid = own_submission.get("id");

    let own_vote_resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/vote",
            base_url, game_id, round1_id
        ))
        .bearer_auth(&tokens[0])
        .json(&VoteRequest {
            submission_id: own_submission_id,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(own_vote_resp.status(), StatusCode::BAD_REQUEST);

    // Player 1 votes for another player's submission
    let other_submission = submissions
        .iter()
        .find(|row| {
            let uid: Uuid = row.get("user_id");
            uid != user_ids[0]
        })
        .unwrap();
    let other_submission_id: Uuid = other_submission.get("id");

    let vote_resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/vote",
            base_url, game_id, round1_id
        ))
        .bearer_auth(&tokens[0])
        .json(&VoteRequest {
            submission_id: other_submission_id,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(vote_resp.status(), StatusCode::OK);

    // Try to vote twice -> should fail with CONFLICT (409)
    let double_vote_resp = client
        .post(format!(
            "{}/games/{}/rounds/{}/vote",
            base_url, game_id, round1_id
        ))
        .bearer_auth(&tokens[0])
        .json(&VoteRequest {
            submission_id: other_submission_id,
        })
        .send()
        .await
        .unwrap();
    assert_eq!(double_vote_resp.status(), StatusCode::CONFLICT);

    // Player 2 votes
    vote_for_other_player(
        &client,
        &base_url,
        &tokens[1],
        user_ids[1],
        game_id,
        round1_id,
        &pool,
    )
    .await;

    // Verify vote_received events on game channel
    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[2].reader;
        let events = collect_events(reader, 15, 2, |v| {
            is_event_on_channel(v, "vote_received", &game_channel)
        })
        .await;
        assert_eq!(
            events.len(),
            2,
            "Expected 2 vote_received events after 2 votes"
        );

        let data = push_data(&events[0]).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(
            payload.get("round_id").unwrap().as_str(),
            Some(round1_id.to_string().as_str())
        );
        assert!(payload.get("voter_id").is_some());
    }

    // Player 3 votes - triggers round finished
    vote_for_other_player(
        &client,
        &base_url,
        &tokens[2],
        user_ids[2],
        game_id,
        round1_id,
        &pool,
    )
    .await;

    // ── 16. Verify round_finished event (Round 1) on game channel ─────────────

    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[0].reader;
        let event = expect_event(
            reader,
            30,
            |v| is_event_on_channel(v, "round_finished", &game_channel),
            "Player 1 did not receive round_finished for Round 1",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(payload.get("round_number").unwrap().as_i64(), Some(1));
        assert!(
            payload.get("winner_user_id").is_some(),
            "Missing winner_user_id"
        );
        assert!(
            payload.get("scoreboard").unwrap().as_array().is_some(),
            "Missing scoreboard"
        );
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ROUND 2
    // ─────────────────────────────────────────────────────────────────────────

    // ── 17. Each player receives hand_updated for Round 2 on personal channel ─
    // Read hand_updated from all personal channels BEFORE checking round_started.
    // After step 16 consumed round_finished from Player 1, Player 1's buffer has:
    //   [hand_updated P1 (personal), round_started (game)].
    // Reading hand_updated first leaves round_started safely in Player 1's buffer
    // for step 18 without racing against Centrifugo delivery timing.

    let mut player_hands_r2: Vec<Vec<Uuid>> = Vec::new();

    for (idx, ws_client) in ws_clients.iter_mut().enumerate() {
        let personal_channel = format!("personal:#{}", ws_client.user_id);
        let event = expect_event(
            &mut ws_client.reader,
            30,
            |v| is_event_on_channel(v, "hand_updated", &personal_channel),
            &format!(
                "Player {} did not receive hand_updated for Round 2 on personal channel",
                idx + 1
            ),
        )
        .await;

        // Confirm it's on personal channel only
        assert_eq!(
            push_channel(&event),
            Some(personal_channel.as_str()),
            "hand_updated for Round 2 must arrive on personal channel"
        );

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        let cards = payload.get("cards").unwrap().as_array().unwrap();
        assert_eq!(
            cards.len(),
            2,
            "Player {} should have 2 cards in Round 2 hand",
            idx + 1
        );

        let hand: Vec<Uuid> = cards
            .iter()
            .map(|c| Uuid::parse_str(c.get("id").unwrap().as_str().unwrap()).unwrap())
            .collect();
        player_hands_r2.push(hand);
    }

    // ── 19. Fetch Round 2 ID ──────────────────────────────────────────────────

    let state_resp2 = client
        .get(format!("{}/games/{}/state", base_url, game_id))
        .bearer_auth(&tokens[0])
        .send()
        .await
        .unwrap();
    assert_eq!(state_resp2.status(), StatusCode::OK);
    let game_state2 = state_resp2
        .json::<RestApiResponse<GameStateDto>>()
        .await
        .unwrap()
        .0
        .data
        .unwrap();
    let round2_id = game_state2.round.unwrap().id;
    assert_ne!(round2_id, round1_id, "Round 2 ID must differ from Round 1");

    // ── 20. Players submit cards (Round 2) ────────────────────────────────────

    for idx in 0..3 {
        submit_card_for_player(
            &client,
            &base_url,
            &tokens[idx],
            game_id,
            round2_id,
            player_hands_r2[idx][0],
        )
        .await;
    }

    // Verify submission_received × 3 on game channel
    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[0].reader;
        let events = collect_events(reader, 20, 3, |v| {
            is_event_on_channel(v, "submission_received", &game_channel)
        })
        .await;
        assert_eq!(
            events.len(),
            3,
            "Expected 3 submission_received events for Round 2"
        );
    }

    // Verify round_phase_changed → voting for Round 2
    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[2].reader;
        let event = expect_event(
            reader,
            20,
            |v| is_event_on_channel(v, "round_phase_changed", &game_channel),
            "Player 3 did not receive round_phase_changed for Round 2",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(payload.get("phase").unwrap().as_str(), Some("voting"));
    }

    // ── 21. Players vote (Round 2) → triggers round_finished + game_finished ──

    for idx in 0..3 {
        vote_for_other_player(
            &client,
            &base_url,
            &tokens[idx],
            user_ids[idx],
            game_id,
            round2_id,
            &pool,
        )
        .await;
    }

    // ── 22. Verify round_finished for Round 2 ────────────────────────────────

    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[1].reader;
        let event = expect_event(
            reader,
            30,
            |v| is_event_on_channel(v, "round_finished", &game_channel),
            "Player 2 did not receive round_finished for Round 2",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();
        assert_eq!(payload.get("round_number").unwrap().as_i64(), Some(2));
        assert!(payload.get("winner_user_id").is_some());
    }

    // ── 23. Verify game_finished event on game channel ────────────────────────

    {
        let game_channel = format!("game:{}", game_id);
        let reader = &mut ws_clients[0].reader;
        let event = expect_event(
            reader,
            30,
            |v| is_event_on_channel(v, "game_finished", &game_channel),
            "Player 1 did not receive game_finished on game channel",
        )
        .await;

        let data = push_data(&event).unwrap();
        let payload = data.get("payload").unwrap();

        // Validate final scoreboard
        let scoreboard = payload.get("final_scoreboard").unwrap().as_array().unwrap();
        assert_eq!(
            scoreboard.len(),
            3,
            "Final scoreboard must contain all 3 players"
        );
        for entry in scoreboard {
            assert!(
                entry.get("user_id").is_some(),
                "Scoreboard entry missing user_id"
            );
            assert!(
                entry.get("score").is_some(),
                "Scoreboard entry missing score"
            );
        }

        // Winner must be one of the game's players
        let winner_id =
            Uuid::parse_str(payload.get("winner_user_id").unwrap().as_str().unwrap()).unwrap();
        assert!(
            user_ids.contains(&winner_id),
            "game_finished winner_user_id {} is not one of the game players",
            winner_id
        );
    }

    // ── 24. Verify game status in DB ──────────────────────────────────────────

    let row = sqlx::query("SELECT status::text as status FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let status: String = row.get("status");
    assert_eq!(status, "finished", "Game status in DB should be 'finished'");
}
