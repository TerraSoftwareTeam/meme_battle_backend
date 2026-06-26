use std::sync::Arc;

use crate::{common::http::error::AppError, features::user::UserRepository};

pub struct PromoteToAdminCommand {
    repo: Arc<dyn UserRepository>,
}

impl PromoteToAdminCommand {
    pub fn new(repo: Arc<dyn UserRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, user_id: &str) -> Result<(), AppError> {
        self.repo
            .promote_to_admin(user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;
        Ok(())
    }
}
