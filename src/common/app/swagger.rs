use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;

use crate::features::{
    auth::UserAuthApiDoc,
    media::MediaApiDoc,
    user::{UserAdminApiDoc, UserApiDoc},
    game::GameApiDoc,
};

pub fn create_swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/docs")
        .url(
            "/api-docs/user_auth/openapi.json",
            UserAuthApiDoc::openapi(),
        )
        .url("/api-docs/user/openapi.json", UserApiDoc::openapi())
        .url("/api-docs/media/openapi.json", MediaApiDoc::openapi())
        .url("/api-docs/game/openapi.json", GameApiDoc::openapi())
}

pub fn create_admin_swagger_ui() -> SwaggerUi {
    SwaggerUi::new("/admin/docs")
        .url(
            "/api-docs/user_admin/openapi.json",
            UserAdminApiDoc::openapi(),
        )
}
