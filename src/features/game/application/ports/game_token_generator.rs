use uuid::Uuid;
use crate::common::http::error::AppError;

pub trait GameTokenGenerator: Send + Sync {
    fn generate_connection_token(&self, user_id: Uuid) -> Result<String, AppError>;
    fn generate_subscription_token(&self, user_id: Uuid, game_id: Uuid) -> Result<String, AppError>;
    fn generate_personal_subscription_token(&self, user_id: Uuid) -> Result<String, AppError>;
}
