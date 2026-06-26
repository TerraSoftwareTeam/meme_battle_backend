use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{MediaAssetResolver, SearchUser, UserProfile, UserRepository},
};

pub struct GetUserListQuery {
    repo: Arc<dyn UserRepository>,
    media_asset_resolver: Arc<dyn MediaAssetResolver>,
}

impl GetUserListQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        media_asset_resolver: Arc<dyn MediaAssetResolver>,
    ) -> Self {
        Self {
            repo,
            media_asset_resolver,
        }
    }

    pub async fn execute(&self, search: SearchUser) -> Result<Vec<UserProfile>, AppError> {
        let users = self.repo.find_list(search).await?;
        let mut profiles = Vec::with_capacity(users.len());

        for user in users {
            profiles.push(UserProfile::resolve(user, self.media_asset_resolver.as_ref()).await?);
        }

        Ok(profiles)
    }
}
