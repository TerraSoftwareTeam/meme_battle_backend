use axum::http::StatusCode;
use futures_util::{SinkExt, StreamExt};
use jsonwebtoken::decode;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
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
    features::game::api::dto::{CreateGameRequest, ReadyRequest, WsTokenDto},
};

// Helpers & Types adapted from test_real_game_centrifugo.rs
type WsWriter = futures_util::stream::SplitSink<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Message,
>;
type WsReader = futures_util::stream::SplitStream<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
>;

struct WsClient {
    writer: WsWriter,
    reader: WsReader,
    #[allow(dead_code)]
    user_id: Uuid,
}

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

/// Drain up to `max_msgs` Text frames looking for one matching predicate.
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
            Some(Ok(_)) => {}
            None | Some(Err(_)) => break,
        }
    }
    panic!("{}", fail_msg);
}

fn push_event_type(val: &Value) -> Option<&str> {
    val.get("push")?
        .get("pub")?
        .get("data")?
        .get("event_type")?
        .as_str()
}

fn push_channel(val: &Value) -> Option<&str> {
    val.get("push")?.get("channel")?.as_str()
}

fn is_event_on_channel(val: &Value, event_type: &str, channel: &str) -> bool {
    push_channel(val) == Some(channel) && push_event_type(val) == Some(event_type)
}

/// Extract Centrifugo publication position metadata (offset) from a push message
fn get_pub_offset(val: &Value) -> u64 {
    let pub_obj = val.get("push").unwrap().get("pub").unwrap();
    pub_obj.get("offset").unwrap().as_u64().unwrap()
}

/// Connect to Centrifugo via WebSocket and authenticate (returns connected client)
async fn connect_ws_client(
    centrifugo_ws_url: &str,
    ws_tokens: &WsTokenDto,
    user_id: Uuid,
    id_base: usize,
) -> WsClient {
    let (ws_stream, _) = connect_async(centrifugo_ws_url)
        .await
        .expect("Failed to connect to Centrifugo WS");
    let (mut writer, mut reader) = ws_stream.split();

    // Connect frame
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

    WsClient {
        writer,
        reader,
        user_id,
    }
}

