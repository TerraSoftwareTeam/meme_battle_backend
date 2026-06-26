use axum::{
    extract::{Multipart, Path, State},
    response::IntoResponse,
    Json,
};
use validator::Validate;

use crate::{
    common::{
        app::state::MediaState,
        http::{current_user::CurrentUser, dto::RestApiResponse, error::AppError},
    },
    features::media::{
        api::dto::{
            request::{UploadMediaFromUrlDto, UploadMediaRequestDto},
            response::MediaAssetDto,
        },
        UploadFile,
    },
};

const MAX_UPLOAD_SIZE_BYTES: usize = 35 * 1024 * 1024;

#[utoipa::path(
    post,
    path = "/media/upload",
    request_body(content = UploadMediaRequestDto, content_type = "multipart/form-data"),
    responses((status = 200, description = "Upload media file", body = MediaAssetDto)),
    tag = "Media"
)]
pub async fn upload_media(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let response = handle_media_upload(&state, current_user.user_id, multipart).await?;
    Ok(RestApiResponse::success(response))
}

#[utoipa::path(
    get,
    path = "/media",
    responses((status = 200, description = "List current user media assets", body = [MediaAssetDto])),
    tag = "Media"
)]
pub async fn get_user_media(
    State(state): State<MediaState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let media = state.get_user_media.execute(current_user.user_id).await?;

    Ok(RestApiResponse::success(
        media
            .into_iter()
            .map(MediaAssetDto::from)
            .collect::<Vec<_>>(),
    ))
}

#[utoipa::path(
    get,
    path = "/media/{id}",
    responses((status = 200, description = "Get media asset by ID", body = MediaAssetDto)),
    tag = "Media"
)]
pub async fn get_media_by_id(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let media = state
        .get_media_by_id
        .execute(current_user.user_id, id)
        .await?;

    Ok(RestApiResponse::success(MediaAssetDto::from(media)))
}

#[utoipa::path(
    post,
    path = "/media/upload_from_url",
    request_body = UploadMediaFromUrlDto,
    responses((status = 200, description = "Upload media from URL", body = MediaAssetDto)),
    tag = "Media"
)]
pub async fn upload_media_from_url(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    Json(payload): Json<UploadMediaFromUrlDto>,
) -> Result<impl IntoResponse, AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))?;

    let media = state
        .upload_media_from_url
        .execute(current_user.user_id, payload.url)
        .await?;

    Ok(RestApiResponse::success(MediaAssetDto::from(media)))
}

#[utoipa::path(
    delete,
    path = "/media/{id}",
    responses((status = 200, description = "Delete media asset")),
    tag = "Media"
)]
pub async fn delete_media(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    Path(id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    state.delete_media.execute(current_user.user_id, id).await?;

    Ok(RestApiResponse::success_with_message(
        "Media deleted".to_string(),
        (),
    ))
}

#[utoipa::path(
    post,
    path = "/media/upload/comment",
    request_body(content = UploadMediaRequestDto, content_type = "multipart/form-data"),
    responses((status = 200, description = "Upload media for comments", body = MediaAssetDto)),
    tag = "Media"
)]
pub async fn upload_comment_media(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let response = handle_media_upload(&state, current_user.user_id, multipart).await?;
    Ok(RestApiResponse::success(response))
}

#[utoipa::path(
    post,
    path = "/media/upload/entry",
    request_body(content = UploadMediaRequestDto, content_type = "multipart/form-data"),
    responses((status = 200, description = "Upload media for entries", body = MediaAssetDto)),
    tag = "Media"
)]
pub async fn upload_entry_media(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    let response = handle_media_upload(&state, current_user.user_id, multipart).await?;
    Ok(RestApiResponse::success(response))
}

async fn handle_media_upload(
    state: &MediaState,
    owner_user_id: String,
    mut multipart: Multipart,
) -> Result<MediaAssetDto, AppError> {
    let file = extract_file(&mut multipart).await?;
    let media = state.upload_media.execute(owner_user_id, file).await?;
    Ok(MediaAssetDto::from(media))
}

async fn extract_file(multipart: &mut Multipart) -> Result<UploadFile, AppError> {
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

        if bytes.len() > MAX_UPLOAD_SIZE_BYTES {
            return Err(AppError::ValidationError(format!(
                "File cannot exceed {} bytes",
                MAX_UPLOAD_SIZE_BYTES
            )));
        }

        return Ok(UploadFile {
            filename,
            content_type,
            bytes: bytes.to_vec(),
        });
    }

    Err(AppError::ValidationError("Missing file field".into()))
}
