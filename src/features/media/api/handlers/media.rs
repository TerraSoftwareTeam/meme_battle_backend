use crate::{
    common::{
        app::state::MediaState,
        http::{current_user::CurrentUser, dto::RestApiResponse, error::AppError},
    },
    features::media::api::{
        dto::{request::UploadMediaRequestDto, response::MediaAssetDto},
        handlers::validation::extract_file,
    },
};
use axum::{
    extract::{Multipart, State},
    response::IntoResponse,
};

#[utoipa::path(
    post,
    path = "/media/upload/image",
    request_body(content = UploadMediaRequestDto, content_type = "multipart/form-data"),
    responses((status = 200, description = "Upload media for images/memes", body = MediaAssetDto)),
    tag = "Media"
)]
pub async fn upload_image_media(
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
    let file = extract_file(&mut multipart, state.max_file_size_bytes).await?;
    let media = state.upload_media.execute(owner_user_id, file).await?;
    Ok(MediaAssetDto::from(media))
}
