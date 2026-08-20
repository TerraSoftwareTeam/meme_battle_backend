use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::GameStatus,
        ports::GameRepository,
    },
};

pub struct ActiveGameInfo {
    pub game_id: Uuid,
    pub status: GameStatus,
}

pub struct GetActiveGameQuery {
    repo: Arc<dyn GameRepository>,
}

impl GetActiveGameQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<Option<ActiveGameInfo>, AppError> {
        let res = self.repo.find_active_game_for_player(user_id).await?;
        Ok(res.map(|(game_id, status)| ActiveGameInfo { game_id, status }))
    }
}
