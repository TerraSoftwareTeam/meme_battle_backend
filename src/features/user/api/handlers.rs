use crate::{
    common::{
        app::state::UserState,
        http::{current_user::CurrentUser, dto::RestApiResponse, error::AppError},
    },
    features::user::{
        api::dto::{
            request::{SearchUserDto, UpdateMeDto, UploadAvatarRequestDto},
            response::UserDto,
        },
        AvatarUploadFile,
    },
};

use axum::{
    extract::{Multipart, State},
    response::IntoResponse,
    Json,
};
use validator::Validate;

const MAX_AVATAR_SIZE_BYTES: usize = 10 * 1024 * 1024;

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
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))?;

    let user = state
        .update_me
        .execute(current_user.user_id, payload.into())
        .await?;

    Ok(RestApiResponse::success(UserDto::from(user)))
}

#[utoipa::path(
    put,
    path = "/user/me/avatar",
    request_body(content = UploadAvatarRequestDto, content_type = "multipart/form-data"),
    responses((status = 200, description = "Update current user avatar", body = UserDto)),
    tag = "Me"
)]
pub async fn update_my_avatar(
    State(state): State<UserState>,
    current_user: CurrentUser,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let file = extract_avatar_file(&mut multipart).await?;
    let user = state
        .update_my_avatar
        .execute(current_user.user_id, file)
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
    path = "/user/list",
    request_body = SearchUserDto,
    responses((status = 200, description = "List users by condition", body = [UserDto])),
    tag = "Users"
)]
pub async fn get_user_list(
    State(state): State<UserState>,
    Json(payload): Json<SearchUserDto>,
) -> Result<impl IntoResponse, AppError> {
    let users = state.get_user_list.execute(payload.into()).await?;
    Ok(RestApiResponse::success(
        users.into_iter().map(UserDto::from).collect::<Vec<_>>(),
    ))
}

#[utoipa::path(
    get,
    path = "/user",
    responses((status = 200, description = "List all users", body = [UserDto])),
    tag = "Users"
)]
pub async fn get_users(State(state): State<UserState>) -> Result<impl IntoResponse, AppError> {
    let users = state.get_users.execute().await?;
    Ok(RestApiResponse::success(
        users.into_iter().map(UserDto::from).collect::<Vec<_>>(),
    ))
}

async fn extract_avatar_file(multipart: &mut Multipart) -> Result<AvatarUploadFile, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::ValidationError(format!("Invalid multipart body: {err}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let filename = field
            .file_name()
            .map(ToString::to_string)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::ValidationError("File name is required".into()))?;
        let content_type = field
            .content_type()
            .map(ToString::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let bytes = field
            .bytes()
            .await
            .map_err(|err| AppError::ValidationError(format!("Invalid multipart file: {err}")))?;

        if bytes.is_empty() {
            return Err(AppError::ValidationError("File cannot be empty".into()));
        }

        if bytes.len() > MAX_AVATAR_SIZE_BYTES {
            return Err(AppError::ValidationError(format!(
                "File cannot exceed {} bytes",
                MAX_AVATAR_SIZE_BYTES
            )));
        }

        return Ok(AvatarUploadFile {
            filename,
            content_type,
            bytes: bytes.to_vec(),
        });
    }

    Err(AppError::ValidationError("Missing file field".into()))
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
    // TODO: enable role check when ready
    // if _current_user.role != Role::Admin {
    //     return Err(AppError::Forbidden);
    // }

    state.promote_to_admin.execute(&id).await?;
    Ok(RestApiResponse::success_with_message(
        "User promoted to admin".to_string(),
        (),
    ))
}
