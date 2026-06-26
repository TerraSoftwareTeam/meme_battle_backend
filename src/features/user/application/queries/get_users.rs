use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{MediaAssetResolver, UserProfile, UserRepository},
};

pub struct GetUsersQuery {
    repo: Arc<dyn UserRepository>,
    media_asset_resolver: Arc<dyn MediaAssetResolver>,
}

impl GetUsersQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        media_asset_resolver: Arc<dyn MediaAssetResolver>,
    ) -> Self {
        Self {
            repo,
            media_asset_resolver,
        }
    }

    pub async fn execute(&self) -> Result<Vec<UserProfile>, AppError> {
        let users = self.repo.find_all().await?;
        let mut profiles = Vec::with_capacity(users.len());

        for user in users {
            profiles.push(UserProfile::resolve(user, self.media_asset_resolver.as_ref()).await?);
        }

        Ok(profiles)
    }
}
