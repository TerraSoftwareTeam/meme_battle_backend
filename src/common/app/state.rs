use axum::extract::FromRef;

use super::config::Config;

pub mod auth;
pub mod media;
pub mod user;

pub use auth::AuthState;
pub use media::MediaState;
pub use user::UserState;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    auth: AuthState,
    media: MediaState,
    user: UserState,
}

impl AppState {
    pub fn new(
        config: Config,
        auth: AuthState,
        media: MediaState,
        user: UserState,
    ) -> Self {
        Self {
            config,
            auth,
            media,
            user,
        }
    }
}

impl FromRef<AppState> for AuthState {
    fn from_ref(state: &AppState) -> Self {
        state.auth.clone()
    }
}

impl FromRef<AppState> for MediaState {
    fn from_ref(state: &AppState) -> Self {
        state.media.clone()
    }
}

impl FromRef<AppState> for UserState {
    fn from_ref(state: &AppState) -> Self {
        state.user.clone()
    }
}
