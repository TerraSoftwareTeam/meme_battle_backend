use async_trait::async_trait;
use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::{
        auth::{AuthRepository, ChangePasswordCommand},
        user::UserAuthService,
    },
};

pub struct UserAuthServiceAdapter {
    change_password_cmd: Arc<ChangePasswordCommand>,
    auth_repository: Arc<dyn AuthRepository>,
}

impl UserAuthServiceAdapter {
    pub fn new(
        change_password_cmd: Arc<ChangePasswordCommand>,
        auth_repository: Arc<dyn AuthRepository>,
    ) -> Self {
        Self {
            change_password_cmd,
            auth_repository,
        }
    }
}

#[async_trait]
impl UserAuthService for UserAuthServiceAdapter {
    async fn change_password(&self, user_id: &str, new_password: &str) -> Result<(), AppError> {
        self.change_password_cmd
            .execute(user_id.to_string(), new_password.to_string())
            .await
    }

    async fn is_guest(&self, user_id: &str) -> Result<bool, AppError> {
        let auth = self.auth_repository.find_by_id(user_id).await?;
        Ok(auth.map(|a| a.password_hash.is_none()).unwrap_or(true))
    }
}
