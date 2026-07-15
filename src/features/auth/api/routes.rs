use crate::common::app::state::AppState;
use crate::features::auth::api::dto::request::{
    AuthUserDto, RefreshSessionDto, RegisterAuthUserDto, GuestAuthDto, ChangePasswordDto,
};
use axum::{routing::post, Router};

use super::handlers;

use utoipa::OpenApi;

/// Import the necessary modules for OpenAPI documentation generation
#[derive(OpenApi)]
#[openapi(
    paths(
        super::handlers::login_user,
        super::handlers::create_user_auth,
        super::handlers::auth_as_guest,
        super::handlers::refresh_session,
        super::handlers::change_password,
    ),
    components(schemas(
        AuthUserDto,
        RegisterAuthUserDto,
        GuestAuthDto,
        RefreshSessionDto,
        ChangePasswordDto,
        crate::common::security::jwt::AuthBody,
    )),
    tags(
        (name = "UserAuth", description = "User authentication endpoints")
    )
)]
/// This struct is used to generate OpenAPI documentation for the user authentication routes.
pub struct UserAuthApiDoc;

/// This function creates a router for the user authentication routes.
/// It defines the routes and their corresponding handlers.
pub fn user_auth_routes(state: AppState) -> Router<AppState> {
    Router::new()
        .route("/login", post(handlers::login_user))
        .route("/register", post(handlers::create_user_auth))
        .route("/guest", post(handlers::auth_as_guest))
        .route("/refresh", post(handlers::refresh_session))
        .route(
            "/change-password",
            post(handlers::change_password)
                .route_layer(axum::middleware::from_fn_with_state(state, crate::common::security::jwt::jwt_auth))
        )
}
