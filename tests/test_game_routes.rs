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
use tower::ServiceExt;
use uuid::Uuid;

use meme_battle_backend::{
    app::create_router,
    common::{
        app::{
            bootstrap::{build_app_state, run_database_migrations},
            config::Config,
        },
        http::{dto::RestApiResponse, role::Role},
        security::jwt::{make_jwt_token, Claims, KEYS},
    },
    features::game::{
        api::dto::{
            AddMemesToPackRequest, AddSituationsToPackRequest, CreateGameRequest,
            CreateMemePackRequest, CreateMemePackResponse, CreateSituationPackRequest,
            CreateSituationPackResponse, GameDto, GameStateDto, MemePackDetailsResponse,
            ReadyRequest, SituationPackDetailsResponse, SubmitCardRequest, UpdateMemePackRequest,
            UpdateSituationPackRequest, VoteRequest,
        },
        ContentSafetyLevel, GameCard, GameMode, RoundPhase,
    },
};

async fn setup_db_and_router() -> (PgPool, Router) {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap();

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();

    run_database_migrations(&pool).await.unwrap();

    let state = build_app_state(pool.clone(), config);
    let app = create_router(state);

    (pool, app)
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
async fn test_game_vulnerability_fixes_and_full_flow() {
    let (pool, app) = setup_db_and_router().await;

    // 1. Create Guest Users
    let (status1, bytes1) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status1, StatusCode::OK);
    let auth_resp1: RestApiResponse<Value> = serde_json::from_slice(&bytes1).unwrap();
    let token1 = auth_resp1
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (status2, bytes2) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status2, StatusCode::OK);
    let auth_resp2: RestApiResponse<Value> = serde_json::from_slice(&bytes2).unwrap();
    let token2 = auth_resp2
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Decode token IDs using KEY decoding
    let claims1 = decode::<Claims>(&token1, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id1 = Uuid::parse_str(&claims1.sub).unwrap();

    let claims2 = decode::<Claims>(&token2, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id2 = Uuid::parse_str(&claims2.sub).unwrap();

    // Insert 12 dummy media assets in DB for memes using sqlx
    for id in 2001..=2012 {
        sqlx::query(
            "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
             VALUES ($1, $2, 'hackclub_cdn', $3, $4, $5, 'image/png', 1024, 'pending', 'private')
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(id as i64)
        .bind(user_id1)
        .bind(format!("prov_id_{}", id))
        .bind(format!("https://example.com/{}.png", id))
        .bind(format!("meme_{}.png", id))
        .execute(&pool)
        .await
        .unwrap();
    }

    // 2. Create Meme Pack
    let meme_pack_payload = CreateMemePackRequest {
        name: "Test Meme Pack".to_string(),
        description: Some("Description".to_string()),
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false, // private pack
        media_ids: (2001..=2012).collect(),
    };

    let (status_create_meme, bytes_create_meme) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token1),
        Some(&meme_pack_payload),
    )
    .await;
    assert_eq!(status_create_meme, StatusCode::OK);
    let meme_pack_resp: RestApiResponse<CreateMemePackResponse> =
        serde_json::from_slice(&bytes_create_meme).unwrap();
    let meme_pack_id = meme_pack_resp.0.data.unwrap().id;

    // 3. Create Situation Pack
    let sit_pack_payload = CreateSituationPackRequest {
        name: "Test Situation Pack".to_string(),
        description: Some("Description".to_string()),
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false, // private pack
        prompts: vec![
            "When you write clean Rust code".to_string(),
            "When compilation fails on a missing semicolon".to_string(),
            "When tests pass on the first try".to_string(),
        ],
    };

    let (status_create_sit, bytes_create_sit) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token1),
        Some(&sit_pack_payload),
    )
    .await;
    assert_eq!(status_create_sit, StatusCode::OK);
    let sit_pack_resp: RestApiResponse<CreateSituationPackResponse> =
        serde_json::from_slice(&bytes_create_sit).unwrap();
    let sit_pack_id = sit_pack_resp.0.data.unwrap().id;

    // 4. Vulnerability Fix Verification: Private Pack Exposure
    // Token 2 (User 2) should NOT be able to view User 1's private meme pack details
    let (get_status_blocked, _) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(get_status_blocked, StatusCode::FORBIDDEN);

    // Token 2 (User 2) should NOT be able to view User 1's private situation pack details
    let (get_sit_status_blocked, _) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(get_sit_status_blocked, StatusCode::FORBIDDEN);

    // Token 1 (Author) should be able to view the private meme pack details
    let (get_status_ok, bytes_pack) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(get_status_ok, StatusCode::OK);
    let pack_details: RestApiResponse<MemePackDetailsResponse> =
        serde_json::from_slice(&bytes_pack).unwrap();
    assert_eq!(pack_details.0.data.unwrap().pack.name, "Test Meme Pack");

    // Token 1 (Author) should be able to view the private situation pack details
    let (get_sit_status_ok, bytes_sit_pack) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(get_sit_status_ok, StatusCode::OK);
    let sit_pack_details: RestApiResponse<SituationPackDetailsResponse> =
        serde_json::from_slice(&bytes_sit_pack).unwrap();
    assert_eq!(
        sit_pack_details.0.data.unwrap().pack.name,
        "Test Situation Pack"
    );

    // 5. Create Game
    let create_game_payload = CreateGameRequest {
        mode: GameMode::SituationToMeme,
        situation_pack_ids: vec![sit_pack_id],
        meme_pack_ids: vec![meme_pack_id],
    };

    let (status_create_game, bytes_create_game) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token1),
        Some(&create_game_payload),
    )
    .await;
    assert_eq!(status_create_game, StatusCode::OK);
    let game_resp: RestApiResponse<GameDto> = serde_json::from_slice(&bytes_create_game).unwrap();
    let game_id = game_resp.0.data.unwrap().id;

    // 6. Join Game
    let (join_status, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(join_status, StatusCode::OK);

    // Vulnerability Fix Verification: Duplicate Join Event Spam
    // Trying to join again should yield 409 Conflict
    let (rejoin_status, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(rejoin_status, StatusCode::CONFLICT);

    // 7. Ready Status
    // Vulnerability Fix Verification: Ready Status Leak for non-lobby players
    let token_random = make_jwt_token(&Uuid::new_v4().to_string(), &Role::User).unwrap();
    let (ready_blocked_status, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token_random),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(ready_blocked_status, StatusCode::NOT_FOUND);

    // Lobby player ready toggling (Player 2 ready)
    let (ready_status, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token2),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(ready_status, StatusCode::OK);

    // 8. Start Game
    let (start_status, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/start", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(start_status, StatusCode::OK);

    // 9. Play Round: Submit Cards
    let (state_status1, state_bytes1) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(state_status1, StatusCode::OK);
    let state_dto1: RestApiResponse<GameStateDto> = serde_json::from_slice(&state_bytes1).unwrap();
    let game_state_data1 = state_dto1.0.data.unwrap();
    let round_id = game_state_data1.round.as_ref().unwrap().id;

    // Dealt cards in host hand
    let card1_id = match &game_state_data1.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    // Player 1 submits card
    let (submit_status1, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/rounds/{}/submit", game_id, round_id),
        Some(&token1),
        Some(&SubmitCardRequest { card_id: card1_id }),
    )
    .await;
    assert_eq!(submit_status1, StatusCode::OK);

    // Fetch Player 2's hand using their token
    let (state_status2, state_bytes2) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(state_status2, StatusCode::OK);
    let state_dto2: RestApiResponse<GameStateDto> = serde_json::from_slice(&state_bytes2).unwrap();
    let game_state_data2 = state_dto2.0.data.unwrap();

    let card2_id = match &game_state_data2.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    // Player 2 submits card (should transition round to Voting)
    let (submit_status2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/rounds/{}/submit", game_id, round_id),
        Some(&token2),
        Some(&SubmitCardRequest { card_id: card2_id }),
    )
    .await;
    assert_eq!(submit_status2, StatusCode::OK);

    // 10. Play Round: Vote
    // Get round voting details (submissions list)
    let (vote_state_status, vote_state_bytes) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(vote_state_status, StatusCode::OK);
    let vote_state_dto: RestApiResponse<GameStateDto> =
        serde_json::from_slice(&vote_state_bytes).unwrap();
    let vote_state_data = vote_state_dto.0.data.unwrap();
    assert_eq!(
        vote_state_data.round.as_ref().unwrap().phase,
        RoundPhase::Voting
    );

    // Query submissions directly from the database
    let submissions = sqlx::query("SELECT id, user_id FROM round_submissions WHERE round_id = $1")
        .bind(round_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(submissions.len(), 2);

    // Find submission IDs
    let sub1 = submissions
        .iter()
        .find(|s| s.get::<Uuid, _>("user_id") == user_id1)
        .unwrap()
        .get::<Uuid, _>("id");
    let sub2 = submissions
        .iter()
        .find(|s| s.get::<Uuid, _>("user_id") == user_id2)
        .unwrap()
        .get::<Uuid, _>("id");

    // Player 1 votes for Player 2's submission
    let (vote_status1, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/rounds/{}/vote", game_id, round_id),
        Some(&token1),
        Some(&VoteRequest {
            submission_id: sub2,
        }),
    )
    .await;
    assert_eq!(vote_status1, StatusCode::OK);

    // Player 2 votes for Player 1's submission (round ends, winner set, new round created or lobby/ended)
    let (vote_status2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/rounds/{}/vote", game_id, round_id),
        Some(&token2),
        Some(&VoteRequest {
            submission_id: sub1,
        }),
    )
    .await;
    assert_eq!(vote_status2, StatusCode::OK);

    // Get final state
    let (final_status, final_bytes) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(final_status, StatusCode::OK);
    let final_state_dto: RestApiResponse<GameStateDto> =
        serde_json::from_slice(&final_bytes).unwrap();
    let final_state_data = final_state_dto.0.data.unwrap();

    // Validate score increase
    assert!(final_state_data.players.iter().any(|p| p.score > 0));

    // 11. Pack CRUD Details (Metadata modification & addition)
    // Update Pack details (Token 1)
    let update_meme_payload = UpdateMemePackRequest {
        name: "Updated Pack Name".to_string(),
        description: Some("New desc".to_string()),
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::Explicit,
        is_public: true,
    };
    let (update_status, _) = send_request(
        &app,
        Method::PATCH,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token1),
        Some(&update_meme_payload),
    )
    .await;
    assert_eq!(update_status, StatusCode::OK);

    // Verify it is now visible to Token 2 since it was changed to public
    let (get_public_status, bytes_updated) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(get_public_status, StatusCode::OK);
    let updated_details: RestApiResponse<MemePackDetailsResponse> =
        serde_json::from_slice(&bytes_updated).unwrap();
    assert_eq!(
        updated_details.0.data.unwrap().pack.name,
        "Updated Pack Name"
    );

    // Add memes to pack
    sqlx::query(
        "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
         VALUES (2099, $1, 'hackclub_cdn', 'prov_99', 'https://example.com/99.png', 'new_meme.png', 'image/png', 1024, 'pending', 'private')
         ON CONFLICT (id) DO NOTHING"
    )
    .bind(user_id1)
    .execute(&pool)
    .await
    .unwrap();

    let add_memes_payload = AddMemesToPackRequest {
        media_ids: vec![2099],
    };
    let (add_meme_status, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/memes/{}/memes", meme_pack_id),
        Some(&token1),
        Some(&add_memes_payload),
    )
    .await;
    assert_eq!(add_meme_status, StatusCode::OK);

    // Get pack detail and verify it has the new meme
    let (_, bytes_after_add) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token1),
        None,
    )
    .await;
    let details_after_add: RestApiResponse<MemePackDetailsResponse> =
        serde_json::from_slice(&bytes_after_add).unwrap();
    let memes_list = details_after_add.0.data.unwrap().memes;
    assert!(memes_list.iter().any(|m| m.media_id == Some(2099)));

    // Delete meme from pack
    let pack_meme_id = memes_list
        .iter()
        .find(|m| m.media_id == Some(2099))
        .unwrap()
        .id;
    let (del_meme_status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/memes/{}/memes/{}", meme_pack_id, pack_meme_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(del_meme_status, StatusCode::OK);

    // Delete game from DB to clean up foreign keys referencing pack_memes
    sqlx::query("DELETE FROM games WHERE id = $1")
        .bind(game_id)
        .execute(&pool)
        .await
        .unwrap();

    // Delete Pack
    let (del_pack_status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/memes/{}", meme_pack_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(del_pack_status, StatusCode::OK);

    // 12. Situation Pack CRUD details
    // Update situation pack
    let update_sit_payload = UpdateSituationPackRequest {
        name: "Updated Situation Pack".to_string(),
        description: Some("New sit desc".to_string()),
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::Explicit,
        is_public: true,
    };
    let (update_sit_status, _) = send_request(
        &app,
        Method::PATCH,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token1),
        Some(&update_sit_payload),
    )
    .await;
    assert_eq!(update_sit_status, StatusCode::OK);

    // Verify it is now visible to Token 2 since it was changed to public
    let (get_sit_public_status, bytes_sit_updated) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(get_sit_public_status, StatusCode::OK);
    let updated_sit_details: RestApiResponse<SituationPackDetailsResponse> =
        serde_json::from_slice(&bytes_sit_updated).unwrap();
    assert_eq!(
        updated_sit_details.0.data.unwrap().pack.name,
        "Updated Situation Pack"
    );

    // Add situations to pack
    let add_sits_payload = AddSituationsToPackRequest {
        prompts: vec!["When you fix a bug on production without testing".to_string()],
    };
    let (add_sit_status, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/situations/{}/situations", sit_pack_id),
        Some(&token1),
        Some(&add_sits_payload),
    )
    .await;
    assert_eq!(add_sit_status, StatusCode::OK);

    // Get pack detail and verify it has the new situation
    let (_, bytes_sit_after_add) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token1),
        None,
    )
    .await;
    let details_sit_after_add: RestApiResponse<SituationPackDetailsResponse> =
        serde_json::from_slice(&bytes_sit_after_add).unwrap();
    let situations_list = details_sit_after_add.0.data.unwrap().situations;
    assert!(situations_list
        .iter()
        .any(|s| s.prompt_text == "When you fix a bug on production without testing"));

    // Delete situation from pack
    let pack_sit_id = situations_list
        .iter()
        .find(|s| s.prompt_text == "When you fix a bug on production without testing")
        .unwrap()
        .id;
    let (del_sit_status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!(
            "/games/packs/situations/{}/situations/{}",
            sit_pack_id, pack_sit_id
        ),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(del_sit_status, StatusCode::OK);

    // Delete Situation Pack
    let (del_sit_pack_status, _) = send_request::<()>(
        &app,
        Method::DELETE,
        &format!("/games/packs/situations/{}", sit_pack_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(del_sit_pack_status, StatusCode::OK);
}

/// Verify that using a non-existent media ID produces a clean 404,
/// not an ugly "foreign key constraint violation" database error.
#[tokio::test]
async fn test_nonexistent_media_returns_404() {
    let (pool, app) = setup_db_and_router().await;

    // Register a guest user
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

    // Use IDs that are astronomically unlikely to exist in the DB
    let nonexistent_media_id: i64 = 999_999_999;

    // ── 1. CREATE pack with non-existent media ─────────────────────────
    let create_payload = CreateMemePackRequest {
        name: "Ghost Pack".to_string(),
        description: None,
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: vec![nonexistent_media_id],
    };

    let (create_status, create_bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token),
        Some(&create_payload),
    )
    .await;

    assert_eq!(
        create_status,
        StatusCode::NOT_FOUND,
        "Expected 404 when creating pack with non-existent media, got {} — body: {}",
        create_status,
        String::from_utf8_lossy(&create_bytes),
    );

    // The error body should mention what was missing, not dump a DB error
    let err_body = String::from_utf8_lossy(&create_bytes);
    assert!(
        err_body.contains("Media assets not found"),
        "Error message should say 'Media assets not found', got: {}",
        err_body,
    );
    assert!(
        !err_body.contains("foreign key constraint"),
        "Response must not expose raw DB errors, got: {}",
        err_body,
    );

    // ── 2. ADD non-existent media to an existing pack ──────────────────
    // First, create a valid empty pack (no media IDs)
    let valid_create = CreateMemePackRequest {
        name: "Real Pack".to_string(),
        description: None,
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: vec![],
    };

    let (valid_status, valid_bytes) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token),
        Some(&valid_create),
    )
    .await;
    assert_eq!(valid_status, StatusCode::OK);
    let pack_resp: RestApiResponse<CreateMemePackResponse> =
        serde_json::from_slice(&valid_bytes).unwrap();
    let pack_id = pack_resp.0.data.unwrap().id;

    // Now try adding a non-existent media to the real pack
    let add_payload = AddMemesToPackRequest {
        media_ids: vec![nonexistent_media_id],
    };

    let (add_status, add_bytes) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/memes/{}/memes", pack_id),
        Some(&token),
        Some(&add_payload),
    )
    .await;

    assert_eq!(
        add_status,
        StatusCode::NOT_FOUND,
        "Expected 404 when adding non-existent media to pack, got {} — body: {}",
        add_status,
        String::from_utf8_lossy(&add_bytes),
    );

    let add_err_body = String::from_utf8_lossy(&add_bytes);
    assert!(
        add_err_body.contains("Media assets not found"),
        "Error should say 'Media assets not found', got: {}",
        add_err_body,
    );
    assert!(
        !add_err_body.contains("foreign key constraint"),
        "Response must not expose raw DB errors, got: {}",
        add_err_body,
    );

    // Clean up the pack we created
    sqlx::query("DELETE FROM meme_packs WHERE id = $1")
        .bind(pack_id)
        .execute(&pool)
        .await
        .unwrap();
}

