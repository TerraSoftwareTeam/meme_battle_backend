use axum::extract::FromRef;

use super::config::Config;

pub mod auth;
pub mod media;
pub mod user;

pub use auth::AuthState;
pub use media::MediaState;
pub use user::UserState;
pub use crate::features::game::GameState;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    auth: AuthState,
    media: MediaState,
    user: UserState,
    game: GameState,
}

impl AppState {
    pub fn new(
        config: Config,
        auth: AuthState,
        media: MediaState,
        user: UserState,
        game: GameState,
    ) -> Self {
        Self {
            config,
            auth,
            media,
            user,
            game,
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

impl FromRef<AppState> for GameState {
    fn from_ref(state: &AppState) -> Self {
        state.game.clone()
    }
}
