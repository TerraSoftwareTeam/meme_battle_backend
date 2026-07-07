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
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    state.realtime.processor.clone().start(pool.clone(), shutdown_rx);

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
        .get(format!("{}/games/{}/ws-token", base_url, game_id))
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
    let resp1 = client.post(format!("{}/auth/guest", base_url)).send().await.unwrap();
    let body1: RestApiResponse<Value> = resp1.json().await.unwrap();
    let token1 = body1.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let resp2 = client.post(format!("{}/auth/guest", base_url)).send().await.unwrap();
    let body2: RestApiResponse<Value> = resp2.json().await.unwrap();
    let token2 = body2.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

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
    let game_id = Uuid::parse_str(game_resp.0.data.unwrap().get("id").unwrap().as_str().unwrap()).unwrap();

    // Test Case A: Get WS token without auth header -> should fail with 401 Unauthorized
    let resp_no_auth = client
        .get(format!("{}/games/{}/ws-token", base_url, game_id))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_no_auth.status(), StatusCode::UNAUTHORIZED);

    // Test Case B: Get WS token with invalid token -> should fail with 401 Unauthorized
    let resp_bad_token = client
        .get(format!("{}/games/{}/ws-token", base_url, game_id))
        .bearer_auth("invalid-token-string")
        .send()
        .await
        .unwrap();
    assert_eq!(resp_bad_token.status(), StatusCode::UNAUTHORIZED);

    // Test Case C: Get WS token for non-existent game -> should fail with 404 Not Found
    let random_game_id = Uuid::new_v4();
    let resp_not_found = client
        .get(format!("{}/games/{}/ws-token", base_url, random_game_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_not_found.status(), StatusCode::NOT_FOUND);

    // Test Case D: Get WS token for game Player 2 is not joined to -> should fail with 403 Forbidden
    let resp_forbidden = client
        .get(format!("{}/games/{}/ws-token", base_url, game_id))
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
        .get(format!("{}/games/{}/ws-token", base_url, game_id))
        .bearer_auth(&token2)
        .send()
        .await
        .unwrap();
    assert_eq!(resp_success.status(), StatusCode::OK);
}
