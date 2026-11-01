use axum::extract::FromRef;
use sqlx::PgPool;

use super::config::Config;

pub mod auth;
pub mod media;
pub mod realtime;
pub mod user;

pub use crate::features::game::GameState;
pub use auth::AuthState;
pub use media::MediaState;
pub use realtime::RealtimeState;
pub use user::UserState;

#[derive(Clone)]
pub struct AppState {
    pub config: Config,
    pub pool: PgPool,
    pub auth: AuthState,
    pub media: MediaState,
    pub user: UserState,
    pub game: GameState,
    pub realtime: RealtimeState,
}

impl AppState {
    pub fn new(
        config: Config,
        pool: PgPool,
        auth: AuthState,
        media: MediaState,
        user: UserState,
        game: GameState,
        realtime: RealtimeState,
    ) -> Self {
        Self {
            config,
            pool,
            auth,
            media,
            user,
            game,
            realtime,
        }
    }
}

impl FromRef<AppState> for PgPool {
    fn from_ref(state: &AppState) -> Self {
        state.pool.clone()
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

impl FromRef<AppState> for RealtimeState {
    fn from_ref(state: &AppState) -> Self {
        state.realtime.clone()
    }
}
