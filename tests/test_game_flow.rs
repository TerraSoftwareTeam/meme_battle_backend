use axum::{
    extract::Path,
    http::StatusCode,
    routing::{delete, post},
    Json, Router,
};
use jsonwebtoken::{decode, Validation};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, Row};
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
    features::game::{
        api::dto::{GameStateDto, ReadyRequest},
        GameCard, GameMode, RoundPhase, GameRepository, GameRepositoryImpl,
    },
};

fn mock_cdn_router() -> Router {
    Router::new()
        .route("/api/v4/upload", post(handle_mock_upload))
        .route("/api/v4/upload/{id}", delete(handle_mock_delete))
}

async fn handle_mock_upload() -> Json<Value> {
    let file_id = Uuid::new_v4().to_string();
    Json(json!({
        "id": file_id,
        "filename": "uploaded_meme.png",
        "size": 1234,
        "content_type": "image/png",
        "url": format!("https://cdn.hackclub.com/{}.png", file_id)
    }))
}

async fn handle_mock_delete(Path(_id): Path<String>) -> Json<Value> {
    Json(json!({
        "deleted": true
    }))
}

#[tokio::test]
async fn test_full_game_flow_and_lock_lifecycle() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Spin up the mock CDN server
    let cdn_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cdn_addr = cdn_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(cdn_listener, mock_cdn_router()).await.unwrap();
    });

    // 2. Load configuration and point HackClub CDN to our mock CDN server
    let mut config = Config::from_env().unwrap();
    config.hackclub_cdn_base_url = format!("http://{}", cdn_addr);
    config.hackclub_cdn_api_key = Some("sk_cdn_test_key".to_string());

    // 3. Connect to the test DB and run migrations
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    // 4. Start the application router on an ephemeral port
    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    let state = build_app_state(pool.clone(), config);
    let app_state = state.clone();
    let app = create_router(state);
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // 5. Register 3 Guest Users
    let resp1 = client.post(format!("{}/auth/guest", base_url)).send().await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);
    let auth_body1: RestApiResponse<Value> = resp1.json().await.unwrap();
    let token1 = auth_body1.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let resp2 = client.post(format!("{}/auth/guest", base_url)).send().await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);
    let auth_body2: RestApiResponse<Value> = resp2.json().await.unwrap();
    let token2 = auth_body2.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let resp3 = client.post(format!("{}/auth/guest", base_url)).send().await.unwrap();
    assert_eq!(resp3.status(), StatusCode::OK);
    let auth_body3: RestApiResponse<Value> = resp3.json().await.unwrap();
    let token3 = auth_body3.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let claims1 = decode::<Claims>(&token1, &KEYS.decoding, &Validation::default()).unwrap().claims;
    let user_id1 = Uuid::parse_str(&claims1.sub).unwrap();
    let claims2 = decode::<Claims>(&token2, &KEYS.decoding, &Validation::default()).unwrap().claims;
    let user_id2 = Uuid::parse_str(&claims2.sub).unwrap();
    let claims3 = decode::<Claims>(&token3, &KEYS.decoding, &Validation::default()).unwrap().claims;
    let user_id3 = Uuid::parse_str(&claims3.sub).unwrap();

    // 6. Upload 6 files to our media API using mock CDN (required: P=3, H=1, R=1 -> memes: 3*1 + 3*1 = 6)
    let mut media_ids = Vec::new();
    for i in 1..=6 {
        let form = reqwest::multipart::Form::new()
            .part("file", reqwest::multipart::Part::bytes(vec![1, 2, 3])
                .file_name(format!("meme_{}.png", i))
                .mime_str("image/png")
                .unwrap());

        let upload_resp = client.post(format!("{}/media/upload/image", base_url))
            .bearer_auth(&token1)
            .multipart(form)
            .send()
            .await
            .unwrap();
        assert_eq!(upload_resp.status(), StatusCode::OK, "Failed to upload image {}", i);
        let upload_body: RestApiResponse<Value> = upload_resp.json().await.unwrap();
        let m_id = upload_body.0.data.unwrap().get("id").unwrap().as_i64().unwrap();
        media_ids.push(m_id);
    }

    // Verify uploaded media has status = 'pending' and visibility = 'private'
    let media_row = sqlx::query("SELECT status::text as status, visibility::text as visibility FROM media_assets WHERE id = $1")
        .bind(media_ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(media_row.get::<String, _>("status"), "pending");
    assert_eq!(media_row.get::<String, _>("visibility"), "private");

    // 7. Create Meme Pack
    let create_pack_payload = json!({
        "name": "Flow Test Meme Pack",
        "description": "Flow description",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": false,
        "media_ids": media_ids
    });
    let create_pack_resp = client.post(format!("{}/games/packs/memes", base_url))
        .bearer_auth(&token1)
        .json(&create_pack_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_pack_resp.status(), StatusCode::OK);
    let pack_body: RestApiResponse<Value> = create_pack_resp.json().await.unwrap();
    let pack_id = Uuid::parse_str(pack_body.0.data.unwrap().get("id").unwrap().as_str().unwrap()).unwrap();

    // Verify media asset status has updated to 'attached' and 'public'
    let media_row_attached = sqlx::query("SELECT status::text as status, visibility::text as visibility FROM media_assets WHERE id = $1")
        .bind(media_ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(media_row_attached.get::<String, _>("status"), "attached");
    assert_eq!(media_row_attached.get::<String, _>("visibility"), "public");

    // 8. Delete a pack meme and test non-active deletion
    let get_pack_resp = client.get(format!("{}/games/packs/memes/{}", base_url, pack_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(get_pack_resp.status(), StatusCode::OK);
    let get_pack_body: RestApiResponse<Value> = get_pack_resp.json().await.unwrap();
    let memes_array = get_pack_body.0.data.unwrap().get("memes").unwrap().as_array().unwrap().clone();
    let pack_meme_id = Uuid::parse_str(memes_array[0].get("id").unwrap().as_str().unwrap()).unwrap();

    let del_meme_resp = client.delete(format!("{}/games/packs/memes/{}/memes/{}", base_url, pack_id, pack_meme_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(del_meme_resp.status(), StatusCode::OK);

    // Upload 7th file and add it to pack to keep required amount (6 memes)
    let form7 = reqwest::multipart::Form::new()
        .part("file", reqwest::multipart::Part::bytes(vec![1, 2, 3])
            .file_name("meme_7.png")
            .mime_str("image/png")
            .unwrap());
    let upload_resp7 = client.post(format!("{}/media/upload/image", base_url))
        .bearer_auth(&token1)
        .multipart(form7)
        .send()
        .await
        .unwrap();
    let m_id7 = upload_resp7.json::<RestApiResponse<Value>>().await.unwrap().0.data.unwrap().get("id").unwrap().as_i64().unwrap();

    let add_meme_resp = client.post(format!("{}/games/packs/memes/{}/memes", base_url, pack_id))
        .bearer_auth(&token1)
        .json(&json!({ "media_ids": vec![m_id7] }))
        .send()
        .await
        .unwrap();
    assert_eq!(add_meme_resp.status(), StatusCode::OK);

    // Fetch the updated pack memes to find a valid pack meme ID to test lock blocking
    let get_pack_resp2 = client.get(format!("{}/games/packs/memes/{}", base_url, pack_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    let get_pack_body2: RestApiResponse<Value> = get_pack_resp2.json().await.unwrap();
    let memes_array2 = get_pack_body2.0.data.unwrap().get("memes").unwrap().as_array().unwrap().clone();
    let pack_meme_id2 = Uuid::parse_str(memes_array2[0].get("id").unwrap().as_str().unwrap()).unwrap();

    // 9. Create Situation Pack
    let create_sit_payload = json!({
        "name": "Flow Test Situation Pack",
        "description": "Flow description",
        "language_code": "ru",
        "safety_level": "family_friendly",
        "is_public": false,
        "prompts": vec!["Flow prompt 1", "Flow prompt 2"]
    });
    let create_sit_resp = client.post(format!("{}/games/packs/situations", base_url))
        .bearer_auth(&token1)
        .json(&create_sit_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_sit_resp.status(), StatusCode::OK);
    let sit_body: RestApiResponse<Value> = create_sit_resp.json().await.unwrap();
    let sit_pack_id = Uuid::parse_str(sit_body.0.data.unwrap().get("id").unwrap().as_str().unwrap()).unwrap();

    // --- TIMEOUT FLOW TEST (GAME 1) ---
    let create_game_payload_timeout = json!({
        "mode": "situation_to_meme",
        "selected_situation_pack_ids": vec![sit_pack_id],
        "selected_meme_pack_ids": vec![pack_id],
        "max_rounds": 1,
        "hand_size": 1
    });
    let create_game_resp_1 = client.post(format!("{}/games", base_url))
        .bearer_auth(&token1)
        .json(&create_game_payload_timeout)
        .send()
        .await
        .unwrap();
    assert_eq!(create_game_resp_1.status(), StatusCode::OK);
    let game_resp_1: RestApiResponse<Value> = create_game_resp_1.json().await.unwrap();
    let game_id_1 = Uuid::parse_str(game_resp_1.0.data.unwrap().get("id").unwrap().as_str().unwrap()).unwrap();

    // Join Players
    client.post(format!("{}/games/{}/join", base_url, game_id_1)).bearer_auth(&token2).send().await.unwrap();
    client.post(format!("{}/games/{}/join", base_url, game_id_1)).bearer_auth(&token3).send().await.unwrap();

    // Ready All
    client.post(format!("{}/games/{}/ready", base_url, game_id_1)).bearer_auth(&token1).json(&ReadyRequest { is_ready: true }).send().await.unwrap();
    client.post(format!("{}/games/{}/ready", base_url, game_id_1)).bearer_auth(&token2).json(&ReadyRequest { is_ready: true }).send().await.unwrap();
    client.post(format!("{}/games/{}/ready", base_url, game_id_1)).bearer_auth(&token3).json(&ReadyRequest { is_ready: true }).send().await.unwrap();

    // Start game
    let start_resp_1 = client.post(format!("{}/games/{}/start", base_url, game_id_1))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp_1.status(), StatusCode::OK);

    // Get current round
    let state_resp_1 = client.get(format!("{}/games/{}/state", base_url, game_id_1)).bearer_auth(&token1).send().await.unwrap();
    let state_dto_1: RestApiResponse<GameStateDto> = state_resp_1.json().await.unwrap();
    let state_1 = state_dto_1.0.data.unwrap();
    let round_id_1 = state_1.round.as_ref().unwrap().id;

    // Manually set phase_expires_at to the past in DB to trigger timeout
    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(10))
        .bind(round_id_1)
        .execute(&pool)
        .await
        .unwrap();

    // Trigger background worker process step
    app_state.game.process_timeout.execute(round_id_1).await.unwrap();

    // Fetch state again and verify it is in Voting phase
    let state_resp_voting = client.get(format!("{}/games/{}/state", base_url, game_id_1)).bearer_auth(&token1).send().await.unwrap();
    let state_dto_voting: RestApiResponse<GameStateDto> = state_resp_voting.json().await.unwrap();
    let state_voting = state_dto_voting.0.data.unwrap();
    assert_eq!(state_voting.round.as_ref().unwrap().phase, RoundPhase::Voting);

    // Manually set voting phase_expires_at to the past to trigger timeout
    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(10))
        .bind(round_id_1)
        .execute(&pool)
        .await
        .unwrap();

    // Trigger background worker process step again
    app_state.game.process_timeout.execute(round_id_1).await.unwrap();

    // Verify game status is finished
    let game_row_1 = sqlx::query("SELECT status::text as status FROM games WHERE id = $1").bind(game_id_1).fetch_one(&pool).await.unwrap();
    assert_eq!(game_row_1.get::<String, _>("status"), "finished");
    // --- END TIMEOUT FLOW TEST (GAME 1) ---

    // 10. Create Game
    let create_game_payload = json!({
        "mode": "situation_to_meme",
        "selected_situation_pack_ids": vec![sit_pack_id],
        "selected_meme_pack_ids": vec![pack_id],
        "max_rounds": 1,
        "hand_size": 1
    });
    let create_game_resp = client.post(format!("{}/games", base_url))
        .bearer_auth(&token1)
        .json(&create_game_payload)
        .send()
        .await
        .unwrap();
    assert_eq!(create_game_resp.status(), StatusCode::OK);
    let game_resp: RestApiResponse<Value> = create_game_resp.json().await.unwrap();
    let game_id = Uuid::parse_str(game_resp.0.data.unwrap().get("id").unwrap().as_str().unwrap()).unwrap();

    // Join Players
    let join_resp2 = client.post(format!("{}/games/{}/join", base_url, game_id)).bearer_auth(&token2).send().await.unwrap();
    assert_eq!(join_resp2.status(), StatusCode::OK);
    let join_resp3 = client.post(format!("{}/games/{}/join", base_url, game_id)).bearer_auth(&token3).send().await.unwrap();
    assert_eq!(join_resp3.status(), StatusCode::OK);

    // Ready All
    client.post(format!("{}/games/{}/ready", base_url, game_id)).bearer_auth(&token1).json(&ReadyRequest { is_ready: true }).send().await.unwrap();
    client.post(format!("{}/games/{}/ready", base_url, game_id)).bearer_auth(&token2).json(&ReadyRequest { is_ready: true }).send().await.unwrap();
    client.post(format!("{}/games/{}/ready", base_url, game_id)).bearer_auth(&token3).json(&ReadyRequest { is_ready: true }).send().await.unwrap();

    // Start game
    let start_resp = client.post(format!("{}/games/{}/start", base_url, game_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(start_resp.status(), StatusCode::OK);

    // Verify content locks are populated (7 locks: 6 memes + 1 situation)
    let locks_count = sqlx::query("SELECT COUNT(*) FROM game_content_locks WHERE game_id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(locks_count, 7);

    // 11. Verify ACTIVE locks validation (deletes should return 409 Conflict)
    let active_del_meme_resp = client.delete(format!("{}/games/packs/memes/{}/memes/{}", base_url, pack_id, pack_meme_id2))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(active_del_meme_resp.status(), StatusCode::CONFLICT);
    let err_body: RestApiResponse<Value> = active_del_meme_resp.json().await.unwrap();
    assert!(err_body.0.message.contains("in use by an active game session"));

    // 12. Play the game to completion
    let state_resp1 = client.get(format!("{}/games/{}/state", base_url, game_id)).bearer_auth(&token1).send().await.unwrap();
    let state_dto1: RestApiResponse<GameStateDto> = state_resp1.json().await.unwrap();
    let state1 = state_dto1.0.data.unwrap();
    let round_id = state1.round.as_ref().unwrap().id;
    let card1_id = match &state1.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    let state_resp2 = client.get(format!("{}/games/{}/state", base_url, game_id)).bearer_auth(&token2).send().await.unwrap();
    let state_dto2: RestApiResponse<GameStateDto> = state_resp2.json().await.unwrap();
    let state2 = state_dto2.0.data.unwrap();
    let card2_id = match &state2.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    let state_resp3 = client.get(format!("{}/games/{}/state", base_url, game_id)).bearer_auth(&token3).send().await.unwrap();
    let state_dto3: RestApiResponse<GameStateDto> = state_resp3.json().await.unwrap();
    let state3 = state_dto3.0.data.unwrap();
    let card3_id = match &state3.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    // Submissions
    client.post(format!("{}/games/{}/rounds/{}/submit", base_url, game_id, round_id)).bearer_auth(&token1).json(&json!({ "card_id": card1_id })).send().await.unwrap();
    client.post(format!("{}/games/{}/rounds/{}/submit", base_url, game_id, round_id)).bearer_auth(&token2).json(&json!({ "card_id": card2_id })).send().await.unwrap();
    client.post(format!("{}/games/{}/rounds/{}/submit", base_url, game_id, round_id)).bearer_auth(&token3).json(&json!({ "card_id": card3_id })).send().await.unwrap();

    // Resolve submissions
    let db_subs = sqlx::query("SELECT id, user_id FROM round_submissions WHERE round_id = $1").bind(round_id).fetch_all(&pool).await.unwrap();
    let sub1 = db_subs.iter().find(|s| s.get::<Uuid, _>("user_id") == user_id1).unwrap().get::<Uuid, _>("id");
    let sub2 = db_subs.iter().find(|s| s.get::<Uuid, _>("user_id") == user_id2).unwrap().get::<Uuid, _>("id");
    let sub3 = db_subs.iter().find(|s| s.get::<Uuid, _>("user_id") == user_id3).unwrap().get::<Uuid, _>("id");

    // Anti-cheat verification: Player 1 tries to vote for their own submission (sub1)
    let self_vote_resp = client.post(format!("{}/games/{}/rounds/{}/vote", base_url, game_id, round_id))
        .bearer_auth(&token1)
        .json(&json!({ "submission_id": sub1 }))
        .send()
        .await
        .unwrap();
    assert_eq!(self_vote_resp.status(), StatusCode::BAD_REQUEST);
    let self_vote_err: RestApiResponse<Value> = self_vote_resp.json().await.unwrap();
    assert!(self_vote_err.0.message.contains("Cannot vote for your own submission"));

    // Real Votes
    client.post(format!("{}/games/{}/rounds/{}/vote", base_url, game_id, round_id)).bearer_auth(&token1).json(&json!({ "submission_id": sub2 })).send().await.unwrap();
    client.post(format!("{}/games/{}/rounds/{}/vote", base_url, game_id, round_id)).bearer_auth(&token2).json(&json!({ "submission_id": sub3 })).send().await.unwrap();
    let vote_resp3 = client.post(format!("{}/games/{}/rounds/{}/vote", base_url, game_id, round_id)).bearer_auth(&token3).json(&json!({ "submission_id": sub1 })).send().await.unwrap();
    assert_eq!(vote_resp3.status(), StatusCode::OK);

    // Verify game status is finished
    let game_row = sqlx::query("SELECT status::text as status FROM games WHERE id = $1").bind(game_id).fetch_one(&pool).await.unwrap();
    assert_eq!(game_row.get::<String, _>("status"), "finished");

    // Verify all game content locks are deleted
    let locks_count_after = sqlx::query("SELECT COUNT(*) FROM game_content_locks WHERE game_id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(locks_count_after, 0);

    // 13. Test deletion after game completion (must succeed due to ON DELETE CASCADE on history)
    let final_del_pack_resp = client.delete(format!("{}/games/packs/memes/{}", base_url, pack_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(final_del_pack_resp.status(), StatusCode::OK);

    let final_del_sit_pack_resp = client.delete(format!("{}/games/packs/situations/{}", base_url, sit_pack_id))
        .bearer_auth(&token1)
        .send()
        .await
        .unwrap();
    assert_eq!(final_del_sit_pack_resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_timer_and_concurrency_edge_cases() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    let mut config = Config::from_env().unwrap();
    config.hackclub_cdn_base_url = "http://127.0.0.1:9999".to_string();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    let state = build_app_state(pool.clone(), config);
    
    let test_start = chrono::Utc::now();
    sqlx::query("DELETE FROM games WHERE created_at < $1")
        .bind(test_start)
        .execute(&pool)
        .await
        .unwrap();
    
    // 1. Setup mock users
    let user_id1 = Uuid::new_v4();
    let user_id2 = Uuid::new_v4();
    let user_id3 = Uuid::new_v4();
    let handle1 = format!("handle_{}", Uuid::new_v4().simple());
    let handle2 = format!("handle_{}", Uuid::new_v4().simple());
    let handle3 = format!("handle_{}", Uuid::new_v4().simple());
    
    sqlx::query("INSERT INTO users (id, username, handle, role) VALUES ($1, 'user1', $4, 'user'), ($2, 'user2', $5, 'user'), ($3, 'user3', $6, 'user')")
        .bind(user_id1)
        .bind(user_id2)
        .bind(user_id3)
        .bind(handle1)
        .bind(handle2)
        .bind(handle3)
        .execute(&pool)
        .await
        .unwrap();

    // 2. Create situation pack & meme pack
    let sit_pack_id = Uuid::new_v4();
    sqlx::query("INSERT INTO situation_packs (id, author_id, name, language_code) VALUES ($1, $2, 'Pack', 'ru')")
        .bind(sit_pack_id)
        .bind(user_id1)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO pack_situations (id, pack_id, prompt_text) VALUES ($1, $2, 'Prompt 1'), ($3, $2, 'Prompt 2')")
        .bind(Uuid::new_v4())
        .bind(sit_pack_id)
        .bind(Uuid::new_v4())
        .execute(&pool)
        .await
        .unwrap();

    let meme_pack_id = Uuid::new_v4();
    sqlx::query("INSERT INTO meme_packs (id, author_id, name, language_code) VALUES ($1, $2, 'Pack', 'ru')")
        .bind(meme_pack_id)
        .bind(user_id1)
        .execute(&pool)
        .await
        .unwrap();
        
    let media_id1 = chrono::Utc::now().timestamp_millis() * 1000 + (rand::random::<i32>().abs() % 1000) as i64;
    let media_id2 = media_id1 + 1;
    let media_id3 = media_id1 + 2;
    let media_id4 = media_id1 + 3;
    let media_id5 = media_id1 + 4;
    let media_id6 = media_id1 + 5;
    let file_id1 = format!("f_{}", Uuid::new_v4().simple());
    let file_id2 = format!("f_{}", Uuid::new_v4().simple());
    let file_id3 = format!("f_{}", Uuid::new_v4().simple());
    let file_id4 = format!("f_{}", Uuid::new_v4().simple());
    let file_id5 = format!("f_{}", Uuid::new_v4().simple());
    let file_id6 = format!("f_{}", Uuid::new_v4().simple());
    sqlx::query("INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility) VALUES ($1, $7, 'hackclub_cdn', $8, 'http://url1', 'f1', 'image/png', 10, 'attached', 'public'), ($2, $7, 'hackclub_cdn', $9, 'http://url2', 'f2', 'image/png', 10, 'attached', 'public'), ($3, $7, 'hackclub_cdn', $10, 'http://url3', 'f3', 'image/png', 10, 'attached', 'public'), ($4, $7, 'hackclub_cdn', $11, 'http://url4', 'f4', 'image/png', 10, 'attached', 'public'), ($5, $7, 'hackclub_cdn', $12, 'http://url5', 'f5', 'image/png', 10, 'attached', 'public'), ($6, $7, 'hackclub_cdn', $13, 'http://url6', 'f6', 'image/png', 10, 'attached', 'public')")
        .bind(media_id1)
        .bind(media_id2)
        .bind(media_id3)
        .bind(media_id4)
        .bind(media_id5)
        .bind(media_id6)
        .bind(user_id1)
        .bind(file_id1)
        .bind(file_id2)
        .bind(file_id3)
        .bind(file_id4)
        .bind(file_id5)
        .bind(file_id6)
        .execute(&pool)
        .await
        .unwrap();

    let pack_meme_id1 = Uuid::new_v4();
    let pack_meme_id2 = Uuid::new_v4();
    let pack_meme_id3 = Uuid::new_v4();
    let pack_meme_id4 = Uuid::new_v4();
    let pack_meme_id5 = Uuid::new_v4();
    let pack_meme_id6 = Uuid::new_v4();
    sqlx::query("INSERT INTO pack_memes (id, pack_id, media_id) VALUES ($1, $2, $3), ($4, $5, $6), ($7, $8, $9), ($10, $11, $12), ($13, $14, $15), ($16, $17, $18)")
        .bind(pack_meme_id1).bind(meme_pack_id).bind(media_id1)
        .bind(pack_meme_id2).bind(meme_pack_id).bind(media_id2)
        .bind(pack_meme_id3).bind(meme_pack_id).bind(media_id3)
        .bind(pack_meme_id4).bind(meme_pack_id).bind(media_id4)
        .bind(pack_meme_id5).bind(meme_pack_id).bind(media_id5)
        .bind(pack_meme_id6).bind(meme_pack_id).bind(media_id6)
        .execute(&pool)
        .await
        .unwrap();

    let repo = GameRepositoryImpl::new(pool.clone());

    // 3. Create Game
    let game = state.game.create_game.execute(user_id1, GameMode::SituationToMeme, vec![sit_pack_id], vec![meme_pack_id], 1, 1).await.unwrap();
    state.game.join_game.execute(user_id2, game.id).await.unwrap();
    state.game.join_game.execute(user_id3, game.id).await.unwrap();
    state.game.set_ready.execute(user_id1, game.id, true).await.unwrap();
    state.game.set_ready.execute(user_id2, game.id, true).await.unwrap();
    state.game.set_ready.execute(user_id3, game.id, true).await.unwrap();
    state.game.start_game.execute(user_id1, game.id).await.unwrap();

    let round = state.game.get_game_state.execute(user_id1, game.id).await.unwrap().round.unwrap();

    // Test Case 1: Stale Lease Takeover
    let worker_a = Uuid::new_v4();
    let worker_b = Uuid::new_v4();
    let now = chrono::Utc::now();

    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1, claimed_at = $2, claimed_by = $3 WHERE id = $4")
        .bind(now - chrono::Duration::seconds(10))
        .bind(now - chrono::Duration::seconds(40))
        .bind(worker_a)
        .bind(round.id)
        .execute(&pool)
        .await
        .unwrap();

    let round_b = state.game.timer_worker.process_single_expired_round(worker_b).await.unwrap();
    assert!(round_b, "Worker B should take over the stale lease");

    // Test Case 2: Concurrency Guard
    let state_voting = state.game.get_game_state.execute(user_id1, game.id).await.unwrap().round.unwrap();
    assert_eq!(state_voting.phase, RoundPhase::Voting);

    state.game.process_timeout.execute(round.id).await.unwrap();

    let state_after_late_call = state.game.get_game_state.execute(user_id1, game.id).await.unwrap().round.unwrap();
    assert_eq!(state_after_late_call.phase, RoundPhase::Voting);

    // Test Case 3: Empty Hand Auto-submission skip
    let game_empty = state.game.create_game.execute(user_id1, GameMode::SituationToMeme, vec![sit_pack_id], vec![meme_pack_id], 1, 1).await.unwrap();
    state.game.join_game.execute(user_id2, game_empty.id).await.unwrap();
    state.game.join_game.execute(user_id3, game_empty.id).await.unwrap();
    state.game.set_ready.execute(user_id1, game_empty.id, true).await.unwrap();
    state.game.set_ready.execute(user_id2, game_empty.id, true).await.unwrap();
    state.game.set_ready.execute(user_id3, game_empty.id, true).await.unwrap();
    state.game.start_game.execute(user_id1, game_empty.id).await.unwrap();

    let round_empty = state.game.get_game_state.execute(user_id1, game_empty.id).await.unwrap().round.unwrap();

    sqlx::query("DELETE FROM game_player_hand WHERE game_id = $1 AND user_id = $2")
        .bind(game_empty.id)
        .bind(user_id2)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(10))
        .bind(round_empty.id)
        .execute(&pool)
        .await
        .unwrap();

    state.game.process_timeout.execute(round_empty.id).await.unwrap();

    let state_voting_empty = state.game.get_game_state.execute(user_id1, game_empty.id).await.unwrap().round.unwrap();
    assert_eq!(state_voting_empty.phase, RoundPhase::Voting);

    // Test Case 4: Zero-Votes Timeout (Nullable Winner)
    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(10))
        .bind(round_empty.id)
        .execute(&pool)
        .await
        .unwrap();

    state.game.process_timeout.execute(round_empty.id).await.unwrap();

    let game_empty_status = sqlx::query("SELECT status::text as status FROM games WHERE id = $1")
        .bind(game_empty.id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<String, _>("status");
    assert_eq!(game_empty_status, "finished");

    let round_winner = sqlx::query("SELECT winner_user_id FROM game_rounds WHERE id = $1")
        .bind(round_empty.id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<Option<Uuid>, _>("winner_user_id");
    assert!(round_winner.is_none(), "Round winner should be None when there are no votes");

    // Test Case 5: Partial Submissions Auto-Submit
    let game_partial = state.game.create_game.execute(user_id1, GameMode::SituationToMeme, vec![sit_pack_id], vec![meme_pack_id], 1, 1).await.unwrap();
    state.game.join_game.execute(user_id2, game_partial.id).await.unwrap();
    state.game.join_game.execute(user_id3, game_partial.id).await.unwrap();
    state.game.set_ready.execute(user_id1, game_partial.id, true).await.unwrap();
    state.game.set_ready.execute(user_id2, game_partial.id, true).await.unwrap();
    state.game.set_ready.execute(user_id3, game_partial.id, true).await.unwrap();
    state.game.start_game.execute(user_id1, game_partial.id).await.unwrap();

    let round_partial = state.game.get_game_state.execute(user_id1, game_partial.id).await.unwrap().round.unwrap();

    let hand1 = state.game.get_game_state.execute(user_id1, game_partial.id).await.unwrap().my_hand;
    let card_id = match &hand1[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };
    state.game.submit_card.execute(user_id1, game_partial.id, round_partial.id, card_id).await.unwrap();

    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(10))
        .bind(round_partial.id)
        .execute(&pool)
        .await
        .unwrap();

    state.game.process_timeout.execute(round_partial.id).await.unwrap();

    let round_partial_after = state.game.get_game_state.execute(user_id1, game_partial.id).await.unwrap().round.unwrap();
    assert_eq!(round_partial_after.phase, RoundPhase::Voting);

    let submission_count = sqlx::query("SELECT COUNT(*) FROM round_submissions WHERE round_id = $1")
        .bind(round_partial.id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(submission_count, 3, "All 3 players should have submissions (2 auto-submitted)");

    // Test Case 6: Concurrent Lease Claim Protection
    let game_lease = state.game.create_game.execute(user_id1, GameMode::SituationToMeme, vec![sit_pack_id], vec![meme_pack_id], 1, 1).await.unwrap();
    state.game.join_game.execute(user_id2, game_lease.id).await.unwrap();
    state.game.join_game.execute(user_id3, game_lease.id).await.unwrap();
    state.game.set_ready.execute(user_id1, game_lease.id, true).await.unwrap();
    state.game.set_ready.execute(user_id2, game_lease.id, true).await.unwrap();
    state.game.set_ready.execute(user_id3, game_lease.id, true).await.unwrap();
    state.game.start_game.execute(user_id1, game_lease.id).await.unwrap();

    let round_lease = state.game.get_game_state.execute(user_id1, game_lease.id).await.unwrap().round.unwrap();

    sqlx::query("UPDATE game_rounds SET phase_expires_at = $1 WHERE id = $2")
        .bind(chrono::Utc::now() - chrono::Duration::seconds(10))
        .bind(round_lease.id)
        .execute(&pool)
        .await
        .unwrap();

    let now_lease = chrono::Utc::now();
    let stale_timeout_lease = now_lease - chrono::Duration::seconds(30);
    let worker_a_lease = Uuid::new_v4();
    let worker_b_lease = Uuid::new_v4();

    let claim_a = repo.claim_next_expired_round(worker_a_lease, now_lease, stale_timeout_lease).await.unwrap();
    assert!(claim_a.is_some(), "Worker A should successfully claim the round");
    assert_eq!(claim_a.unwrap().id, round_lease.id);

    let claim_b = repo.claim_next_expired_round(worker_b_lease, now_lease, stale_timeout_lease).await.unwrap();
    assert!(claim_b.is_none(), "Worker B should fail to claim the round because Worker A holds an active lease");

    let round_lease_db = sqlx::query("SELECT claimed_by FROM game_rounds WHERE id = $1")
        .bind(round_lease.id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(round_lease_db.get::<Option<Uuid>, _>("claimed_by"), Some(worker_a_lease));
}

