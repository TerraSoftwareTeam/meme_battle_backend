use crate::{
    common::{
        app::state::AuthState, http::dto::RestApiResponse, http::error::AppError,
        security::jwt::AuthBody,
    },
    features::auth::api::dto::request::{AuthUserDto, RefreshSessionDto, RegisterAuthUserDto},
};
use axum::extract::State;
use axum::{response::IntoResponse, Json};
use validator::Validate;

/// this function creates a router for creating user authentication registration
/// it will create a new user in the database
#[utoipa::path(
    post,
    path = "/auth/register",
    request_body = RegisterAuthUserDto,
    responses((status = 200, description = "Create user authentication", body = RegisterAuthUserDto)),
    tag = "UserAuth"
)]
pub async fn create_user_auth(
    State(state): State<AuthState>,
    Json(payload): Json<RegisterAuthUserDto>,
) -> Result<impl IntoResponse, AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))?;

    state.register_user.execute(payload.into()).await?;
    Ok(RestApiResponse::success(()))
}

/// this function creates a router for login user
/// it will return a JWT token if the user is authenticated
#[utoipa::path(
    post,
    path = "/auth/login",
    request_body = AuthUserDto,
    responses((status = 200, description = "Login user", body = AuthBody)),
    tag = "UserAuth"
)]
pub async fn login_user(
    State(state): State<AuthState>,
    Json(payload): Json<AuthUserDto>,
) -> Result<impl IntoResponse, AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))?;

    let auth_body = state.login_user.execute(payload.into()).await?;
    Ok(RestApiResponse::success(auth_body))
}

#[utoipa::path(
    post,
    path = "/auth/guest",
    responses((status = 200, description = "Authenticate as guest", body = AuthBody)),
    tag = "UserAuth"
)]
pub async fn auth_as_guest(State(state): State<AuthState>) -> Result<impl IntoResponse, AppError> {
    let auth_body = state.auth_as_guest.execute().await?;
    Ok(RestApiResponse::success(auth_body))
}

#[utoipa::path(
    post,
    path = "/auth/refresh",
    request_body = RefreshSessionDto,
    responses((status = 200, description = "Refresh session", body = AuthBody)),
    tag = "UserAuth"
)]
pub async fn refresh_session(
    State(state): State<AuthState>,
    Json(payload): Json<RefreshSessionDto>,
) -> Result<impl IntoResponse, AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))?;

    let auth_body = state.refresh_session.execute(payload.into()).await?;
    Ok(RestApiResponse::success(auth_body))
}
