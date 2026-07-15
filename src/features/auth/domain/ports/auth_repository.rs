use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::{
    common::http::error::AppError,
    features::auth::domain::model::{RefreshTokenRecord, UserAuth},
};

#[async_trait]
pub trait AuthRepository: Send + Sync {
    async fn create_user_with_auth(
        &self,
        username: Option<String>,
        password_hash: Option<String>,
    ) -> Result<String, AppError>;

    async fn find_by_username(&self, username: &str) -> Result<Option<UserAuth>, AppError>;

    async fn find_by_id(&self, user_id: &str) -> Result<Option<UserAuth>, AppError>;

    async fn save_refresh_token(
        &self,
        user_id: String,
        token_hash: String,
        family_id: uuid::Uuid,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError>;

    async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRecord>, AppError>;

    async fn mark_token_as_used(&self, token_hash: &str) -> Result<(), AppError>;

    async fn revoke_token_family(&self, family_id: uuid::Uuid) -> Result<(), AppError>;

    async fn update_password_hash(
        &self,
        user_id: &str,
        password_hash: String,
    ) -> Result<(), AppError>;
}
