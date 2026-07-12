use axum::http::StatusCode;
use futures_util::{SinkExt, StreamExt};
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
    },
    features::game::api::dto::{ReadyRequest, WsTokenDto},
};

#[tokio::test]
async fn test_centrifugo_websocket_connection_and_broadcast() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Load configuration
    let config = Config::from_env().unwrap();

    // 2. Connect to the test DB and run migrations
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    // 4. Start the application router on an ephemeral port
    let state = build_app_state(pool.clone(), config.clone());

    let test_start = chrono::Utc::now();
    sqlx::query("DELETE FROM games WHERE created_at < $1")
        .bind(test_start)
        .execute(&pool)
        .await
        .unwrap();

    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    // 3. Start publisher outbox processor in tests
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let processor_handle = state
        .realtime
        .processor
        .clone()
        .start(pool.clone(), shutdown_rx);

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // 5. Register 3 Guest Users
    let mut tokens = Vec::new();
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
        tokens.push(token);
    }

    // 6. Create Game with first player
    let create_game_payload = json!({
        "mode": "situation_to_meme",
        "selected_situation_pack_ids": Vec::<Uuid>::new(),
        "selected_meme_pack_ids": Vec::<Uuid>::new(),
        "max_rounds": 3,
        "hand_size": 5
    });
    let create_game_resp = client
        .post(format!("{}/games", base_url))
        .bearer_auth(&tokens[0])
        .json(&create_game_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_game_resp.status(), StatusCode::OK);
    let game_resp: RestApiResponse<Value> = create_game_resp.json().await.unwrap();
    let game_id = Uuid::parse_str(
        game_resp
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // 7. Join other players to the lobby
    for token in &tokens[1..] {
        let join_resp = client
            .post(format!("{}/games/{}/join", base_url, game_id))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        assert_eq!(join_resp.status(), StatusCode::OK);
    }

    // 8. Retrieve WebSocket tokens for Player 1
    let token_resp = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .bearer_auth(&tokens[0])
        .send()
        .await
        .unwrap();
    assert_eq!(token_resp.status(), StatusCode::OK);
    let token_body: RestApiResponse<WsTokenDto> = token_resp.json().await.unwrap();
    let ws_tokens = token_body.0.data.unwrap();

    // 9. Connect Player 1 via WebSocket to Centrifugo
    let centrifugo_ws_url = "ws://127.0.0.1:8000/connection/websocket";
    let (ws_stream, _) = connect_async(centrifugo_ws_url)
        .await
        .expect("Failed to connect to Centrifugo WS");
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // 10. Send Connect frame
    let connect_cmd = json!({
        "connect": {
            "token": ws_tokens.connection_token
        },
        "id": 1
    });
    ws_write
        .send(Message::Text(connect_cmd.to_string().into()))
        .await
        .unwrap();

    // Read connect response
    let msg = ws_read.next().await.unwrap().unwrap();
    let resp: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert!(
        resp.get("connect").is_some(),
        "Expected connect response, got: {:?}",
        resp
    );

    // 11. Send Subscribe frame for the game channel
    let subscribe_cmd = json!({
        "subscribe": {
            "channel": format!("game:{}", game_id),
            "token": ws_tokens.game_subscription_token
        },
        "id": 2
    });
    ws_write
        .send(Message::Text(subscribe_cmd.to_string().into()))
        .await
        .unwrap();

    // Read subscribe response
    let msg = ws_read.next().await.unwrap().unwrap();
    let resp: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert!(
        resp.get("subscribe").is_some(),
        "Expected subscribe response, got: {:?}",
        resp
    );

    // 12. Trigger a gameplay action: Player 2 sets ready to true
    let ready_resp = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&tokens[1])
        .json(&ReadyRequest { is_ready: true })
        .send()
        .await
        .unwrap();
    assert_eq!(ready_resp.status(), StatusCode::OK);

    // 13. Wait for the broadcast message on Player 1's WebSocket
    let mut received_ready_broadcast = false;
    for _ in 0..20 {
        // Read next message (might be a reply to sub, or pub event)
        if let Some(Ok(Message::Text(text))) = ws_read.next().await {
            println!("WS RECEIVED: {}", text);
            if let Ok(msg_val) = serde_json::from_str::<Value>(&text) {
                // Centrifugo push messages contain a "push" field
                if let Some(push) = msg_val.get("push") {
                    if let Some(pub_data) = push.get("pub") {
                        if let Some(data) = pub_data.get("data") {
                            if let Some(event_type) = data.get("event_type") {
                                if event_type.as_str() == Some("player_ready_changed") {
                                    let payload = data.get("payload").unwrap();
                                    assert_eq!(
                                        payload.get("is_ready").unwrap().as_bool(),
                                        Some(true)
                                    );
                                    received_ready_broadcast = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    assert!(
        received_ready_broadcast,
        "Did not receive PlayerReadyChanged event via WebSocket"
    );

    // Clean up processor
    shutdown_tx.send(true).unwrap();
    let _ = processor_handle.await;
}

#[tokio::test]
async fn test_ws_token_endpoint_auth_and_permissions() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Load configuration
    let config = Config::from_env().unwrap();

    // 2. Connect to the test DB and run migrations
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    // 3. Start the application router on an ephemeral port
    let state = build_app_state(pool.clone(), config.clone());

    let test_start = chrono::Utc::now();
    sqlx::query("DELETE FROM games WHERE created_at < $1")
        .bind(test_start)
        .execute(&pool)
        .await
        .unwrap();

    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // Register Player 1 & Player 2
    let resp1 = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    let body1: RestApiResponse<Value> = resp1.json().await.unwrap();
    let token1 = body1
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let resp2 = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    let body2: RestApiResponse<Value> = resp2.json().await.unwrap();
    let token2 = body2
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Create a game with Player 1 (Player 2 is NOT joined yet)
    let create_game_payload = json!({
        "mode": "situation_to_meme",
        "selected_situation_pack_ids": Vec::<Uuid>::new(),
        "selected_meme_pack_ids": Vec::<Uuid>::new(),
        "max_rounds": 3,
        "hand_size": 5
    });
    let create_game_resp = client
        .post(format!("{}/games", base_url))
        .bearer_auth(&token1)
        .json(&create_game_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_game_resp.status(), StatusCode::OK);
    let game_resp: RestApiResponse<Value> = create_game_resp.json().await.unwrap();
    let game_id = Uuid::parse_str(
        game_resp
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // Test Case A: Get WS token without auth header -> should fail with 401 Unauthorized
    let resp_no_auth = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_no_auth.status(), StatusCode::UNAUTHORIZED);

    // Test Case B: Get WS token with invalid token -> should fail with 401 Unauthorized
    let resp_bad_token = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .bearer_auth("invalid-token-string")
        .send()
        .await
        .unwrap();
    assert_eq!(resp_bad_token.status(), StatusCode::UNAUTHORIZED);

    // Test Case C: Get WS token for non-existent game -> should fail with 404 Not Found
    let random_game_id = Uuid::new_v4();
    let resp_not_found = client
        .get(format!("{}/games/events/{}/ws-token", base_url, random_game_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_not_found.status(), StatusCode::NOT_FOUND);

    // Test Case D: Get WS token for game Player 2 is not joined to -> should fail with 403 Forbidden
    let resp_forbidden = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .bearer_auth(&token2)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_forbidden.status(), StatusCode::FORBIDDEN);

    // Now let Player 2 join the game
    let join_resp = client
        .post(format!("{}/games/{}/join", base_url, game_id))
        .bearer_auth(&token2)
        .send()
        .await
        .unwrap();
    assert_eq!(join_resp.status(), StatusCode::OK);

    // Test Case E: Get WS token for game Player 2 is now joined to -> should succeed
    let resp_success = client
        .get(format!("{}/games/events/{}/ws-token", base_url, game_id))
        .bearer_auth(&token2)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_success.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_lobbies_realtime_websocket_updates() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Load configuration
    let config = Config::from_env().unwrap();

    // 2. Connect to the test DB and run migrations
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    // 3. Start the application router on an ephemeral port
    let state = build_app_state(pool.clone(), config.clone());

    let test_start = chrono::Utc::now();
    sqlx::query("DELETE FROM games WHERE created_at < $1")
        .bind(test_start)
        .execute(&pool)
        .await
        .unwrap();

    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    let app = create_router(state.clone());
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    // Start publisher outbox processor in tests
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let processor_handle = state
        .realtime
        .processor
        .clone()
        .start(pool.clone(), shutdown_rx);

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // Register 2 Guest Users
    let resp1 = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let body1: RestApiResponse<Value> = resp1.json().await.unwrap();
    let token1 = body1
        .0
        .data
        .as_ref()
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let user_id1: Uuid =
        sqlx::query_scalar("SELECT id FROM users ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();

    let resp2 = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let body2: RestApiResponse<Value> = resp2.json().await.unwrap();
    let token2 = body2
        .0
        .data
        .as_ref()
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let _: Uuid = sqlx::query_scalar("SELECT id FROM users ORDER BY created_at DESC LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    let resp3 = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);
    let body3: RestApiResponse<Value> = resp3.json().await.unwrap();
    let token3 = body3
        .0
        .data
        .as_ref()
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let _: Uuid = sqlx::query_scalar("SELECT id FROM users ORDER BY created_at DESC LIMIT 1")
        .fetch_one(&pool)
        .await
        .unwrap();

    // 4. Setup dummy media assets in DB for Player 1
    for id in 4001..=4024 {
        sqlx::query(
            "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
             VALUES ($1, $2, 'hackclub_cdn', $3, $4, $5, 'image/png', 1024, 'pending', 'private')
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(id as i64)
        .bind(user_id1)
        .bind(format!("prov_websocket_id_{}", id))
        .bind(format!("https://example.com/websocket_{}.png", id))
        .bind(format!("meme_websocket_{}.png", id))
        .execute(&pool)
        .await
        .unwrap();
    }

    // 5. Create Meme Pack
    let meme_pack_payload = json!({
        "name": "Websocket Meme Pack",
        "description": "Description",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": false,
        "media_ids": (4001..=4024).collect::<Vec<i64>>()
    });
    let create_meme_resp = client
        .post(format!("{}/games/packs/memes", base_url))
        .bearer_auth(&token1)
        .json(&meme_pack_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_meme_resp.status(), StatusCode::OK);
    let meme_pack_resp: RestApiResponse<Value> = create_meme_resp.json().await.unwrap();
    let meme_pack_id = Uuid::parse_str(
        meme_pack_resp
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // 6. Create Situation Pack
    let sit_pack_payload = json!({
        "name": "Websocket Situation Pack",
        "description": "Description",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": false,
        "prompts": vec![
            "When we write websocket tests".to_string(),
            "When we assert lobby delete".to_string(),
            "When all checks pass".to_string()
        ]
    });
    let create_sit_resp = client
        .post(format!("{}/games/packs/situations", base_url))
        .bearer_auth(&token1)
        .json(&sit_pack_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_sit_resp.status(), StatusCode::OK);
    let sit_pack_resp: RestApiResponse<Value> = create_sit_resp.json().await.unwrap();
    let sit_pack_id = Uuid::parse_str(
        sit_pack_resp
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // 7. Player 1 fetches catalog first to obtain connection and lobbies subscription tokens
    let get_ws_token_resp = client
        .get(format!("{}/games/catalog/ws-token", base_url))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(get_ws_token_resp.status(), StatusCode::OK);
    let ws_token_body: RestApiResponse<Value> = get_ws_token_resp.json().await.unwrap();
    let ws_token_data = ws_token_body.0.data.unwrap();

    let connection_token = ws_token_data
        .get("connection_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let lobbies_subscription_token = ws_token_data
        .get("lobbies_subscription_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // 8. Connect Player 1 via WebSocket to Centrifugo and subscribe to 'lobbies' channel
    let centrifugo_ws_url = "ws://127.0.0.1:8000/connection/websocket";
    let (ws_stream, _) = connect_async(centrifugo_ws_url)
        .await
        .expect("Failed to connect to Centrifugo WS");
    let (mut ws_write, mut ws_read) = ws_stream.split();

    // Connect
    let connect_cmd = json!({
        "connect": {
            "token": connection_token
        },
        "id": 1
    });
    ws_write
        .send(Message::Text(connect_cmd.to_string().into()))
        .await
        .unwrap();
    let msg = ws_read.next().await.unwrap().unwrap();
    let resp: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert!(resp.get("connect").is_some());

    // Subscribe to lobbies channel
    let subscribe_cmd = json!({
        "subscribe": {
            "channel": "lobbies",
            "token": lobbies_subscription_token
        },
        "id": 2
    });
    ws_write
        .send(Message::Text(subscribe_cmd.to_string().into()))
        .await
        .unwrap();
    let msg = ws_read.next().await.unwrap().unwrap();
    let resp: Value = serde_json::from_str(msg.to_text().unwrap()).unwrap();
    assert!(resp.get("subscribe").is_some());

    // 9. Create Game with Player 1 (Trigger lobby_created)
    let create_game_payload = json!({
        "mode": "situation_to_meme",
        "selected_situation_pack_ids": vec![sit_pack_id],
        "selected_meme_pack_ids": vec![meme_pack_id],
        "max_rounds": 3,
        "hand_size": 5
    });
    let create_game_resp = client
        .post(format!("{}/games", base_url))
        .bearer_auth(&token1)
        .json(&create_game_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_game_resp.status(), StatusCode::OK);
    let game_resp: RestApiResponse<Value> = create_game_resp.json().await.unwrap();
    let game_id = Uuid::parse_str(
        game_resp
            .0
            .data
            .unwrap()
            .get("id")
            .unwrap()
            .as_str()
            .unwrap(),
    )
    .unwrap();

    // 10. Read WebSocket events for lobby_created
    let mut received_created = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(text))) = ws_read.next().await {
            println!("LOBBY WS RECEIVED: {}", text);
            if let Ok(msg_val) = serde_json::from_str::<Value>(&text) {
                if let Some(push) = msg_val.get("push") {
                    if let Some(pub_data) = push.get("pub") {
                        if let Some(data) = pub_data.get("data") {
                            if let Some(event_type) = data.get("event_type") {
                                if event_type.as_str() == Some("lobby_created") {
                                    let payload = data.get("payload").unwrap();
                                    assert_eq!(
                                        payload.get("id").unwrap().as_str().unwrap(),
                                        game_id.to_string()
                                    );
                                    assert_eq!(
                                        payload.get("players_count").unwrap().as_i64(),
                                        Some(1)
                                    );
                                    received_created = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        received_created,
        "Did not receive lobby_created event via WebSocket"
    );

    // 11. Player 2 joins the game (Trigger lobby_updated to 2)
    let join_resp2 = client
        .post(format!("{}/games/{}/join", base_url, game_id))
        .bearer_auth(&token2)
        .send()
        .await
        .unwrap();
    assert_eq!(join_resp2.status(), StatusCode::OK);

    // 12. Player 3 joins the game (Trigger lobby_updated to 3)
    let join_resp3 = client
        .post(format!("{}/games/{}/join", base_url, game_id))
        .bearer_auth(&token3)
        .send()
        .await
        .unwrap();
    assert_eq!(join_resp3.status(), StatusCode::OK);

    // Read WebSocket events for lobby_updated to 2 and 3
    let mut received_updated_2 = false;
    let mut received_updated_3 = false;
    for _ in 0..30 {
        if let Some(Ok(Message::Text(text))) = ws_read.next().await {
            println!("LOBBY WS RECEIVED: {}", text);
            if let Ok(msg_val) = serde_json::from_str::<Value>(&text) {
                if let Some(push) = msg_val.get("push") {
                    if let Some(pub_data) = push.get("pub") {
                        if let Some(data) = pub_data.get("data") {
                            if let Some(event_type) = data.get("event_type") {
                                if event_type.as_str() == Some("lobby_updated") {
                                    let payload = data.get("payload").unwrap();
                                    assert_eq!(
                                        payload.get("id").unwrap().as_str().unwrap(),
                                        game_id.to_string()
                                    );
                                    let count =
                                        payload.get("players_count").unwrap().as_i64().unwrap();
                                    if count == 2 {
                                        received_updated_2 = true;
                                    } else if count == 3 {
                                        received_updated_3 = true;
                                    }
                                    if received_updated_2 && received_updated_3 {
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        received_updated_2,
        "Did not receive lobby_updated (2) event via WebSocket"
    );
    assert!(
        received_updated_3,
        "Did not receive lobby_updated (3) event via WebSocket"
    );

    // 13. Player 1 ready up
    let ready_resp1 = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&token1)
        .json(&json!({ "is_ready": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(ready_resp1.status(), StatusCode::OK);

    // 14. Player 2 ready up
    let ready_resp2 = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&token2)
        .json(&json!({ "is_ready": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(ready_resp2.status(), StatusCode::OK);

    // 15. Player 3 ready up
    let ready_resp3 = client
        .post(format!("{}/games/{}/ready", base_url, game_id))
        .bearer_auth(&token3)
        .json(&json!({ "is_ready": true }))
        .send()
        .await
        .unwrap();
    assert_eq!(ready_resp3.status(), StatusCode::OK);

    // 16. Player 1 starts the game session (Trigger lobby_removed)
    let start_resp = client
        .post(format!("{}/games/{}/start", base_url, game_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    let start_status = start_resp.status();
    let start_text = start_resp.text().await.unwrap();
    assert_eq!(
        start_status,
        StatusCode::OK,
        "Start game failed: {}",
        start_text
    );

    // 17. Read WebSocket events for lobby_removed
    let mut received_removed = false;
    for _ in 0..20 {
        if let Some(Ok(Message::Text(text))) = ws_read.next().await {
            println!("LOBBY WS RECEIVED: {}", text);
            if let Ok(msg_val) = serde_json::from_str::<Value>(&text) {
                if let Some(push) = msg_val.get("push") {
                    if let Some(pub_data) = push.get("pub") {
                        if let Some(data) = pub_data.get("data") {
                            if let Some(event_type) = data.get("event_type") {
                                if event_type.as_str() == Some("lobby_removed") {
                                    let payload = data.get("payload").unwrap();
                                    assert_eq!(
                                        payload.get("id").unwrap().as_str().unwrap(),
                                        game_id.to_string()
                                    );
                                    received_removed = true;
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(
        received_removed,
        "Did not receive lobby_removed event via WebSocket"
    );

    // Clean up processor
    shutdown_tx.send(true).unwrap();
    let _ = processor_handle.await;
}

#[tokio::test]
async fn test_large_media_upload_limit() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
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

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // Register guest user
    let resp = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
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

    // Prepare a mock file of 4 MB (exceeds default 2MB limit but is within 35MB MAX_UPLOAD_SIZE_BYTES)
    let four_mb = 4 * 1024 * 1024;
    let dummy_data = vec![0u8; four_mb];
    let part = reqwest::multipart::Part::bytes(dummy_data)
        .file_name("mock_large_image.png")
        .mime_str("image/png")
        .unwrap();
    let form = reqwest::multipart::Form::new().part("file", part);

    let upload_resp = client
        .post(format!("{}/media/upload/image", base_url))
        .bearer_auth(&token)
        .multipart(form)
        .send()
        .await
        .unwrap();

    // Verify it is not rejected by Axum body limit layer (which would return 413 Payload Too Large)
    let status = upload_resp.status();
    println!("Large upload response status: {}", status);

    assert_ne!(status, StatusCode::PAYLOAD_TOO_LARGE);
}
