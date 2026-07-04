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
        GameCard,
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
