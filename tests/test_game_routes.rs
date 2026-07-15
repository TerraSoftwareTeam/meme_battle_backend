use axum::{
    body::{Body, Bytes},
    http::{Method, Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use jsonwebtoken::{decode, Validation};
use serde::Serialize;
use serde_json::{json, Value};
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
        http::dto::RestApiResponse,
        security::jwt::{Claims, KEYS},
    },
    features::game::{
        api::dto::{
            ActiveGamesResponseDto, AddMemesToPackRequest, AddSituationsToPackRequest,
            CreateGameRequest, CreateMemePackRequest, CreateMemePackResponse,
            CreateSituationPackRequest, CreateSituationPackResponse, GameDto, GameStateDto,
            LobbiesWsTokenDto, MemePackDetailsResponse, ReadyRequest, SituationPackDetailsResponse,
            SubmitCardRequest, UpdateGameRequest, UpdateMemePackRequest,
            UpdateSituationPackRequest, VoteRequest,
        },
        ContentSafetyLevel, GameCard, GameMode, LanguageCode, RoundPhase,
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

    let (status3, bytes3) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status3, StatusCode::OK);
    let auth_resp3: RestApiResponse<Value> = serde_json::from_slice(&bytes3).unwrap();
    let token3 = auth_resp3
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

    let claims3 = decode::<Claims>(&token3, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id3 = Uuid::parse_str(&claims3.sub).unwrap();

    // Insert 24 dummy media assets in DB for memes using sqlx
    for id in 2001..=2024 {
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
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false, // private pack
        media_ids: (2001..=2024).collect(),
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
        language_code: LanguageCode::Ru,
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

    // Test list_user_meme_packs endpoint
    let (status_my_memes, bytes_my_memes) = send_request::<()>(
        &app,
        Method::GET,
        "/games/packs/memes/me",
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(status_my_memes, StatusCode::OK);
    let my_memes: RestApiResponse<Vec<MemePackDetailsResponse>> =
        serde_json::from_slice(&bytes_my_memes).unwrap();
    let my_memes_data = my_memes.0.data.unwrap();
    assert!(my_memes_data.iter().any(|p| p.pack.id == meme_pack_id));

    // Test list_user_situation_packs endpoint
    let (status_my_sits, bytes_my_sits) = send_request::<()>(
        &app,
        Method::GET,
        "/games/packs/situations/me",
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(status_my_sits, StatusCode::OK);
    let my_sits: RestApiResponse<Vec<SituationPackDetailsResponse>> =
        serde_json::from_slice(&bytes_my_sits).unwrap();
    let my_sits_data = my_sits.0.data.unwrap();
    assert!(my_sits_data.iter().any(|p| p.pack.id == sit_pack_id));

    // For token2, the returned list shouldn't contain these packs as token2 didn't create them
    let (status_my_memes2, bytes_my_memes2) = send_request::<()>(
        &app,
        Method::GET,
        "/games/packs/memes/me",
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(status_my_memes2, StatusCode::OK);
    let my_memes2: RestApiResponse<Vec<MemePackDetailsResponse>> =
        serde_json::from_slice(&bytes_my_memes2).unwrap();
    assert!(!my_memes2
        .0
        .data
        .unwrap()
        .iter()
        .any(|p| p.pack.id == meme_pack_id));

    // 5. Create Game
    let create_game_payload = CreateGameRequest {
        mode: GameMode::SituationToMeme,
        selected_situation_pack_ids: vec![sit_pack_id],
        selected_meme_pack_ids: vec![meme_pack_id],
        max_rounds: 3,
        hand_size: 5,
        handle: None,
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

    let (join_status3, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token3),
        None,
    )
    .await;
    assert_eq!(join_status3, StatusCode::OK);

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
    let (status_rand, bytes_rand) =
        send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status_rand, StatusCode::OK);
    let auth_resp_rand: RestApiResponse<Value> = serde_json::from_slice(&bytes_rand).unwrap();
    let token_random = auth_resp_rand
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (ready_blocked_status, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token_random),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(ready_blocked_status, StatusCode::NOT_FOUND);

    // Lobby players ready toggling (Player 2, Player 3 ready)
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token1),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    let (ready_status, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token2),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(ready_status, StatusCode::OK);
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token3),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;

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
        &format!("/games/{}/submit", game_id),
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

    // Player 2 submits card
    let (submit_status2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/submit", game_id),
        Some(&token2),
        Some(&SubmitCardRequest { card_id: card2_id }),
    )
    .await;
    assert_eq!(submit_status2, StatusCode::OK);

    // Fetch Player 3's hand using their token
    let (state_status3, state_bytes3) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token3),
        None,
    )
    .await;
    assert_eq!(state_status3, StatusCode::OK);
    let state_dto3: RestApiResponse<GameStateDto> = serde_json::from_slice(&state_bytes3).unwrap();
    let game_state_data3 = state_dto3.0.data.unwrap();

    let card3_id = match &game_state_data3.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    // Player 3 submits card (transitions round to Voting)
    let (submit_status3, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/submit", game_id),
        Some(&token3),
        Some(&SubmitCardRequest { card_id: card3_id }),
    )
    .await;
    assert_eq!(submit_status3, StatusCode::OK);

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
    assert_eq!(submissions.len(), 3);

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
    let sub3 = submissions
        .iter()
        .find(|s| s.get::<Uuid, _>("user_id") == user_id3)
        .unwrap()
        .get::<Uuid, _>("id");

    // Player 1 votes for Player 2's submission
    let (vote_status1, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token1),
        Some(&VoteRequest {
            submission_id: sub2,
        }),
    )
    .await;
    assert_eq!(vote_status1, StatusCode::OK);

    // Player 2 votes for Player 3's submission
    let (vote_status2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token2),
        Some(&VoteRequest {
            submission_id: sub3,
        }),
    )
    .await;
    assert_eq!(vote_status2, StatusCode::OK);

    // Player 3 votes for Player 1's submission (round ends)
    let (vote_status3, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token3),
        Some(&VoteRequest {
            submission_id: sub1,
        }),
    )
    .await;
    assert_eq!(vote_status3, StatusCode::OK);

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
        language_code: LanguageCode::Ru,
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
        language_code: LanguageCode::Ru,
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
        language_code: LanguageCode::Ru,
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
        language_code: LanguageCode::Ru,
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
    let media_id: i64 = 7_777_001;
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
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: vec![media_id],
    };
    let (cs, cb) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token),
        Some(&create_payload),
    )
    .await;
    assert_eq!(cs, StatusCode::OK);
    let pack_resp: RestApiResponse<CreateMemePackResponse> = serde_json::from_slice(&cb).unwrap();
    let pack_id = pack_resp.0.data.unwrap().id;

    // Try adding the SAME media again — must be 409
    let add_dup = AddMemesToPackRequest {
        media_ids: vec![media_id],
    };
    let (dup_status, dup_bytes) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/memes/{}/memes", pack_id),
        Some(&token),
        Some(&add_dup),
    )
    .await;

    assert_eq!(
        dup_status,
        StatusCode::CONFLICT,
        "Expected 409 Conflict for duplicate meme, got {} — body: {}",
        dup_status,
        String::from_utf8_lossy(&dup_bytes),
    );
    let dup_body = String::from_utf8_lossy(&dup_bytes);
    assert!(
        dup_body.contains("already in this pack"),
        "Body should say 'already in this pack', got: {}",
        dup_body,
    );
    assert!(
        !dup_body.contains("foreign key constraint") && !dup_body.contains("unique constraint"),
        "Must not expose raw DB errors, got: {}",
        dup_body,
    );

    // ── 2. SITUATION PACK duplicate ────────────────────────────────────
    let prompt = "When the compiler is your best friend".to_string();
    let sit_create = CreateSituationPackRequest {
        name: "Dup Sit Pack".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        prompts: vec![prompt.clone()],
    };
    let (ss, sb) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token),
        Some(&sit_create),
    )
    .await;
    assert_eq!(ss, StatusCode::OK);
    let sit_resp: RestApiResponse<CreateSituationPackResponse> =
        serde_json::from_slice(&sb).unwrap();
    let sit_pack_id = sit_resp.0.data.unwrap().id;

    // Try adding the SAME prompt again — must be 409
    let add_dup_sit = AddSituationsToPackRequest {
        prompts: vec![prompt.clone()],
    };
    let (dup_sit_status, dup_sit_bytes) = send_request(
        &app,
        Method::POST,
        &format!("/games/packs/situations/{}/situations", sit_pack_id),
        Some(&token),
        Some(&add_dup_sit),
    )
    .await;

    assert_eq!(
        dup_sit_status,
        StatusCode::CONFLICT,
        "Expected 409 Conflict for duplicate situation, got {} — body: {}",
        dup_sit_status,
        String::from_utf8_lossy(&dup_sit_bytes),
    );
    let dup_sit_body = String::from_utf8_lossy(&dup_sit_bytes);
    assert!(
        dup_sit_body.contains("already exists in this pack"),
        "Body should say 'already exists in this pack', got: {}",
        dup_sit_body,
    );

    // Cleanup
    sqlx::query("DELETE FROM meme_packs WHERE id = $1")
        .bind(pack_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM situation_packs WHERE id = $1")
        .bind(sit_pack_id)
        .execute(&pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn test_game_start_deterministic_and_precomputes() {
    let (pool, app) = setup_db_and_router().await;

    // Create 3 guest users
    let (_s1, b1) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token1 = serde_json::from_slice::<RestApiResponse<Value>>(&b1)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (_s2, b2) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token2 = serde_json::from_slice::<RestApiResponse<Value>>(&b2)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (_s3, b3) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token3 = serde_json::from_slice::<RestApiResponse<Value>>(&b3)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let claims1 = decode::<Claims>(&token1, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id1 = Uuid::parse_str(&claims1.sub).unwrap();

    // Insert 20 dummy media assets for memes (insufficient for 3 players, hand size 5, rounds 3, which needs 24 memes)
    for id in 3001..=3020 {
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

    // Create Meme Pack with 20 memes
    let meme_create = CreateMemePackRequest {
        name: "Deterministic Test Memes".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: (3001..=3020).collect(),
    };
    let (sm, bm) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token1),
        Some(&meme_create),
    )
    .await;
    assert_eq!(sm, StatusCode::OK);
    let meme_pack_resp: RestApiResponse<CreateMemePackResponse> =
        serde_json::from_slice(&bm).unwrap();
    let meme_pack_id = meme_pack_resp.0.data.unwrap().id;

    // Create Situation Pack with 5 situations
    let sit_create = CreateSituationPackRequest {
        name: "Deterministic Test Situations".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        prompts: vec![
            "Sit 1".to_string(),
            "Sit 2".to_string(),
            "Sit 3".to_string(),
            "Sit 4".to_string(),
            "Sit 5".to_string(),
        ],
    };
    let (ss, sb) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token1),
        Some(&sit_create),
    )
    .await;
    assert_eq!(ss, StatusCode::OK);
    let sit_resp: RestApiResponse<CreateSituationPackResponse> =
        serde_json::from_slice(&sb).unwrap();
    let sit_pack_id = sit_resp.0.data.unwrap().id;

    // Create Game
    let create_game_payload = CreateGameRequest {
        mode: GameMode::SituationToMeme,
        selected_situation_pack_ids: vec![sit_pack_id],
        selected_meme_pack_ids: vec![meme_pack_id],
        max_rounds: 3,
        hand_size: 5,
        handle: None,
    };
    let (sg, bg) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token1),
        Some(&create_game_payload),
    )
    .await;
    assert_eq!(sg, StatusCode::OK);
    let game_resp: RestApiResponse<GameDto> = serde_json::from_slice(&bg).unwrap();
    let game_id = game_resp.0.data.unwrap().id;

    // Players join
    let (sj, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(sj, StatusCode::OK);
    let (sj3, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token3),
        None,
    )
    .await;
    assert_eq!(sj3, StatusCode::OK);

    // All players ready
    let (sr1, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token1),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(sr1, StatusCode::OK);
    let (sr2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token2),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(sr2, StatusCode::OK);
    let (sr3, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token3),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(sr3, StatusCode::OK);

    // Try starting the game -> should fail with 400 Validation Error "not_enough_memes"
    let (sstart, bstart) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/start", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(sstart, StatusCode::BAD_REQUEST);
    let err_msg = String::from_utf8_lossy(&bstart);
    assert!(
        err_msg.contains("not_enough_memes"),
        "Expected error 'not_enough_memes', got: {}",
        err_msg
    );

    // Now insert 4 more memes to make it 24 memes
    for id in 3021..=3024 {
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

        let add_payload = AddMemesToPackRequest {
            media_ids: vec![id as i64],
        };
        send_request(
            &app,
            Method::POST,
            &format!("/games/packs/memes/{}/memes", meme_pack_id),
            Some(&token1),
            Some(&add_payload),
        )
        .await;
    }

    // Now start the game -> should succeed with 200 OK
    let (sstart2, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/start", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(sstart2, StatusCode::OK);

    // Check game status, started_at
    let game_row = sqlx::query(
        "SELECT status::TEXT AS status, started_at, hand_size, max_rounds FROM games WHERE id = $1",
    )
    .bind(game_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(game_row.get::<String, _>("status"), "playing");
    assert!(game_row
        .try_get::<chrono::DateTime<chrono::Utc>, _>("started_at")
        .is_ok());

    // Check that exactly max_rounds (3) game rounds were created
    let rounds = sqlx::query("SELECT round_number, prompt_situation_id, phase::TEXT AS phase FROM game_rounds WHERE game_id = $1 ORDER BY round_number")
        .bind(game_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(rounds.len(), 3);
    assert_eq!(rounds[0].get::<String, _>("phase"), "submitting");
    assert_eq!(rounds[1].get::<String, _>("phase"), "waiting");
    assert_eq!(rounds[2].get::<String, _>("phase"), "waiting");

    // Check that game player hand has hand_size (5) cards for each player
    let hands_count = sqlx::query("SELECT COUNT(*) FROM game_player_hand WHERE game_id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(hands_count, 15); // 3 players * 5 cards

    // Check that game player reserve has max_rounds (3) cards for each player
    let reserve_count = sqlx::query("SELECT COUNT(*) FROM game_player_reserve WHERE game_id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(reserve_count, 9); // 3 players * 3 reserve cards

    // Check that all selected cards (3 situations + 24 memes = 27 items) are locked in game_content_locks
    let locks_count = sqlx::query("SELECT COUNT(*) FROM game_content_locks WHERE game_id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(locks_count, 27);
}

#[tokio::test]
async fn test_game_settings_update() {
    let (pool, app) = setup_db_and_router().await;

    // Create 2 guest users
    let (_s1, b1) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token1 = serde_json::from_slice::<RestApiResponse<Value>>(&b1)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (_s2, b2) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token2 = serde_json::from_slice::<RestApiResponse<Value>>(&b2)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // Create situation pack
    let sit_create = CreateSituationPackRequest {
        name: "Settings Test Situations".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        prompts: vec![
            "S1".to_string(),
            "S2".to_string(),
            "S3".to_string(),
            "S4".to_string(),
            "S5".to_string(),
        ],
    };
    let (_, sb) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token1),
        Some(&sit_create),
    )
    .await;
    let sit_pack_id = serde_json::from_slice::<RestApiResponse<CreateSituationPackResponse>>(&sb)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Create meme pack
    let claims1 = decode::<Claims>(&token1, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id1 = Uuid::parse_str(&claims1.sub).unwrap();
    sqlx::query("INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility) VALUES (4001, $1, 'hackclub_cdn', 'p_41', 'https://example.com/41.png', '41.png', 'image/png', 1024, 'pending', 'private') ON CONFLICT DO NOTHING").bind(user_id1).execute(&pool).await.unwrap();
    let meme_create = CreateMemePackRequest {
        name: "Settings Test Memes".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: vec![4001],
    };
    let (_, bm) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token1),
        Some(&meme_create),
    )
    .await;
    let meme_pack_id = serde_json::from_slice::<RestApiResponse<CreateMemePackResponse>>(&bm)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Create game with max_rounds: 3, hand_size: 5
    let create_game_payload = CreateGameRequest {
        mode: GameMode::SituationToMeme,
        selected_situation_pack_ids: vec![sit_pack_id],
        selected_meme_pack_ids: vec![meme_pack_id],
        max_rounds: 3,
        hand_size: 5,
        handle: None,
    };
    let (sg, bg) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token1),
        Some(&create_game_payload),
    )
    .await;
    assert_eq!(sg, StatusCode::OK);
    let game_id = serde_json::from_slice::<RestApiResponse<GameDto>>(&bg)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Test PATCH /games/{id} by guest 2 (non-host) -> should fail with 403 Forbidden
    let patch_payload = UpdateGameRequest {
        mode: Some(GameMode::MemeToSituation),
        selected_situation_pack_ids: None,
        selected_meme_pack_ids: None,
        max_rounds: Some(5),
        hand_size: Some(6),
    };
    let (spatch1, _) = send_request(
        &app,
        Method::PATCH,
        &format!("/games/{}", game_id),
        Some(&token2),
        Some(&patch_payload),
    )
    .await;
    assert_eq!(spatch1, StatusCode::FORBIDDEN);

    // Test PATCH /games/{id} by host (guest 1) -> should succeed with 200 OK
    let (spatch2, bpatch2) = send_request(
        &app,
        Method::PATCH,
        &format!("/games/{}", game_id),
        Some(&token1),
        Some(&patch_payload),
    )
    .await;
    assert_eq!(spatch2, StatusCode::OK);
    let patch_dto: RestApiResponse<GameDto> = serde_json::from_slice(&bpatch2).unwrap();
    let patched_game = patch_dto.0.data.unwrap();
    assert_eq!(patched_game.mode, GameMode::MemeToSituation);

    // Verify database has updated settings
    let game_row =
        sqlx::query("SELECT mode::TEXT AS mode, max_rounds, hand_size FROM games WHERE id = $1")
            .bind(game_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(game_row.get::<String, _>("mode"), "meme_to_situation");
    assert_eq!(game_row.get::<i32, _>("max_rounds"), 5);
    assert_eq!(game_row.get::<i32, _>("hand_size"), 6);
}

#[tokio::test]
async fn test_game_play_to_completion_deletes_locks() {
    let (pool, app) = setup_db_and_router().await;

    // Create 3 guest users
    let (_s1, b1) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token1 = serde_json::from_slice::<RestApiResponse<Value>>(&b1)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (_s2, b2) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token2 = serde_json::from_slice::<RestApiResponse<Value>>(&b2)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let (_s3, b3) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    let token3 = serde_json::from_slice::<RestApiResponse<Value>>(&b3)
        .unwrap()
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let claims1 = decode::<Claims>(&token1, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id1 = Uuid::parse_str(&claims1.sub).unwrap();
    let claims2 = decode::<Claims>(&token2, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id2 = Uuid::parse_str(&claims2.sub).unwrap();
    let claims3 = decode::<Claims>(&token3, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id3 = Uuid::parse_str(&claims3.sub).unwrap();

    // Insert 6 memes and 1 situation for max_rounds: 1, hand_size: 1 (required memes = 3*1 + 3*1 = 6, required situations = 1)
    for id in 5001..=5006 {
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

    // Create Meme Pack with 6 memes
    let meme_create = CreateMemePackRequest {
        name: "Completion Test Memes".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: (5001..=5006).collect(),
    };
    let (_, bm) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token1),
        Some(&meme_create),
    )
    .await;
    let meme_pack_id = serde_json::from_slice::<RestApiResponse<CreateMemePackResponse>>(&bm)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Create Situation Pack with 1 situation
    let sit_create = CreateSituationPackRequest {
        name: "Completion Test Situations".to_string(),
        description: None,
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        prompts: vec!["Completion prompt".to_string()],
    };
    let (_, sb) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token1),
        Some(&sit_create),
    )
    .await;
    let sit_pack_id = serde_json::from_slice::<RestApiResponse<CreateSituationPackResponse>>(&sb)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Create Game with max_rounds: 1, hand_size: 1
    let create_game_payload = CreateGameRequest {
        mode: GameMode::SituationToMeme,
        selected_situation_pack_ids: vec![sit_pack_id],
        selected_meme_pack_ids: vec![meme_pack_id],
        max_rounds: 1,
        hand_size: 1,
        handle: None,
    };
    let (_, bg) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token1),
        Some(&create_game_payload),
    )
    .await;
    let game_id = serde_json::from_slice::<RestApiResponse<GameDto>>(&bg)
        .unwrap()
        .0
        .data
        .unwrap()
        .id;

    // Players join
    send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token2),
        None,
    )
    .await;
    send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token3),
        None,
    )
    .await;

    // Ready up
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token1),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token2),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game_id),
        Some(&token3),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;

    // Start game
    let (sstart, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/start", game_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(sstart, StatusCode::OK);

    // Verify content locks are populated (7 locks: 6 memes + 1 situation)
    let locks_count = sqlx::query("SELECT COUNT(*) FROM game_content_locks WHERE game_id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get::<i64, _>(0);
    assert_eq!(locks_count, 7);

    // Get game state to resolve round_id and player hands
    let (_, state_bytes1) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token1),
        None,
    )
    .await;
    let state1 = serde_json::from_slice::<RestApiResponse<GameStateDto>>(&state_bytes1)
        .unwrap()
        .0
        .data
        .unwrap();
    let round_id = state1.round.as_ref().unwrap().id;
    let card1_id = match &state1.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    let (_, state_bytes2) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token2),
        None,
    )
    .await;
    let state2 = serde_json::from_slice::<RestApiResponse<GameStateDto>>(&state_bytes2)
        .unwrap()
        .0
        .data
        .unwrap();
    let card2_id = match &state2.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    let (_, state_bytes3) = send_request::<()>(
        &app,
        Method::GET,
        &format!("/games/{}/state", game_id),
        Some(&token3),
        None,
    )
    .await;
    let state3 = serde_json::from_slice::<RestApiResponse<GameStateDto>>(&state_bytes3)
        .unwrap()
        .0
        .data
        .unwrap();
    let card3_id = match &state3.my_hand[0] {
        GameCard::Meme { id, .. } => *id,
        GameCard::Situation { id, .. } => *id,
    };

    // Submissions
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/submit", game_id),
        Some(&token1),
        Some(&SubmitCardRequest { card_id: card1_id }),
    )
    .await;
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/submit", game_id),
        Some(&token2),
        Some(&SubmitCardRequest { card_id: card2_id }),
    )
    .await;
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/submit", game_id),
        Some(&token3),
        Some(&SubmitCardRequest { card_id: card3_id }),
    )
    .await;

    // Resolve submissions
    let db_subs = sqlx::query("SELECT id, user_id FROM round_submissions WHERE round_id = $1")
        .bind(round_id)
        .fetch_all(&pool)
        .await
        .unwrap();
    let sub1 = db_subs
        .iter()
        .find(|s| s.get::<Uuid, _>("user_id") == user_id1)
        .unwrap()
        .get::<Uuid, _>("id");
    let sub2 = db_subs
        .iter()
        .find(|s| s.get::<Uuid, _>("user_id") == user_id2)
        .unwrap()
        .get::<Uuid, _>("id");
    let sub3 = db_subs
        .iter()
        .find(|s| s.get::<Uuid, _>("user_id") == user_id3)
        .unwrap()
        .get::<Uuid, _>("id");

    // 10. Verify self-voting constraint: Player 1 votes for Player 1's submission (sub1) -> should fail
    let (self_vote_status, self_vote_bytes) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token1),
        Some(&VoteRequest {
            submission_id: sub1,
        }),
    )
    .await;
    assert_eq!(self_vote_status, StatusCode::BAD_REQUEST);
    let self_vote_body =
        serde_json::from_slice::<RestApiResponse<Value>>(&self_vote_bytes).unwrap();
    assert!(self_vote_body
        .0
        .message
        .contains("Cannot vote for your own submission"));

    // Real votes
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token1),
        Some(&VoteRequest {
            submission_id: sub2,
        }),
    )
    .await;
    send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token2),
        Some(&VoteRequest {
            submission_id: sub3,
        }),
    )
    .await;
    let (svote, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/vote", game_id),
        Some(&token3),
        Some(&VoteRequest {
            submission_id: sub1,
        }),
    )
    .await;
    assert_eq!(svote, StatusCode::OK);

    // Verify game status is finished
    let game_row = sqlx::query("SELECT status::TEXT AS status FROM games WHERE id = $1")
        .bind(game_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(game_row.get::<String, _>("status"), "finished");

    // Verify all game content locks are deleted
    let locks_count_after =
        sqlx::query("SELECT COUNT(*) FROM game_content_locks WHERE game_id = $1")
            .bind(game_id)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get::<i64, _>(0);
    assert_eq!(locks_count_after, 0);
}

