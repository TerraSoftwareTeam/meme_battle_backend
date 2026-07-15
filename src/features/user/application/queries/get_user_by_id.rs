use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{UserProfile, UserRepository},
};

pub struct GetUserByIdQuery {
    repo: Arc<dyn UserRepository>,
}

impl GetUserByIdQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            repo,
        }
    }

    pub async fn execute(&self, id: String) -> Result<UserProfile, AppError> {
        let user = self
            .repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        UserProfile::resolve(user).await
    }
}
