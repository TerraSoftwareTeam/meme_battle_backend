use axum::{
    body::{Body, Bytes},
    http::{Method, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use jsonwebtoken::{decode, Validation};
use serde::Serialize;
use serde_json::Value;
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::sync::Arc;
use tower::ServiceExt;
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
        api::dto::{
            CreateGameRequest, CreateMemePackRequest, CreateMemePackResponse,
            CreateSituationPackRequest, CreateSituationPackResponse, MemePackDetailsResponse,
            SituationPackDetailsResponse,
        },
        ContentSafetyLevel, GameMode, LanguageCode,
    },
};

async fn setup_db_and_router() -> (
    PgPool,
    Router,
    Arc<meme_battle_backend::common::app::state::AppState>,
) {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();

    run_database_migrations(&pool).await.unwrap();

    let state = Arc::new(build_app_state(pool.clone(), config));
    let app = create_router((*state).clone());

    (pool, app, state)
}

async fn send_request<T: Serialize>(
    app: &Router,
    method: Method,
    uri: &str,
    token: Option<&str>,
    payload: Option<&T>,
) -> (StatusCode, Bytes) {
    let body = if let Some(p) = payload {
        Body::from(serde_json::to_vec(p).unwrap())
    } else {
        Body::empty()
    };

    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");

    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {}", t));
    }

    let req = builder.body(body).unwrap();
    let res = app.clone().oneshot(req).await.unwrap();
    let status = res.status();
    let body_bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, body_bytes)
}