#[tokio::test]
async fn test_game_catalog_endpoint() {
    let (pool, app) = setup_db_and_router().await;

    // Clean up games table to isolate this test's catalog assertions
    sqlx::query("DELETE FROM games")
        .execute(&pool)
        .await
        .unwrap();

    // Create guest users
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

    let (status3, bytes3) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status3, StatusCode::OK);
    let auth_resp3: RestApiResponse<Value> = serde_json::from_slice(&bytes3).unwrap();
    let token3 = auth_resp3
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    let claims1 = decode::<Claims>(&token1, &KEYS.decoding, &Validation::default())
        .unwrap()
        .claims;
    let user_id1 = Uuid::parse_str(&claims1.sub).unwrap();

    // Setup 24 dummy media assets
    for id in 3001..=3024 {
        sqlx::query(
            "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
             VALUES ($1, $2, 'hackclub_cdn', $3, $4, $5, 'image/png', 1024, 'pending', 'private')
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(id as i64)
        .bind(user_id1)
        .bind(format!("prov_catalog_id_{}", id))
        .bind(format!("https://example.com/catalog_{}.png", id))
        .bind(format!("meme_catalog_{}.png", id))
        .execute(&pool)
        .await
        .unwrap();
    }

    // Create Meme Pack
    let meme_pack_payload = CreateMemePackRequest {
        name: "Catalog Meme Pack".to_string(),
        description: Some("Description".to_string()),
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        media_ids: (3001..=3024).collect(),
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

    // Create Situation Pack
    let sit_pack_payload = CreateSituationPackRequest {
        name: "Catalog Situation Pack".to_string(),
        description: Some("Description".to_string()),
        language_code: LanguageCode::Ru,
        safety_level: ContentSafetyLevel::FamilyFriendly,
        is_public: false,
        prompts: vec![
            "When catalog test works".to_string(),
            "When we list active games".to_string(),
            "When the third situation is added".to_string(),
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

    // Query active games when there is none from THIS test yet
    // (can't assert 0 — other parallel tests may have created lobbies)
    let (status_list1, _bytes_list1) =
        send_request::<()>(&app, Method::GET, "/games", Some(&token1), None).await;
    assert_eq!(status_list1, StatusCode::OK);
    // Just verify the endpoint responds OK — we'll check specific game_id after creation

    // Verify ws-token endpoint returns tokens
    let (status_ws_token, bytes_ws_token) = send_request::<()>(
        &app,
        Method::GET,
        "/games/catalog/ws-token",
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(status_ws_token, StatusCode::OK);
    let ws_token_resp: RestApiResponse<LobbiesWsTokenDto> =
        serde_json::from_slice(&bytes_ws_token).unwrap();
    let ws_token_data = ws_token_resp.0.data.unwrap();
    assert!(!ws_token_data.connection_token.is_empty());
    assert!(!ws_token_data.lobbies_subscription_token.is_empty());

    // Create Game 1
    let create_payload = CreateGameRequest {
        mode: GameMode::SituationToMeme,
        selected_situation_pack_ids: vec![sit_pack_id],
        selected_meme_pack_ids: vec![meme_pack_id],
        max_rounds: 3,
        hand_size: 5,
        handle: None,
    };
    let (status_game1, bytes_game1) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token1),
        Some(&create_payload),
    )
    .await;
    assert_eq!(status_game1, StatusCode::OK);
    let game1_resp: RestApiResponse<GameDto> = serde_json::from_slice(&bytes_game1).unwrap();
    let game1_id = game1_resp.0.data.unwrap().id;

    // Query active games: Game 1 should be present with player count = 1
    let (status_list2, bytes_list2) =
        send_request::<()>(&app, Method::GET, "/games", Some(&token1), None).await;
    assert_eq!(status_list2, StatusCode::OK);
    let list_resp2: RestApiResponse<ActiveGamesResponseDto> =
        serde_json::from_slice(&bytes_list2).unwrap();
    let data2 = list_resp2.0.data.unwrap();
    let active_games = data2.games;
    // Filter to only OUR game to avoid interference from parallel tests
    let our_game = active_games
        .iter()
        .find(|g| g.id == game1_id)
        .expect("Game 1 should be in the active games list");
    assert_eq!(our_game.players_count, 1);

    // Player 2 joins Game 1
    let (status_join, _) = send_request::<Value>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game1_id),
        Some(&token2),
        None,
    )
    .await;
    assert_eq!(status_join, StatusCode::OK);

    // Player 3 joins Game 1
    let (status_join3, _) = send_request::<Value>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game1_id),
        Some(&token3),
        None,
    )
    .await;
    assert_eq!(status_join3, StatusCode::OK);

    // Query active games: Game 1 should be present with player count = 3
    let (status_list3, bytes_list3) =
        send_request::<()>(&app, Method::GET, "/games", Some(&token1), None).await;
    assert_eq!(status_list3, StatusCode::OK);
    let list_resp3: RestApiResponse<ActiveGamesResponseDto> =
        serde_json::from_slice(&bytes_list3).unwrap();
    let data3 = list_resp3.0.data.unwrap();
    let active_games3 = data3.games;
    // Filter to only OUR game to avoid interference from parallel tests
    let our_game3 = active_games3
        .iter()
        .find(|g| g.id == game1_id)
        .expect("Game 1 should still be in the active games list");
    assert_eq!(our_game3.players_count, 3);

    // Players ready up and start the game to change status to playing
    let (status_ready1, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game1_id),
        Some(&token1),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(status_ready1, StatusCode::OK);

    let (status_ready2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game1_id),
        Some(&token2),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(status_ready2, StatusCode::OK);

    let (status_ready3, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/ready", game1_id),
        Some(&token3),
        Some(&ReadyRequest { is_ready: true }),
    )
    .await;
    assert_eq!(status_ready3, StatusCode::OK);

    let (status_start, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/start", game1_id),
        Some(&token1),
        None,
    )
    .await;
    assert_eq!(status_start, StatusCode::OK);

    // Query active games: Game 1 should no longer be listed because status is 'playing'
    let (status_list4, bytes_list4) =
        send_request::<()>(&app, Method::GET, "/games", Some(&token1), None).await;
    assert_eq!(status_list4, StatusCode::OK);
    let list_resp4: RestApiResponse<ActiveGamesResponseDto> =
        serde_json::from_slice(&bytes_list4).unwrap();
    let data4 = list_resp4.0.data.unwrap();
    // Our game should no longer appear in the lobby list
    assert!(
        !data4.games.iter().any(|g| g.id == game1_id),
        "Started game should not appear in lobby list"
    );
}

