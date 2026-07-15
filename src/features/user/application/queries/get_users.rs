use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{UserProfile, UserRepository},
};

pub struct GetUsersQuery {
    repo: Arc<dyn UserRepository>,
}

impl GetUsersQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            repo,
        }
    }

    pub async fn execute(&self) -> Result<Vec<UserProfile>, AppError> {
        let users = self.repo.find_all().await?;
        let mut profiles = Vec::with_capacity(users.len());

        for user in users {
            profiles.push(UserProfile::resolve(user).await?);
        }

        Ok(profiles)
    }
}
