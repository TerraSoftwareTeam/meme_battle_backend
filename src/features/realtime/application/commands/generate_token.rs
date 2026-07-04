use uuid::Uuid;
use crate::common::{
    http::error::AppError,
    security::jwt::{make_centrifugo_connect_token, make_centrifugo_subscribe_token},
};

pub struct GenerateTokenCommand;

impl GenerateTokenCommand {
    pub fn new() -> Self {
        Self
    }

    pub fn generate_connection_token(&self, user_id: Uuid) -> Result<String, AppError> {
        make_centrifugo_connect_token(&user_id.to_string())
    }

    pub fn generate_subscription_token(&self, user_id: Uuid, channel: &str) -> Result<String, AppError> {
        make_centrifugo_subscribe_token(&user_id.to_string(), channel)
    }
}

impl Default for GenerateTokenCommand {
    fn default() -> Self {
        Self::new()
    }
}
