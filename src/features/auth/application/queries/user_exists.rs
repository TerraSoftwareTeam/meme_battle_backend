use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::auth::domain::ports::auth_repository::AuthRepository,
};

pub struct UserExistsQuery {
    repo: Arc<dyn AuthRepository>,
}

impl UserExistsQuery {
    pub fn new(repo: Arc<dyn AuthRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, user_id: &str) -> Result<bool, AppError> {
        let user = self.repo.find_by_id(user_id).await?;
        Ok(user.is_some())
    }
}
