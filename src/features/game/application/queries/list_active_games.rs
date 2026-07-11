use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::game::domain::{model::ActiveGame, ports::GameRepository},
};

pub struct ListActiveGamesQuery {
    repo: Arc<dyn GameRepository>,
}

impl ListActiveGamesQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self) -> Result<Vec<ActiveGame>, AppError> {
        self.repo.find_active_lobby_games().await
    }
}
