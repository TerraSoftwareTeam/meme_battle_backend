use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{SearchUser, UserProfile, UserRepository},
};

pub struct GetUserListQuery {
    repo: Arc<dyn UserRepository>,
}

impl GetUserListQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            repo,
        }
    }

    pub async fn execute(&self, search: SearchUser) -> Result<Vec<UserProfile>, AppError> {
        let users = self.repo.find_list(search).await?;
        let mut profiles = Vec::with_capacity(users.len());

        for user in users {
            profiles.push(UserProfile::resolve(user).await?);
        }

        Ok(profiles)
    }
}
