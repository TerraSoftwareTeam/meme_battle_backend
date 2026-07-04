use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_token_generator::GameTokenGenerator,
        domain::ports::GameRepository,
    },
};

#[derive(serde::Serialize, Clone, Debug, utoipa::ToSchema)]
pub struct WsTokenResult {
    pub connection_token: String,
    pub game_subscription_token: String,
    pub personal_subscription_token: String,
}

pub struct GetWsTokenQuery {
    repo: Arc<dyn GameRepository>,
    token_generator: Arc<dyn GameTokenGenerator>,
}

impl GetWsTokenQuery {
    pub fn new(repo: Arc<dyn GameRepository>, token_generator: Arc<dyn GameTokenGenerator>) -> Self {
        Self { repo, token_generator }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid) -> Result<WsTokenResult, AppError> {
        // 1. Verify game exists
        let _game = self.repo
            .find_game(game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        // 2. Verify user is joined
        let players = self.repo.get_players(game_id).await?;
        let is_joined = players.iter().any(|p| p.user_id == user_id);
        if !is_joined {
            return Err(AppError::Forbidden("You are not a participant in this game".to_string()));
        }

        // 3. Generate tokens
        let connection_token = self.token_generator.generate_connection_token(user_id)?;
        let game_subscription_token = self.token_generator.generate_subscription_token(user_id, game_id)?;
        let personal_subscription_token = self.token_generator.generate_personal_subscription_token(user_id)?;

        Ok(WsTokenResult {
            connection_token,
            game_subscription_token,
            personal_subscription_token,
        })
    }
}
