use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{
        application::ports::UserAuthService, SearchUser, UserProfile, UserRepository,
    },
};

pub struct GetUserListQuery {
    repo: Arc<dyn UserRepository>,
    user_auth_service: Arc<dyn UserAuthService>,
}

impl GetUserListQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        user_auth_service: Arc<dyn UserAuthService>,
    ) -> Self {
        Self {
            repo,
            user_auth_service,
        }
    }

    pub async fn execute(&self, search: SearchUser) -> Result<Vec<UserProfile>, AppError> {
        let users = self.repo.find_list(search).await?;
        let mut profiles = Vec::with_capacity(users.len());

        for user in users {
            let is_guest = self.user_auth_service.is_guest(&user.id).await.unwrap_or(true);
            profiles.push(UserProfile::new(user, is_guest));
        }

        Ok(profiles)
    }
}
