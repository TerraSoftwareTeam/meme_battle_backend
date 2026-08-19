use async_trait::async_trait;

use crate::common::http::error::AppError;

#[async_trait]
pub trait UserAuthService: Send + Sync {
    async fn change_password(&self, user_id: &str, new_password: &str) -> Result<(), AppError>;
    async fn is_guest(&self, user_id: &str) -> Result<bool, AppError>;
}
