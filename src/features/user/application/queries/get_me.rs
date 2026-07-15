use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{UserProfile, UserRepository},
};

pub struct GetMeQuery {
    repo: Arc<dyn UserRepository>,
}

impl GetMeQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            repo,
        }
    }

    pub async fn execute(&self, user_id: String) -> Result<UserProfile, AppError> {
        let user = self
            .repo
            .find_by_id(&user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        UserProfile::resolve(user).await
    }
}
