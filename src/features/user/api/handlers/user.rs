use crate::{
    common::{
        app::state::UserState,
        http::{current_user::CurrentUser, dto::RestApiResponse, error::AppError},
    },
    features::user::api::{
        dto::{
            request::UpdateMeDto,
            response::UserDto,
        },
        handlers::validation::validate_update_me,
    },
};
use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};

#[utoipa::path(
    get,
    path = "/user/me",
    responses((status = 200, description = "Get current user profile", body = UserDto)),
    tag = "Me"
)]
pub async fn get_me(
    State(state): State<UserState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let user = state.get_me.execute(current_user.user_id).await?;
    Ok(RestApiResponse::success(UserDto::from(user)))
}

#[utoipa::path(
    patch,
    path = "/user/me",
    request_body = UpdateMeDto,
    responses((status = 200, description = "Update current user profile", body = UserDto)),
    tag = "Me"
)]
pub async fn update_me(
    State(state): State<UserState>,
    current_user: CurrentUser,
    Json(payload): Json<UpdateMeDto>,
) -> Result<impl IntoResponse, AppError> {
    validate_update_me(&payload)?;

    let user = state
        .update_me
        .execute(current_user.user_id, payload.into())
        .await?;

    Ok(RestApiResponse::success(UserDto::from(user)))
}

#[utoipa::path(
    get,
    path = "/user/{id}",
    responses((status = 200, description = "Get user by ID", body = UserDto)),
    tag = "Users"
)]
pub async fn get_user_by_id(
    State(state): State<UserState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user = state.get_user_by_id.execute(id).await?;
    Ok(RestApiResponse::success(UserDto::from(user)))
}


#[utoipa::path(
    post,
    path = "/user/{id}/promote-admin",
    params(
        ("id" = String, Path, description = "User ID to promote")
    ),
    responses((status = 200, description = "User promoted to admin")),
    tag = "Admin"
)]
pub async fn promote_to_admin(
    State(state): State<UserState>,
    _current_user: CurrentUser,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<impl IntoResponse, AppError> {
    state.promote_to_admin.execute(&id).await?;
    Ok(RestApiResponse::success_with_message(
        "User promoted to admin".to_string(),
        (),
    ))
}