#[tokio::test]
async fn test_meme_and_situation_pack_soft_delete_filtering() {
    let (pool, app, _) = setup_db_and_router().await;

    // 1. Create Guest User
    let (status, bytes) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let auth_resp: RestApiResponse<Value> = serde_json::from_slice(&bytes).unwrap();
    let token = auth_resp
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let claims = decode::<Claims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(&KEYS.jwt_secret),
        &Validation::default(),
    )
    .unwrap()
    .claims;
    let user_id = Uuid::parse_str(&claims.sub).unwrap();

    // 2. Insert mock media assets
    let media_id1 = 8101i64;
    let media_id2 = 8102i64;
    for mid in [media_id1, media_id2] {
        sqlx::query(
            "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
             VALUES ($1, $2, 'hackclub_cdn', $3, $4, 'meme.png', 'image/png', 1024, 'attached', 'public')
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(mid)
        .bind(user_id)
        .bind(format!("file_{}", mid))
        .bind(format!("https://cdn.example.com/file_{}.png", mid))
        .execute(&pool)
        .await
        .unwrap();
    }

    // 3. Create Meme Pack
    let create_pack_req = CreateMemePackRequest {
        name: "Soft Delete Test Meme Pack".to_string(),
        description: Some("Test soft delete".to_string()),
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: true,
        media_ids: vec![media_id1, media_id2],
    };
    let (status, bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token),
        Some(&create_pack_req),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pack_resp: RestApiResponse<CreateMemePackResponse> =
        serde_json::from_slice(&bytes).unwrap();
    let meme_pack_id = pack_resp.0.data.unwrap().id;

    // Verify pack exists in listings and details
    let (status, bytes) =
        send_request::<()>(&app, Method::GET, "/games/packs/memes", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    let list_resp: RestApiResponse<Vec<Value>> = serde_json::from_slice(&bytes).unwrap();
    assert!(list_resp
        .0
        .data
        .unwrap()
        .iter()
        .any(|p| p.get("id").unwrap().as_str().unwrap() == meme_pack_id.to_string()));

    let (status, bytes) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pack_details: RestApiResponse<MemePackDetailsResponse> =
        serde_json::from_slice(&bytes).unwrap();
    let details = pack_details.0.data.unwrap();
    assert_eq!(details.memes.len(), 2);
    let meme_to_delete = details.memes[0].id;

    // 4. Delete 1 meme from the pack
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!(
            "/games/packs/memes/{}/memes/{}",
            meme_pack_id, meme_to_delete
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify pack details now only returns 1 meme
    let (status, bytes) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let pack_details_after: RestApiResponse<MemePackDetailsResponse> =
        serde_json::from_slice(&bytes).unwrap();
    let memes_after = pack_details_after.0.data.unwrap().memes;
    assert_eq!(memes_after.len(), 1);
    assert_ne!(memes_after[0].id, meme_to_delete);

    // 5. Delete entire meme pack
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify pack is gone from listing and returns 404 on details
    let (status, bytes) =
        send_request::<()>(&app, Method::GET, "/games/packs/memes", Some(&token), None).await;
    assert_eq!(status, StatusCode::OK);
    let list_resp_after: RestApiResponse<Vec<Value>> = serde_json::from_slice(&bytes).unwrap();
    assert!(!list_resp_after.0.data.unwrap().iter().any(|p| p
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        == meme_pack_id.to_string()));

    let (status, _) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 6. Test Situation Pack soft delete
    let create_sit_req = CreateSituationPackRequest {
        name: "Soft Delete Sit Pack".to_string(),
        description: Some("Test sit soft delete".to_string()),
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: true,
        prompts: vec!["Prompt 1".to_string(), "Prompt 2".to_string()],
    };
    let (status, bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token),
        Some(&create_sit_req),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sit_resp: RestApiResponse<CreateSituationPackResponse> =
        serde_json::from_slice(&bytes).unwrap();
    let sit_pack_id = sit_resp.0.data.unwrap().id;

    let (status, bytes) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sit_details: RestApiResponse<SituationPackDetailsResponse> =
        serde_json::from_slice(&bytes).unwrap();
    let situations = sit_details.0.data.unwrap().situations;
    assert_eq!(situations.len(), 2);
    let sit_to_delete = situations[0].id;

    // Delete 1 situation prompt
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!(
            "/games/packs/situations/{}/situations/{}",
            sit_pack_id, sit_to_delete
        ),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, bytes) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let sit_details_after: RestApiResponse<SituationPackDetailsResponse> =
        serde_json::from_slice(&bytes).unwrap();
    assert_eq!(sit_details_after.0.data.unwrap().situations.len(), 1);

    // Delete entire situation pack
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_unstarted_lobby_pack_auto_deselection_on_pack_delete() {
    let (pool, app, _) = setup_db_and_router().await;

    // 1. Create Guest User
    let (status, bytes) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let auth_resp: RestApiResponse<Value> = serde_json::from_slice(&bytes).unwrap();
    let token = auth_resp
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // 2. Insert mock media assets
    let mut media_ids = Vec::new();
    for mid in 8201i64..=8204i64 {
        sqlx::query(
            "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
             VALUES ($1, '00000000-0000-0000-0000-000000000000', 'hackclub_cdn', $2, 'https://cdn.example.com/m.png', 'm.png', 'image/png', 1024, 'attached', 'public')
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(mid)
        .bind(format!("f_{}", mid))
        .execute(&pool)
        .await
        .unwrap();
        media_ids.push(mid);
    }

    // 3. Create Meme Pack & Situation Pack
    let create_pack_req = CreateMemePackRequest {
        name: "Lobby Deselect Meme Pack".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: true,
        media_ids,
    };
    let (_, bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token),
        Some(&create_pack_req),
    )
    .await;
    let meme_pack_id = serde_json::from_slice::<RestApiResponse<CreateMemePackResponse>>(&bytes)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    let create_sit_req = CreateSituationPackRequest {
        name: "Lobby Deselect Sit Pack".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: true,
        prompts: vec!["Lobby prompt 1".to_string(), "Lobby prompt 2".to_string()],
    };
    let (_, bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token),
        Some(&create_sit_req),
    )
    .await;
    let sit_pack_id =
        serde_json::from_slice::<RestApiResponse<CreateSituationPackResponse>>(&bytes)
            .unwrap()
            .0
            .data
            .unwrap()
            .id;

    // 4. Create Game Lobby with both packs selected
    let create_game_req = CreateGameRequest {
        name: "Lobby Deselect Test Game".to_string(),
        handle: None,
        mode: GameMode::SituationToMeme,
        selected_situation_pack_ids: vec![sit_pack_id],
        selected_meme_pack_ids: vec![meme_pack_id],
        max_rounds: 1,
        hand_size: 1,
    };
    let (status, bytes) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token),
        Some(&create_game_req),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let game_id = serde_json::from_slice::<RestApiResponse<Value>>(&bytes)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let game_uuid = Uuid::parse_str(&game_id).unwrap();

    // Verify packs are in game_selected_* tables
    let meme_count: i64 = sqlx::query(
        "SELECT COUNT(*) FROM game_selected_meme_packs WHERE game_id = $1 AND pack_id = $2",
    )
    .bind(game_uuid)
    .bind(meme_pack_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(meme_count, 1);

    let sit_count: i64 = sqlx::query(
        "SELECT COUNT(*) FROM game_selected_situation_packs WHERE game_id = $1 AND pack_id = $2",
    )
    .bind(game_uuid)
    .bind(sit_pack_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(sit_count, 1);

    // 5. Delete Meme Pack -> should be removed from unstarted lobby
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let meme_count_after: i64 = sqlx::query(
        "SELECT COUNT(*) FROM game_selected_meme_packs WHERE game_id = $1 AND pack_id = $2",
    )
    .bind(game_uuid)
    .bind(meme_pack_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(
        meme_count_after, 0,
        "Deleted meme pack must be deselected from unstarted lobbies"
    );

    // 6. Delete Situation Pack -> should be removed from unstarted lobby
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let sit_count_after: i64 = sqlx::query(
        "SELECT COUNT(*) FROM game_selected_situation_packs WHERE game_id = $1 AND pack_id = $2",
    )
    .bind(game_uuid)
    .bind(sit_pack_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get(0);
    assert_eq!(
        sit_count_after, 0,
        "Deleted situation pack must be deselected from unstarted lobbies"
    );
}

#[tokio::test]
async fn test_cleanup_worker_active_lock_postponement_and_history_preservation() {
    let (pool, app, state) = setup_db_and_router().await;

    // 1. Create Guest User
    let (status, bytes) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let auth_resp: RestApiResponse<Value> = serde_json::from_slice(&bytes).unwrap();
    let token = auth_resp
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let claims = decode::<Claims>(
        &token,
        &jsonwebtoken::DecodingKey::from_secret(&KEYS.jwt_secret),
        &Validation::default(),
    )
    .unwrap()
    .claims;
    let user_id = Uuid::parse_str(&claims.sub).unwrap();

    // 2. Clear purge tasks and insert mock media asset
    let media_id = 8301i64;
    sqlx::query("DELETE FROM media_purge_tasks")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM media_assets WHERE id = $1")
        .bind(media_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
         VALUES ($1, $2, 'hackclub_cdn', 'f8301', 'https://cdn.example.com/8301.png', 'history_test.png', 'image/png', 1024, 'attached', 'public')
         ON CONFLICT (id) DO NOTHING"
    )
    .bind(media_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // 3. Create Meme Pack with this media asset
    let create_pack_req = CreateMemePackRequest {
        name: "Lock & History Test Pack".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: true,
        media_ids: vec![media_id],
    };
    let (_, bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token),
        Some(&create_pack_req),
    )
    .await;
    let meme_pack_id = serde_json::from_slice::<RestApiResponse<CreateMemePackResponse>>(&bytes)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Find pack_meme_id
    let pack_meme_id: Uuid =
        sqlx::query("SELECT id FROM pack_memes WHERE pack_id = $1 AND media_id = $2")
            .bind(meme_pack_id)
            .bind(media_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("id");

    // 4. Create Game and Game Round and Round Submission pointing to pack_meme_id
    let game_id = Uuid::new_v4();
    sqlx::query("INSERT INTO games (id, name, host_id, mode, status, max_rounds, hand_size) VALUES ($1, 'History Game', $2, 'situation_to_meme', 'playing', 1, 1)")
        .bind(game_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    // Create Situation Pack for prompt
    let create_sit_req = CreateSituationPackRequest {
        name: "Prompt Sit Pack".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: true,
        prompts: vec!["History prompt".to_string()],
    };
    let (_, bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token),
        Some(&create_sit_req),
    )
    .await;
    let sit_pack_id =
        serde_json::from_slice::<RestApiResponse<CreateSituationPackResponse>>(&bytes)
            .unwrap()
            .0
            .data
            .unwrap()
            .id;
    let situation_id: Uuid = sqlx::query("SELECT id FROM pack_situations WHERE pack_id = $1")
        .bind(sit_pack_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("id");

    let round_id = Uuid::new_v4();
    sqlx::query("INSERT INTO game_rounds (id, game_id, round_number, prompt_situation_id, phase, winner_user_id) VALUES ($1, $2, 1, $3, 'voting', $4)")
        .bind(round_id)
        .bind(game_id)
        .bind(situation_id)
        .bind(user_id)
        .execute(&pool)
        .await
        .unwrap();

    let submission_id = Uuid::new_v4();
    sqlx::query("INSERT INTO round_submissions (id, round_id, user_id, submission_meme_id) VALUES ($1, $2, $3, $4)")
        .bind(submission_id)
        .bind(round_id)
        .bind(user_id)
        .bind(pack_meme_id)
        .execute(&pool)
        .await
        .unwrap();

    // 5. Add Active Game Lock on media_id
    sqlx::query(
        "INSERT INTO game_content_locks (game_id, meme_id, situation_id) VALUES ($1, $2, NULL)",
    )
    .bind(game_id)
    .bind(pack_meme_id)
    .execute(&pool)
    .await
    .unwrap();

    // 6. Delete the meme from pack -> should soft delete and enqueue purge task
    let (status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/memes/{}/memes/{}", meme_pack_id, pack_meme_id),
        Some(&token),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // Verify task is in queue
    let task_count: i64 =
        sqlx::query("SELECT COUNT(*) FROM media_purge_tasks WHERE media_asset_id = $1")
            .bind(media_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert_eq!(task_count, 1);

    // 7. Run cleanup worker -> Media is locked by active game!
    state
        .media
        .cleanup_worker
        .process_pending_purges()
        .await
        .unwrap();

    // Verify task was NOT completed (it was deferred with error message)
    let deferred_task = sqlx::query(
        "SELECT retry_count, last_error FROM media_purge_tasks WHERE media_asset_id = $1",
    )
    .bind(media_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    let retry_count: i32 = deferred_task.get("retry_count");
    let last_error: Option<String> = deferred_task.get("last_error");
    assert_eq!(retry_count, 1);
    assert!(last_error
        .unwrap()
        .contains("Media locked in active game session"));

    // Media asset MUST still exist in database
    let asset_exists: bool = sqlx::query("SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = $1)")
        .bind(media_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get(0);
    assert!(
        asset_exists,
        "Media asset must NOT be deleted while locked in active game"
    );

    // 8. Simulate game ending: remove the lock from game_content_locks
    sqlx::query("DELETE FROM game_content_locks WHERE game_id = $1")
        .bind(game_id)
        .execute(&pool)
        .await
        .unwrap();

    // Reset task retry time so worker picks it up immediately
    sqlx::query("UPDATE media_purge_tasks SET next_retry_at = now() - INTERVAL '1 second' WHERE media_asset_id = $1")
        .bind(media_id)
        .execute(&pool)
        .await
        .unwrap();

    // 9. Run cleanup worker again -> Now unlocked!
    // Note: mock CDN or real CDN may return 404/NotFound or success, cleanup_worker treats NotFound as success and deletes DB row
    state
        .media
        .cleanup_worker
        .process_pending_purges()
        .await
        .unwrap();

    // Verify task is now completed and removed from queue
    let remaining_tasks: i64 =
        sqlx::query("SELECT COUNT(*) FROM media_purge_tasks WHERE media_asset_id = $1")
            .bind(media_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert_eq!(
        remaining_tasks, 0,
        "Purge task must be completed after locks release"
    );

    // Verify media_assets row is deleted
    let asset_exists_after: bool =
        sqlx::query("SELECT EXISTS(SELECT 1 FROM media_assets WHERE id = $1)")
            .bind(media_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert!(
        !asset_exists_after,
        "Media asset must be physically deleted from database"
    );

    // 10. CRITICAL CHECK: Foreign Key ON DELETE SET NULL and Round History Preservation
    // In pack_memes, media_id should now be NULL
    let pack_meme_media: Option<i64> = sqlx::query("SELECT media_id FROM pack_memes WHERE id = $1")
        .bind(pack_meme_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("media_id");
    assert_eq!(
        pack_meme_media, None,
        "pack_memes.media_id must be set to NULL (ON DELETE SET NULL)"
    );

    // IN round_submissions, the submission MUST STILL EXIST!
    let submission_exists: bool =
        sqlx::query("SELECT EXISTS(SELECT 1 FROM round_submissions WHERE id = $1)")
            .bind(submission_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get(0);
    assert!(
        submission_exists,
        "round_submissions MUST NOT be deleted by CASCADE! Game history must be preserved!"
    );

    // In game_rounds, the winner MUST STILL BE SET!
    let winner: Option<Uuid> = sqlx::query("SELECT winner_user_id FROM game_rounds WHERE id = $1")
        .bind(round_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("winner_user_id");
    assert_eq!(winner, Some(user_id), "Game round winner must be preserved");
}
