use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_token_generator::GameTokenGenerator,
        domain::{model::ActiveGame, ports::GameRepository},
    },
};

pub struct ListActiveGamesResult {
    pub games: Vec<ActiveGame>,
    pub connection_token: String,
    pub lobbies_subscription_token: String,
}

pub struct ListActiveGamesQuery {
    repo: Arc<dyn GameRepository>,
    token_generator: Arc<dyn GameTokenGenerator>,
}

impl ListActiveGamesQuery {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        token_generator: Arc<dyn GameTokenGenerator>,
    ) -> Self {
        Self {
            repo,
            token_generator,
        }
    }

    pub async fn execute(&self, user_id: Uuid) -> Result<ListActiveGamesResult, AppError> {
        let games = self.repo.find_active_lobby_games().await?;
        let connection_token = self.token_generator.generate_connection_token(user_id)?;
        let lobbies_subscription_token = self.token_generator.generate_lobbies_subscription_token(user_id)?;

        Ok(ListActiveGamesResult {
            games,
            connection_token,
            lobbies_subscription_token,
        })
    }
}