/// Verify that adding a duplicate meme (same media_id) or duplicate situation prompt
/// to a pack returns 409 Conflict with a human-readable message.
#[tokio::test]
async fn test_duplicate_pack_item_returns_conflict() {
    let (pool, app) = setup_db_and_router().await;

    // Register a guest user and seed one media asset
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

    let claims = jsonwebtoken::decode::<Claims>(
        &token,
        &KEYS.decoding,
        &jsonwebtoken::Validation::default(),
    )
    .unwrap()
    .claims;
    let user_id = Uuid::parse_str(&claims.sub).unwrap();

    // Seed a real media asset
    let media_id: i64 = 7777_001;
    sqlx::query(
        "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
         VALUES ($1, $2, 'hackclub_cdn', 'prov_dup_test', 'https://example.com/dup.png', 'dup.png', 'image/png', 1024, 'pending', 'private')
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(media_id)
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();

    // ── 1. MEME PACK duplicate ─────────────────────────────────────────
    // Create pack with the media already included
    let create_payload = CreateMemePackRequest {
        name: "Dup Test Pack".to_string(),
        description: None,
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: vec![media_id],
    };
    let (cs, cb) = send_request(&app, Method::POST, "/games/packs/memes", Some(&token), Some(&create_payload)).await;
    assert_eq!(cs, StatusCode::OK);
    let pack_resp: RestApiResponse<CreateMemePackResponse> = serde_json::from_slice(&cb).unwrap();
    let pack_id = pack_resp.0.data.unwrap().id;

    // Try adding the SAME media again — must be 409
    let add_dup = AddMemesToPackRequest { media_ids: vec![media_id] };
    let (dup_status, dup_bytes) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/memes/{}/memes", pack_id),
        Some(&token),
        Some(&add_dup),
    )
    .await;

    assert_eq!(
        dup_status, StatusCode::CONFLICT,
        "Expected 409 Conflict for duplicate meme, got {} — body: {}",
        dup_status, String::from_utf8_lossy(&dup_bytes),
    );
    let dup_body = String::from_utf8_lossy(&dup_bytes);
    assert!(
        dup_body.contains("already in this pack"),
        "Body should say 'already in this pack', got: {}", dup_body,
    );
    assert!(
        !dup_body.contains("foreign key constraint") && !dup_body.contains("unique constraint"),
        "Must not expose raw DB errors, got: {}", dup_body,
    );

    // ── 2. SITUATION PACK duplicate ────────────────────────────────────
    let prompt = "When the compiler is your best friend".to_string();
    let sit_create = CreateSituationPackRequest {
        name: "Dup Sit Pack".to_string(),
        description: None,
        language_code: "ru".to_string(),
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        prompts: vec![prompt.clone()],
    };
    let (ss, sb) = send_request(&app, Method::POST, "/games/packs/situations", Some(&token), Some(&sit_create)).await;
    assert_eq!(ss, StatusCode::OK);
    let sit_resp: RestApiResponse<CreateSituationPackResponse> = serde_json::from_slice(&sb).unwrap();
    let sit_pack_id = sit_resp.0.data.unwrap().id;

    // Try adding the SAME prompt again — must be 409
    let add_dup_sit = AddSituationsToPackRequest { prompts: vec![prompt.clone()] };
    let (dup_sit_status, dup_sit_bytes) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/situations/{}/situations", sit_pack_id),
        Some(&token),
        Some(&add_dup_sit),
    )
    .await;

    assert_eq!(
        dup_sit_status, StatusCode::CONFLICT,
        "Expected 409 Conflict for duplicate situation, got {} — body: {}",
        dup_sit_status, String::from_utf8_lossy(&dup_sit_bytes),
    );
    let dup_sit_body = String::from_utf8_lossy(&dup_sit_bytes);
    assert!(
        dup_sit_body.contains("already exists in this pack"),
        "Body should say 'already exists in this pack', got: {}", dup_sit_body,
    );

    // Cleanup
    sqlx::query("DELETE FROM meme_packs WHERE id = $1").bind(pack_id).execute(&pool).await.unwrap();
    sqlx::query("DELETE FROM situation_packs WHERE id = $1").bind(sit_pack_id).execute(&pool).await.unwrap();
}
