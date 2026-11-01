use axum::http::StatusCode;
use meme_battle_backend::{
    app::create_router,
    common::{
        app::{
            bootstrap::{build_app_state, run_database_migrations},
            config::Config,
        },
        http::dto::RestApiResponse,
    },
    features::media::{
        api::dto::response::ImportTelegramStickersResponseDto, TelegramBotClient,
        TelegramStickerClient,
    },
};
use serde_json::{json, Value};
use sqlx::{postgres::PgPoolOptions, Row};

#[tokio::test]
#[ignore = "requires real TELEGRAM_BOT_TOKEN and network connection"]
async fn test_live_telegram_connection() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("Config must load from .env");
    let token = config
        .telegram_bot_token
        .expect("TELEGRAM_BOT_TOKEN must be set in .env");

    let client = TelegramBotClient::new(token, config.telegram_bot_api_base_url);

    // 1. Fetch real public sticker set from Telegram
    let set = client
        .get_sticker_set("animals")
        .await
        .expect("Sticker set 'animals' must be fetched successfully from Telegram");

    println!("Sticker set title: {}", set.title);
    println!("Total stickers: {}", set.stickers.len());
    assert!(!set.stickers.is_empty(), "Stickers must not be empty");

    // 2. Find first static sticker
    let static_sticker = set
        .stickers
        .iter()
        .find(|s| !s.is_animated && !s.is_video)
        .expect("Must have at least one static sticker");

    println!(
        "Found static sticker with file_id: {}",
        static_sticker.file_id
    );

    // 3. Test downloading the sticker bytes
    let file = client
        .download_sticker_file(&static_sticker.file_id)
        .await
        .expect("Must download sticker file bytes from Telegram");

    println!("Downloaded filename: {}", file.filename);
    println!("Downloaded size: {} bytes", file.bytes.len());
    assert!(!file.bytes.is_empty(), "Bytes cannot be empty");
    assert_eq!(file.content_type, "image/webp");
}

#[tokio::test]
#[ignore = "requires real TELEGRAM_BOT_TOKEN and network connection"]
async fn test_live_import_endpoint_end_to_end() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().expect("Config must load from .env");

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .expect("Failed to connect to DB");

    run_database_migrations(&pool).await.unwrap();

    let state = build_app_state(pool.clone(), config);
    let app = create_router(state);

    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // 1. Authenticate as a guest user
    let guest_resp = client
        .post(format!("{}/auth/guest", base_url))
        .send()
        .await
        .unwrap();
    assert_eq!(guest_resp.status(), StatusCode::OK);
    let guest_body: RestApiResponse<Value> = guest_resp.json().await.unwrap();
    let token = guest_body
        .0
        .data
        .unwrap()
        .get("access_token")
        .unwrap()
        .as_str()
        .unwrap()
        .to_string();

    // 2. Call POST /media/import/telegram with real telegram sticker pack
    let resp = client
        .post(format!("{}/media/import/telegram", base_url))
        .bearer_auth(&token)
        .json(&json!({ "url": "https://t.me/addstickers/animals" }))
        .send()
        .await
        .unwrap();

    assert_eq!(resp.status(), StatusCode::OK);
    let body: RestApiResponse<ImportTelegramStickersResponseDto> = resp.json().await.unwrap();
    let data = body.0.data.expect("Must have imported data");

    println!("Imported pack name: {}", data.name);
    println!("Imported pack title: {}", data.title);
    println!("Total imported stickers: {}", data.total_imported);
    println!("Skipped animated: {}", data.skipped_animated);

    assert!(data.total_imported > 0);
    assert!(!data.stickers.is_empty());

    let first = &data.stickers[0];
    println!("First sticker media_id: {}", first.media_id);
    println!("First sticker URL: {}", first.url);
    println!("First sticker emoji: {:?}", first.emoji);

    assert!(first.media_id > 0);
    assert!(first.url.starts_with("https://cdn.hackclub.com/"));

    // 3. Verify in database that it was saved with status 'pending'
    let row = sqlx::query(
        "SELECT status::text AS status, provider::text AS provider FROM media_assets WHERE id = $1",
    )
    .bind(first.media_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let status: String = row.get("status");
    let provider: String = row.get("provider");

    assert_eq!(status, "pending");
    assert_eq!(provider, "hackclub_cdn");
}
