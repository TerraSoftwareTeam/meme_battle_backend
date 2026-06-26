use axum::{
    routing::{get, post},
    Router,
};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    OpenApi,
};

use crate::{
    common::app::state::AppState,
    features::media::{
        api::{
            dto::{
                request::{UploadMediaFromUrlDto, UploadMediaRequestDto},
                response::MediaAssetDto,
            },
            handlers::{
                __path_delete_media, __path_get_media_by_id, __path_get_user_media,
                __path_upload_comment_media, __path_upload_entry_media, __path_upload_media,
                __path_upload_media_from_url, delete_media, get_media_by_id, get_user_media,
                upload_comment_media, upload_entry_media, upload_media, upload_media_from_url,
            },
        },
        MediaProvider,
    },
};

#[derive(OpenApi)]
#[openapi(
    paths(
        get_user_media,
        get_media_by_id,
        upload_media,
        upload_media_from_url,
        upload_comment_media,
        upload_entry_media,
        delete_media,
    ),
    components(schemas(
        UploadMediaFromUrlDto,
        UploadMediaRequestDto,
        MediaAssetDto,
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
        .route("/", get(get_user_media))
        .route("/{id}", get(get_media_by_id).delete(delete_media))
        .route("/upload", post(upload_media))
        .route("/upload_from_url", post(upload_media_from_url))
        .route("/upload/comment", post(upload_comment_media))
        .route("/upload/entry", post(upload_entry_media))
}
