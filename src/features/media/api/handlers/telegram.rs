use axum::{extract::State, Json};

use crate::{
    common::{
        app::state::MediaState,
        http::{current_user::CurrentUser, dto::RestApiResponse, error::AppError},
    },
    features::media::api::dto::{
        request::ImportTelegramStickersRequestDto, response::ImportTelegramStickersResponseDto,
    },
};

#[utoipa::path(
    post,
    path = "/media/import/telegram",
    request_body = ImportTelegramStickersRequestDto,
    responses(
        (status = 200, description = "Import stickers from Telegram sticker pack", body = ImportTelegramStickersResponseDto),
        (status = 400, description = "Validation error (invalid URL or unsupported format)"),
        (status = 404, description = "Sticker set not found")
    ),
    tag = "Media"
)]
pub async fn import_telegram_stickers(
    State(state): State<MediaState>,
    current_user: CurrentUser,
    Json(payload): Json<ImportTelegramStickersRequestDto>,
) -> Result<RestApiResponse<ImportTelegramStickersResponseDto>, AppError> {
    let result = state
        .import_telegram_stickers
        .execute(current_user.user_id, payload.url)
        .await?;

    Ok(RestApiResponse::success(result.into()))
}
