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

    // --- 1. Test /auth/guest without username (gets auto-generated player-{uuid} username) ---
    let resp = client.post(format!("{}/auth/guest", base_url))
        .header("content-type", "application/json")
        .body("{}")
        .send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let auth_body: RestApiResponse<Value> = resp.json().await.unwrap();
    let data = auth_body.0.data.unwrap();
    let access_token = data.get("access_token").unwrap().as_str().unwrap().to_string();
    let refresh_token = data.get("refresh_token").unwrap().as_str().unwrap().to_string();

    // Verify the guest user has a player-{uuid} username in both API response and DB
    let me_resp = client.get(format!("{}/user/me", base_url))
        .bearer_auth(&access_token)
        .send().await.unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);
    let me_body: RestApiResponse<Value> = me_resp.json().await.unwrap();
    let me_data = me_body.0.data.unwrap();
    let user_id = me_data.get("id").unwrap().as_str().unwrap().to_string();
    let username = me_data.get("username").unwrap().as_str().unwrap().to_string();
    assert!(username.starts_with("player-"));

    // Check DB directly — guest users now always have username = "player-{uuid}"
    let db_username: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1::uuid")
        .bind(&user_id)
        .fetch_one(&pool).await.unwrap();
    assert!(
        db_username.as_deref().map(|u| u.starts_with("player-")).unwrap_or(false),
        "Guest user should have a player-{{uuid}} username in DB, got: {:?}", db_username
    );


    // --- 2. Test /auth/guest with username ---
    let guest_name = format!("guest-{}", Uuid::new_v4());
    let resp_named = client.post(format!("{}/auth/guest", base_url))
        .json(&json!({ "username": guest_name }))
        .send().await.unwrap();
    assert_eq!(resp_named.status(), StatusCode::OK);
    let auth_body_named: RestApiResponse<Value> = resp_named.json().await.unwrap();
    let access_token_named = auth_body_named.0.data.unwrap().get("access_token").unwrap().as_str().unwrap().to_string();

    let me_resp_named = client.get(format!("{}/user/me", base_url))
        .bearer_auth(&access_token_named)
        .send().await.unwrap();
    assert_eq!(me_resp_named.status(), StatusCode::OK);
    let me_body_named: RestApiResponse<Value> = me_resp_named.json().await.unwrap();
    let data_named = me_body_named.0.data.unwrap();
    let user_id_named = data_named.get("id").unwrap().as_str().unwrap().to_string();
    let username_named = data_named.get("username").unwrap().as_str().unwrap().to_string();
    assert_eq!(username_named, guest_name);

    // Check DB directly
    let db_username_named: Option<String> = sqlx::query_scalar("SELECT username FROM users WHERE id = $1::uuid")
        .bind(&user_id_named)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(db_username_named, Some(guest_name.clone()));

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

    // --- 6. Test /auth/change-password ---
    let new_password = "newsecretpassword";
    let change_pwd_resp = client.post(format!("{}/auth/change-password", base_url))
        .bearer_auth(&reg_access_token)
        .json(&json!({
            "new_password": new_password
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
