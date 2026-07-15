use super::handlers::*;
use crate::{
    common::app::state::AppState,
    features::user::api::dto::{request::UpdateMeDto, response::UserDto},
};

use axum::{
    routing::{get, post},
    Router,
};

use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    OpenApi,
};

#[derive(OpenApi)]
#[openapi(
    paths(
        get_me,
        update_me,
        get_user_by_id,
    ),
    components(schemas(UserDto, UpdateMeDto)),
    tags(
        (name = "Users", description = "Users endpoints"),
        (name = "Me", description = "Current user profile endpoints")
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&UserApiDoc)
)]
pub struct UserApiDoc;

impl utoipa::Modify for UserApiDoc {
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

#[derive(OpenApi)]
#[openapi(
    paths(
        promote_to_admin,
    ),
    tags(
        (name = "Admin", description = "Admin-only endpoints")
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&UserAdminApiDoc)
)]
pub struct UserAdminApiDoc;

impl utoipa::Modify for UserAdminApiDoc {
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

pub fn user_routes() -> Router<AppState> {
    Router::new()
        .route("/me", get(get_me).patch(update_me))
        .route("/{id}", get(get_user_by_id))
        .route("/{id}/promote-admin", post(promote_to_admin))
}
