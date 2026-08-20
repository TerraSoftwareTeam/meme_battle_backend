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
async fn test_auth_routes_lifecycle() {
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

    // --- 1. Test /auth/guest (gets auto-generated player-{uuid} username in profile, null in DB) ---
    let resp = client.post(format!("{}/auth/guest", base_url))
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let auth_body: RestApiResponse<Value> = resp.json().await.unwrap();
    let data = auth_body.0.data.unwrap();
    let access_token = data.get("access_token").unwrap().as_str().unwrap().to_string();
    let refresh_token = data.get("refresh_token").unwrap().as_str().unwrap().to_string();

    // Verify the guest user has a player-{uuid} username in API response and is_guest is true
    let me_resp = client.get(format!("{}/user/me", base_url))
        .bearer_auth(&access_token)
        .send().await.unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);
    let me_body: RestApiResponse<Value> = me_resp.json().await.unwrap();
    let me_data = me_body.0.data.unwrap();
    let user_id = me_data.get("id").unwrap().as_str().unwrap().to_string();
    let username = me_data.get("username").unwrap().as_str().unwrap().to_string();
    assert!(username.starts_with("player-"));
    assert_eq!(me_data.get("is_guest").unwrap().as_bool().unwrap(), true);

    // Check DB directly — guest users have username = "player-{user_id}" and NULL password_hash in DB
    let db_username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1::uuid")
        .bind(&user_id)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(db_username, Some(format!("player-{}", user_id)));

    // --- 2. Convert guest into registered user via PATCH /user/me ---
    let claimed_username = format!("claimed-{}", Uuid::new_v4());
    let claimed_password = "securepassword123";
    let claim_resp = client.patch(format!("{}/user/me", base_url))
        .bearer_auth(&access_token)
        .json(&json!({
            "username": claimed_username,
            "password": claimed_password
        }))
        .send().await.unwrap();
    assert_eq!(claim_resp.status(), StatusCode::OK);
    let claim_body: RestApiResponse<Value> = claim_resp.json().await.unwrap();
    let claim_data = claim_body.0.data.unwrap();
    assert_eq!(claim_data.get("username").unwrap().as_str().unwrap(), claimed_username);
    assert_eq!(claim_data.get("is_guest").unwrap().as_bool().unwrap(), false);

    // Verify login with claimed credentials works
    let claim_login_resp = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": claimed_username,
            "password": claimed_password
        }))
        .send().await.unwrap();
    assert_eq!(claim_login_resp.status(), StatusCode::OK);

    // --- 3. Test /auth/register (register normal user) ---
    let reg_username = format!("user-{}", Uuid::new_v4());
    let reg_password = "testpassword123";
    let reg_resp = client.post(format!("{}/auth/register", base_url))
        .json(&json!({
            "username": reg_username,
            "password": reg_password
        }))
        .send().await.unwrap();
    assert_eq!(reg_resp.status(), StatusCode::OK);

    // Try registering the same username again (should fail)
    let dup_resp = client.post(format!("{}/auth/register", base_url))
        .json(&json!({
            "username": reg_username,
            "password": reg_password
        }))
        .send().await.unwrap();
    assert_eq!(dup_resp.status(), StatusCode::CONFLICT);

    // --- 4. Test /auth/login ---
    let login_resp = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": reg_username,
            "password": reg_password
        }))
        .send().await.unwrap();
    assert_eq!(login_resp.status(), StatusCode::OK);
    let login_body: RestApiResponse<Value> = login_resp.json().await.unwrap();
    let reg_access_token = login_body.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    // Try login with wrong password
    let bad_login_resp = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": reg_username,
            "password": "wrongpassword"
        }))
        .send().await.unwrap();
    assert_eq!(bad_login_resp.status(), StatusCode::UNAUTHORIZED);

    // --- 5. Test /auth/refresh ---
    let refresh_resp = client.post(format!("{}/auth/refresh", base_url))
        .json(&json!({
            "refresh_token": refresh_token
        }))
        .send().await.unwrap();
    assert_eq!(refresh_resp.status(), StatusCode::OK);
    let refresh_body: RestApiResponse<Value> = refresh_resp.json().await.unwrap();
    let new_access_token = refresh_body.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();
    assert!(!new_access_token.is_empty());

    // --- 6. Test password change via PATCH /user/me ---
    let new_password = "newsecretpassword";
    let change_pwd_resp = client.patch(format!("{}/user/me", base_url))
        .bearer_auth(&reg_access_token)
        .json(&json!({
            "password": new_password
        }))
        .send().await.unwrap();
    assert_eq!(change_pwd_resp.status(), StatusCode::OK);

    // Login with old password should now fail
    let old_pwd_login_resp = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": reg_username,
            "password": reg_password
        }))
        .send().await.unwrap();
    assert_eq!(old_pwd_login_resp.status(), StatusCode::UNAUTHORIZED);

    // Login with new password should succeed
    let new_pwd_login_resp = client.post(format!("{}/auth/login", base_url))
        .json(&json!({
            "username": reg_username,
            "password": new_password
        }))
        .send().await.unwrap();
    assert_eq!(new_pwd_login_resp.status(), StatusCode::OK);
}
