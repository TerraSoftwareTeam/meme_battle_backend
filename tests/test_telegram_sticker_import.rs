use axum::{
    extract::{Path, Query},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, PgPool, Row};
use std::net::SocketAddr;
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
    features::media::api::dto::response::ImportTelegramStickersResponseDto,
};

#[derive(Deserialize)]
struct GetStickerSetQuery {
    name: Option<String>,
}

#[derive(Deserialize)]
struct GetFileQuery {
    file_id: Option<String>,
}

async fn handle_mock_get_sticker_set(Query(query): Query<GetStickerSetQuery>) -> impl IntoResponse {
    let name = query.name.unwrap_or_default();
    if name == "cool_animals" {
        (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "result": {
                    "name": "cool_animals",
                    "title": "Cool Animals",
                    "stickers": [
                        {
                            "file_id": "tg_file_static_1",
                            "file_unique_id": "u1",
                            "width": 512,
                            "height": 512,
                            "is_animated": false,
                            "is_video": false,
                            "emoji": "🐱"
                        },
                        {
                            "file_id": "tg_file_anim_2",
                            "file_unique_id": "u2",
                            "width": 512,
                            "height": 512,
                            "is_animated": true,
                            "is_video": false,
                            "emoji": "🔥"
                        }
                    ]
                }
            })),
        )
    } else if name == "all_animated" {
        (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "result": {
                    "name": "all_animated",
                    "title": "All Animated",
                    "stickers": [
                        {
                            "file_id": "tg_file_anim_only",
                            "file_unique_id": "u_anim",
                            "width": 512,
                            "height": 512,
                            "is_animated": true,
                            "is_video": false,
                            "emoji": "✨"
                        }
                    ]
                }
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "ok": false,
                "error_code": 404,
                "description": "STICKERSET_INVALID"
            })),
        )
    }
}

async fn handle_mock_get_file(Query(query): Query<GetFileQuery>) -> impl IntoResponse {
    let file_id = query.file_id.unwrap_or_default();
    if file_id == "tg_file_static_1" {
        (
            StatusCode::OK,
            Json(json!({
                "ok": true,
                "result": {
                    "file_id": "tg_file_static_1",
                    "file_path": "stickers/cool_cat.webp"
                }
            })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(json!({
                "ok": false,
                "error_code": 404,
                "description": "FILE_NOT_FOUND"
            })),
        )
    }
}

async fn handle_mock_download_file(
    Path((_token, _dir, _file)): Path<(String, String, String)>,
) -> impl IntoResponse {
    let dummy_webp_bytes = vec![82, 73, 70, 70, 0, 0, 0, 0, 87, 69, 66, 80];
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "image/webp".parse().unwrap());
    (StatusCode::OK, headers, dummy_webp_bytes)
}

fn mock_telegram_router() -> Router {
    Router::new()
        .route(
            "/bot{token}/getStickerSet",
            get(handle_mock_get_sticker_set),
        )
        .route("/bot{token}/getFile", get(handle_mock_get_file))
        .route(
            "/file/bot{token}/{dir}/{file}",
            get(handle_mock_download_file),
        )
}

async fn handle_mock_upload() -> Json<Value> {
    let file_id = Uuid::new_v4().to_string();
    Json(json!({
        "id": file_id,
        "filename": "cool_cat.webp",
        "size": 12,
        "content_type": "image/webp",
        "url": format!("https://cdn.hackclub.com/{}.webp", file_id)
    }))
}

async fn handle_mock_delete(Path(_id): Path<String>) -> Json<Value> {
    Json(json!({
        "deleted": true
    }))
}

fn mock_cdn_router() -> Router {
    Router::new()
        .route("/api/v4/upload", post(handle_mock_upload))
        .route("/api/v4/upload/{id}", delete(handle_mock_delete))
}

async fn setup_test_servers() -> (SocketAddr, SocketAddr) {
    let tg_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let tg_addr = tg_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(tg_listener, mock_telegram_router())
            .await
            .unwrap();
    });

    let cdn_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let cdn_addr = cdn_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(cdn_listener, mock_cdn_router()).await.unwrap();
    });

    (tg_addr, cdn_addr)
}

