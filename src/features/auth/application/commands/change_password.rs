use std::sync::Arc;

use crate::{
    common::{http::error::AppError, security::hash_util},
    features::auth::AuthRepository,
};

pub struct ChangePasswordCommand {
    repo: Arc<dyn AuthRepository>,
}

impl ChangePasswordCommand {
    pub fn new(repo: Arc<dyn AuthRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, user_id: String, new_password: String) -> Result<(), AppError> {
        if new_password.trim().is_empty() {
            return Err(AppError::ValidationError("New password cannot be empty".into()));
        }

        let password_hash = hash_util::hash_password(&new_password)
            .map_err(|_| AppError::InternalError)?;

        self.repo.update_password_hash(&user_id, password_hash).await?;

        Ok(())
    }
}