#[tokio::test]
async fn test_game_handle_conflicts() {
    let (pool, app) = setup_db_and_router().await;

    // 1. Create Guest Users with unique usernames
    let name2 = format!("hero-{}", Uuid::new_v4());
    let name3 = format!("villain-{}", Uuid::new_v4());

    let (status1, bytes1) = send_request::<()>(&app, Method::POST, "/auth/guest", None, None).await;
    assert_eq!(status1, StatusCode::OK);
    let token1 = serde_json::from_slice::<RestApiResponse<Value>>(&bytes1).unwrap().0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let (status2, bytes2) = send_request(
        &app,
        Method::POST,
        "/auth/guest",
        None,
        Some(&json!({ "username": name2 })),
    ).await;
    assert_eq!(status2, StatusCode::OK);
    let token2 = serde_json::from_slice::<RestApiResponse<Value>>(&bytes2).unwrap().0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let (status3, bytes3) = send_request(
        &app,
        Method::POST,
        "/auth/guest",
        None,
        Some(&json!({ "username": name3 })),
    ).await;
    assert_eq!(status3, StatusCode::OK);
    let token3 = serde_json::from_slice::<RestApiResponse<Value>>(&bytes3).unwrap().0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();
    let claims3 = decode::<Claims>(&token3, &KEYS.decoding, &Validation::default()).unwrap().claims;
    let user_id3 = claims3.sub.clone();

    // 2. Setup packs
    let mut media_ids = Vec::new();
    for id in 4001..=4006 {
        sqlx::query(
            "INSERT INTO media_assets (id, owner_user_id, provider, provider_file_id, url, filename, content_type, size_bytes, status, visibility)
             VALUES ($1, $2::uuid, $3, $4, $5, $6, 'image/png', 1024, 'pending', 'private')
             ON CONFLICT (id) DO NOTHING"
        )
        .bind(id as i64)
        .bind(Uuid::parse_str(&claims3.sub).unwrap())
        .bind("hackclub_cdn")
        .bind(format!("prov_conflict_id_{}", id))
        .bind(format!("https://example.com/conflict_{}.png", id))
        .bind(format!("meme_conflict_{}.png", id))
        .execute(&pool)
        .await
        .unwrap();
        media_ids.push(id as i64);
    }

    let (status_create_meme, bytes_create_meme) = send_request(
        &app,
        Method::POST,
        "/games/packs/memes",
        Some(&token1),
        Some(&CreateMemePackRequest {
            name: "Conflict Meme Pack".to_string(),
            description: Some("Description".to_string()),
            language_code: LanguageCode::Ru,
            safety_level: ContentSafetyLevel::FamilyFriendly,
            is_public: true,
            media_ids,
        }),
    )
    .await;
    assert_eq!(status_create_meme, StatusCode::OK);
    let meme_pack_id = serde_json::from_slice::<RestApiResponse<CreateMemePackResponse>>(&bytes_create_meme).unwrap().0.data.unwrap().id;

    let (status_create_sit, bytes_create_sit) = send_request(
        &app,
        Method::POST,
        "/games/packs/situations",
        Some(&token1),
        Some(&CreateSituationPackRequest {
            name: "Conflict Situation Pack".to_string(),
            description: Some("Description".to_string()),
            language_code: LanguageCode::Ru,
            safety_level: ContentSafetyLevel::FamilyFriendly,
            is_public: true,
            prompts: vec!["Prompt 1".to_string(), "Prompt 2".to_string()],
        }),
    )
    .await;
    assert_eq!(status_create_sit, StatusCode::OK);
    let sit_pack_id = serde_json::from_slice::<RestApiResponse<CreateSituationPackResponse>>(&bytes_create_sit).unwrap().0.data.unwrap().id;

    // 3. Create Game
    let (status_game, bytes_game) = send_request(
        &app,
        Method::POST,
        "/games",
        Some(&token1),
        Some(&CreateGameRequest {
            mode: GameMode::SituationToMeme,
            selected_situation_pack_ids: vec![sit_pack_id],
            selected_meme_pack_ids: vec![meme_pack_id],
            max_rounds: 3,
            hand_size: 5,
            handle: None,
        }),
    )
    .await;
    assert_eq!(status_game, StatusCode::OK);
    let game_id = serde_json::from_slice::<RestApiResponse<GameDto>>(&bytes_game).unwrap().0.data.unwrap().id;

    // 4. User 2 joins with explicit handle equal to User 3's nickname (name3)
    let (join_status2, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token2),
        Some(&json!({ "handle": name3 })),
    )
    .await;
    assert_eq!(join_status2, StatusCode::OK);

    // 5. User 3 joins with explicit handle equal to User 3's nickname (name3) -> should yield 409 Conflict because User 2 already claimed it
    let (join_status3_fail, _) = send_request(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token3),
        Some(&json!({ "handle": name3 })),
    )
    .await;
    assert_eq!(join_status3_fail, StatusCode::CONFLICT);

    // 6. User 3 joins with None (implicit handle) -> should resolve to user_id3 (UUID string) because their persistent nickname (name3) conflicts with User 2's chosen handle in this lobby
    let (join_status3_ok, _) = send_request::<()>(
        &app,
        Method::POST,
        &format!("/games/{}/join", game_id),
        Some(&token3),
        None,
    )
    .await;
    assert_eq!(join_status3_ok, StatusCode::OK);

    // Verify resolved handle in DB is user_id3
    let handle_in_db: String = sqlx::query_scalar("SELECT handle FROM game_players WHERE game_id = $1 AND user_id = $2::uuid")
        .bind(game_id)
        .bind(&user_id3)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(handle_in_db, user_id3);
}