async fn setup_app() -> (PgPool, String) {
    dotenvy::dotenv().ok();
    let (tg_addr, cdn_addr) = setup_test_servers().await;

    let mut config = Config::from_env().unwrap();
    config.telegram_bot_token = Some("test_bot_token".to_string());
    config.telegram_bot_api_base_url = format!("http://{}", tg_addr);
    config.hackclub_cdn_base_url = format!("http://{}", cdn_addr);
    config.hackclub_cdn_api_key = Some("test_api_key".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();

    run_database_migrations(&pool).await.unwrap();

    let state = build_app_state(pool.clone(), config);
    let app = create_router(state);

    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    (pool, format!("http://{}", app_addr))
}

#[tokio::test]
async fn test_import_telegram_stickers_end_to_end() {
    let (pool, base_url) = setup_app().await;
    let client = reqwest::Client::new();

    // 1. Unauthenticated request must return 401 Unauthorized
    let resp_unauth = client
        .post(format!("{}/media/import/telegram", base_url))
        .json(&json!({ "url": "https://t.me/addstickers/cool_animals" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_unauth.status(), StatusCode::UNAUTHORIZED);

    // 2. Authenticate as a guest user
    let guest_resp = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(guest_resp.status(), StatusCode::OK);
    let guest_body: RestApiResponse<Value> = guest_resp.json().await.unwrap();
    let guest_data = guest_body.0.data.unwrap();
    let token = guest_data
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();
    let me_resp = client
        .get(format!("{}/user/me", base_url))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);
    let me_body: RestApiResponse<Value> = me_resp.json().await.unwrap();
    let user_id = me_body
        .0
        .data
        .unwrap()
        .get("id")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // 3. Test invalid URL format -> 400 Bad Request
    let resp_invalid = client
        .post(format!("{}/media/import/telegram", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "url": "not a valid pack" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_invalid.status(), StatusCode::BAD_REQUEST);

    // 4. Test non-existent pack -> 404 Not Found
    let resp_not_found = client
        .post(format!("{}/media/import/telegram", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "url": "non_existent_pack_123" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_not_found.status(), StatusCode::NOT_FOUND);

    // 5. Test pack with only animated stickers -> 400 Bad Request (Validation error)
    let resp_all_anim = client
        .post(format!("{}/media/import/telegram", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "url": "tg://addstickers?set=all_animated" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_all_anim.status(), StatusCode::BAD_REQUEST);

    // 6. Test successful import with static + animated mix
    let resp_success = client
        .post(format!("{}/media/import/telegram", base_url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&json!({ "url": "https://t.me/addstickers/cool_animals" }))
        .send()
        .await
        .unwrap();
    assert_eq!(resp_success.status(), StatusCode::OK);

    let body: RestApiResponse<ImportTelegramStickersResponseDto> =
        resp_success.json().await.unwrap();
    let data = body.0.data.expect("Response data must be present");

    assert_eq!(data.name, "cool_animals");
    assert_eq!(data.title, "Cool Animals");
    assert_eq!(data.total_imported, 1);
    assert_eq!(data.skipped_animated, 1);
    assert_eq!(data.stickers.len(), 1);

    let sticker = &data.stickers[0];
    assert!(sticker.media_id > 0);
    assert!(sticker.url.starts_with("https://cdn.hackclub.com/"));
    assert_eq!(sticker.emoji.as_deref(), Some("🐱"));
    assert_eq!(sticker.width, 512);
    assert_eq!(sticker.height, 512);

    // 7. Verify database record
    let row = sqlx::query(
        "SELECT status::text AS status, owner_user_id::text AS owner_user_id, provider::text AS provider FROM media_assets WHERE id = $1"
    )
    .bind(sticker.media_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let status: String = row.get("status");
    let owner: String = row.get("owner_user_id");
    let provider: String = row.get("provider");

    assert_eq!(status, "pending");
    assert_eq!(owner, user_id);
    assert_eq!(provider, "hackclub_cdn");
}
