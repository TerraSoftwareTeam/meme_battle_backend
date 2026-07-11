use std::sync::Arc;
use uuid::Uuid;
use crate::{
    common::http::error::AppError,
    features::{
        game::GameTokenGenerator,
        realtime::GenerateTokenCommand,
    },
};

pub struct GameTokenGeneratorAdapter {
    token_usecase: Arc<GenerateTokenCommand>,
}

impl GameTokenGeneratorAdapter {
    pub fn new(token_usecase: Arc<GenerateTokenCommand>) -> Self {
        Self { token_usecase }
    }
}

impl GameTokenGenerator for GameTokenGeneratorAdapter {
    fn generate_connection_token(&self, user_id: Uuid) -> Result<String, AppError> {
        self.token_usecase.generate_connection_token(user_id)
    }

    fn generate_subscription_token(&self, user_id: Uuid, game_id: Uuid) -> Result<String, AppError> {
        let channel = format!("game:{}", game_id);
        self.token_usecase.generate_subscription_token(user_id, &channel)
    }

    fn generate_personal_subscription_token(&self, user_id: Uuid) -> Result<String, AppError> {
        let channel = format!("personal:#{}", user_id);
        self.token_usecase.generate_subscription_token(user_id, &channel)
    }

    fn generate_lobbies_subscription_token(&self, user_id: Uuid) -> Result<String, AppError> {
        self.token_usecase.generate_subscription_token(user_id, "lobbies")
    }
}