/// Subscribe to a channel
async fn subscribe_to_channel(
    client: &mut WsClient,
    channel: &str,
    token: &str,
    id_base: usize,
) -> Value {
    let sub_id = id_base as u64;
    client
        .writer
        .send(Message::Text(
            json!({
                "subscribe": {
                    "channel": channel,
                    "token": token
                },
                "id": sub_id
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();
    let resp = wait_for_reply(&mut client.reader, sub_id, 10).await;
    assert!(
        resp.get("subscribe").is_some()
            || resp
                .get("error")
                .and_then(|e| e.get("code"))
                .and_then(|c| c.as_i64())
                .map(|code| code == 105)
                .unwrap_or(false),
        "Expected subscribe success, got: {:?}",
        resp
    );
    resp
}

#[tokio::test]
async fn test_centrifugo_websocket_replication_and_recovery() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // ── 1. Setup Backend Server ───────────────────────────────────────────────
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
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.realtime.processor.clone().start(pool.clone(), shutdown_rx);

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);
    let centrifugo_ws_url = "ws://127.0.0.1:8000/connection/websocket";

    // ── 2. Register 3 Users ───────────────────────────────────────────────────
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

    // ── 3. Upload Media & Create Packs ────────────────────────────────────────
    let mut media_ids = Vec::new();
    for i in 0..6 {
        let filepath = if i % 2 == 0 {
            "test_assets/cat.png"
        } else {
            "test_assets/dog.png"
        };
        media_ids.push(send_multipart_upload(&client, &base_url, &tokens[0], filepath).await);
    }

    // Create Meme Pack
    let pack_resp = client
        .post(format!("{}/games/packs/memes", base_url))
        .bearer_auth(&tokens[0])
        .json(&json!({
            "name": "Recovery Test Meme Pack",
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

    // Create Situation Pack
    let sit_resp = client
        .post(format!("{}/games/packs/situations", base_url))
        .bearer_auth(&tokens[0])
        .json(&json!({
            "name": "Recovery Test Situation Pack",
            "description": "Integration test situations",
            "language_code": "ru",
            "safety_level": "family_friendly",
            "is_public": true,
            "prompts": [
                "Когда отвалился вебсокет",
                "Когда восстановилось соединение"
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

    // ── 4. Create Game & Join ─────────────────────────────────────────────────
    let game_resp = client
        .post(format!("{}/games", base_url))
        .bearer_auth(&tokens[0])
        .json(&CreateGameRequest {
            mode: meme_battle_backend::features::game::GameMode::SituationToMeme,
            selected_situation_pack_ids: vec![sit_pack_id],
            selected_meme_pack_ids: vec![meme_pack_id],
            max_rounds: 1,
            hand_size: 1,
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

    // Join Player 2 and 3
    for token in &tokens[1..] {
        let resp = client
            .post(format!("{}/games/{}/join", base_url, game_id))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // ── 5. Connect Player 1 and Player 2 ──────────────────────────────────────
    let game_channel = format!("game:{}", game_id);

    // WS tokens for Player 1
    let token_resp1 = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .bearer_auth(&tokens[0])
        .send()
        .await
        .unwrap();
    let ws_tokens1: WsTokenDto = token_resp1
        .json::<RestApiResponse<WsTokenDto>>()
        .await
        .unwrap()
        .0
        .data
        .unwrap();

    let mut ws_client1 = connect_ws_client(centrifugo_ws_url, &ws_tokens1, user_ids[0], 100).await;
    subscribe_to_channel(
        &mut ws_client1,
        &game_channel,
        &ws_tokens1.game_subscription_token,
        101,
    )
    .await;

    // WS tokens for Player 2
    let token_resp2 = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .bearer_auth(&tokens[1])
        .send()
        .await
        .unwrap();
    let ws_tokens2: WsTokenDto = token_resp2
        .json::<RestApiResponse<WsTokenDto>>()
        .await
        .unwrap()
        .0
        .data
        .unwrap();

    let mut ws_client2 = connect_ws_client(centrifugo_ws_url, &ws_tokens2, user_ids[1], 200).await;
    let sub_resp2 = subscribe_to_channel(
        &mut ws_client2,
        &game_channel,
        &ws_tokens2.game_subscription_token,
        201,
    )
    .await;
    let sub_obj = sub_resp2.get("subscribe").unwrap();
    let last_epoch = sub_obj.get("epoch").unwrap().as_str().unwrap().to_string();

    // ── 6. Verification of Self-Broadcasting (Creator gets their own events) ──
    // Player 1 sets ready
    let resp = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&tokens[0])
        .json(&ReadyRequest { is_ready: true })
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);

    // Verify Player 1 (creator) receives their own event on their WebSocket
    let _p1_event = expect_event(
        &mut ws_client1.reader,
        10,
        |v| is_event_on_channel(v, "player_ready_changed", &game_channel),
        "Player 1 (creator) did not receive their own ready event",
    )
    .await;

    // Verify Player 2 also receives it
    let p2_event = expect_event(
        &mut ws_client2.reader,
        10,
        |v| is_event_on_channel(v, "player_ready_changed", &game_channel),
        "Player 2 did not receive Player 1's ready event",
    )
    .await;

    // Extract current offset from the publication for Player 2
    let last_offset = get_pub_offset(&p2_event);
    assert!(last_offset > 0);

    // ── 7. Simulate Connection Drop ──────────────────────────────────────────
    // Drop Player 2's socket
    let _ = ws_client2.writer.close().await;
    drop(ws_client2); // connection is fully terminated here

    // ── 8. Publish Events in the Interim (while Player 2 is offline) ──────────
    // Player 2 ready = true (from backend side, we send HTTP requests)
    let resp_p2_ready = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&tokens[1])
        .json(&ReadyRequest { is_ready: true })
        .send()
        .await
        .unwrap();
    assert_eq!(resp_p2_ready.status(), StatusCode::OK);

    // Player 3 ready = true
    let resp_p3_ready = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&tokens[2])
        .json(&ReadyRequest { is_ready: true })
        .send()
        .await
        .unwrap();
    assert_eq!(resp_p3_ready.status(), StatusCode::OK);

    // Host (Player 1) starts the game
    let start_resp = client
        .post(format!("{}/games/{}/start", base_url, game_id))
        .bearer_auth(&tokens[0])
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), StatusCode::OK);

    // Wait slightly to ensure outbox retries / processing completes and messages are written to Centrifugo history log
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

    // ── 9. Simulate Reconnection & Recovery ──────────────────────────────────
    // Player 2 reconnects
    let mut ws_client2_reconnected =
        connect_ws_client(centrifugo_ws_url, &ws_tokens2, user_ids[1], 300).await;

    // Send subscribe request with "recover": true, and the last seen offset/epoch
    let sub_id = 301u64;
    ws_client2_reconnected
        .writer
        .send(Message::Text(
            json!({
                "subscribe": {
                    "channel": game_channel,
                    "token": ws_tokens2.game_subscription_token,
                    "recover": true,
                    "offset": last_offset,
                    "epoch": last_epoch
                },
                "id": sub_id
            })
            .to_string()
            .into(),
        ))
        .await
        .unwrap();

    let sub_reply = wait_for_reply(&mut ws_client2_reconnected.reader, sub_id, 10).await;
    let sub_result = sub_reply
        .get("subscribe")
        .expect("Missing subscribe field in reply");

    println!("DEBUG: sub_result = {:?}", sub_result);

    // Verify recovery was successful
    let recovered = sub_result
        .get("recovered")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    assert!(
        recovered,
        "Centrifugo did not recover the subscription. Result: {:?}",
        sub_result
    );

    // Verify that the publications contains the missed events
    let publications = sub_result
        .get("publications")
        .and_then(|v| v.as_array())
        .expect("Missing publications in recovered subscribe response");

    // We missed:
    // 1. Player 2 ready changed
    // 2. Player 3 ready changed
    // 3. game_started
    // 4. round_started
    // Let's verify we got them in the publications list
    assert_eq!(
        publications.len(),
        4,
        "Expected exactly 4 missed publications, got: {:?}",
        publications
    );

    let event_types: Vec<&str> = publications
        .iter()
        .map(|pub_obj| {
            pub_obj
                .get("data")
                .and_then(|d| d.get("event_type"))
                .and_then(|et| et.as_str())
                .unwrap_or("")
        })
        .collect();

    assert_eq!(
        event_types,
        vec![
            "player_ready_changed",
            "player_ready_changed",
            "game_started",
            "round_started"
        ],
        "Missed publications order/types do not match"
    );

    println!(
        "Success! All missed events were successfully recovered: {:?}",
        event_types
    );
}
