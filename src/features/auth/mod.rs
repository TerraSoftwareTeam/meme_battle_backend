mod api;
mod application;
mod domain;
mod infra;

// Re-export commonly used items for convenience
pub use api::routes::{user_auth_routes, UserAuthApiDoc};
pub use application::commands::{
    auth_as_guest::AuthAsGuestCommand, login_user::LoginUserCommand,
    refresh_session::RefreshSessionCommand, register_user::RegisterUserCommand,
};
pub use application::queries::user_exists::UserExistsQuery;
pub use domain::{
    model::{LoginUser, RefreshSession, RegisterUser},
    ports::auth_repository::AuthRepository,
};
pub use infra::adapters::auth_repository_impl::AuthRepositoryImpl;
