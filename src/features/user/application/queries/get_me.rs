use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{application::ports::UserAuthService, UserProfile, UserRepository},
};

pub struct GetMeQuery {
    repo: Arc<dyn UserRepository>,
    user_auth_service: Arc<dyn UserAuthService>,
}

impl GetMeQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        user_auth_service: Arc<dyn UserAuthService>,
    ) -> Self {
        Self {
            repo,
            user_auth_service,
        }
    }

    pub async fn execute(&self, user_id: String) -> Result<UserProfile, AppError> {
        let user = self
            .repo
            .find_by_id(&user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        let is_guest = self.user_auth_service.is_guest(&user_id).await?;

        Ok(UserProfile::new(user, is_guest))
    }
}
