use axum::{routing::post, Router};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    OpenApi,
};

use crate::{
    common::app::state::AppState,
    features::media::{
        api::{
            dto::{
                request::{ImportTelegramStickersRequestDto, UploadMediaRequestDto},
                response::{ImportTelegramStickersResponseDto, ImportedStickerDto, MediaAssetDto},
            },
            handlers::{
                __path_import_telegram_stickers, __path_upload_image_media,
                import_telegram_stickers, upload_image_media,
            },
        },
        MediaProvider,
    },
};

#[derive(OpenApi)]
#[openapi(
    paths(
        upload_image_media,
        import_telegram_stickers,
    ),
    components(schemas(
        UploadMediaRequestDto,
        MediaAssetDto,
        ImportTelegramStickersRequestDto,
        ImportTelegramStickersResponseDto,
        ImportedStickerDto,
        MediaProvider,
    )),
    tags(
        (name = "Media", description = "Media upload and storage endpoints")
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&MediaApiDoc)
)]
pub struct MediaApiDoc;

impl utoipa::Modify for MediaApiDoc {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some("Input your `<your-jwt>`"))
                    .build(),
            ),
        )
    }
}

pub fn media_routes() -> Router<AppState> {
    Router::new()
        .route("/upload/image", post(upload_image_media))
        .route("/import/telegram", post(import_telegram_stickers))
}
