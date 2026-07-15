use axum::http::StatusCode;
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
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
};

#[tokio::test]
async fn test_user_routes_lifecycle() {
    dotenvy::dotenv().ok();
    let _ = tracing_subscriber::fmt::try_init();

    // 1. Load configuration and connect to the test DB
    let config = Config::from_env().unwrap();
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .min_connections(1)
        .connect(&config.database_url)
        .await
        .unwrap();
    run_database_migrations(&pool).await.unwrap();

    // 2. Start the application router on an ephemeral port
    let app_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let app_addr = app_listener.local_addr().unwrap();
    let state = build_app_state(pool.clone(), config);
    let app = create_router(state);
    tokio::spawn(async move {
        axum::serve(app_listener, app).await.unwrap();
    });

    let client = reqwest::Client::new();
    let base_url = format!("http://{}", app_addr);

    // Register a test user
    let username1 = format!("u1-{}", Uuid::new_v4());
    let resp1 = client.post(format!("{}/auth/register", base_url))
        .json(&json!({
            "username": username1,
            "password": "password123"
        }))
        .send().await.unwrap();
    assert_eq!(resp1.status(), StatusCode::OK);

    // Login to get access token
    let login_resp = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": username1,
            "password": "password123"
        }))
        .send().await.unwrap();
    assert_eq!(login_resp.status(), StatusCode::OK);
    let login_body: RestApiResponse<Value> = login_resp.json().await.unwrap();
    let access_token1 = login_body.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    // --- 1. Test GET /user/me ---
    let me_resp = client.get(format!("{}/user/me", base_url))
        .bearer_auth(&access_token1)
        .send().await.unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);
    let me_body: RestApiResponse<Value> = me_resp.json().await.unwrap();
    let me_data = me_body.0.data.unwrap();
    let user_id1 = me_data.get("id").unwrap().as_str().unwrap().to_string();
    assert_eq!(me_data.get("username").unwrap().as_str().unwrap(), username1);

    // --- 2. Test PATCH /user/me ---
    let updated_username = format!("u1-new-{}", Uuid::new_v4());
    let patch_resp = client.patch(format!("{}/user/me", base_url))
        .bearer_auth(&access_token1)
        .json(&json!({
            "username": updated_username
        }))
        .send().await.unwrap();
    assert_eq!(patch_resp.status(), StatusCode::OK);
    let patch_body: RestApiResponse<Value> = patch_resp.json().await.unwrap();
    assert_eq!(patch_body.0.data.unwrap().get("username").unwrap().as_str().unwrap(), updated_username);

    // --- 3. Test GET /user/{id} ---
    let get_by_id_resp = client.get(format!("{}/user/{}", base_url, user_id1))
        .bearer_auth(&access_token1)
        .send().await.unwrap();
    assert_eq!(get_by_id_resp.status(), StatusCode::OK);
    let get_by_id_body: RestApiResponse<Value> = get_by_id_resp.json().await.unwrap();
    assert_eq!(get_by_id_body.0.data.unwrap().get("username").unwrap().as_str().unwrap(), updated_username);

    // --- 4. Test POST /user/{id}/promote-admin ---
    // Register another user to promote
    let username2 = format!("u2-{}", Uuid::new_v4());
    let resp2 = client.post(format!("{}/auth/register", base_url))
        .json(&json!({
            "username": username2,
            "password": "password123"
        }))
        .send().await.unwrap();
    assert_eq!(resp2.status(), StatusCode::OK);

    // Login user 2 to get their ID
    let login_resp2 = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": username2,
            "password": "password123"
        }))
        .send().await.unwrap();
    let login_body2: RestApiResponse<Value> = login_resp2.json().await.unwrap();
    let access_token2 = login_body2.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let me_resp2 = client.get(format!("{}/user/me", base_url))
        .bearer_auth(&access_token2)
        .send().await.unwrap();
    let me_body2: RestApiResponse<Value> = me_resp2.json().await.unwrap();
    let user_id2 = me_body2.0.data.unwrap().get("id").unwrap().as_str().unwrap().to_string();

    // Promote user 2 using user 1's token (promote-admin is currently unrestricted by role)
    let promote_resp = client.post(format!("{}/user/{}/promote-admin", base_url, user_id2))
        .bearer_auth(&access_token1)
        .send().await.unwrap();
    assert_eq!(promote_resp.status(), StatusCode::OK);

    // Verify user 2's role is now admin in DB
    let role: String = sqlx::query_scalar("SELECT role::text FROM users WHERE id = $1::uuid")
        .bind(&user_id2)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(role, "admin");
}
