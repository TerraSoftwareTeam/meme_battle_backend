use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{MediaAssetResolver, UserProfile, UserRepository},
};

pub struct GetUserByIdQuery {
    repo: Arc<dyn UserRepository>,
    media_asset_resolver: Arc<dyn MediaAssetResolver>,
}

impl GetUserByIdQuery {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        media_asset_resolver: Arc<dyn MediaAssetResolver>,
    ) -> Self {
        Self {
            repo,
            media_asset_resolver,
        }
    }

    pub async fn execute(&self, id: String) -> Result<UserProfile, AppError> {
        let user = self
            .repo
            .find_by_id(&id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        UserProfile::resolve(user, self.media_asset_resolver.as_ref()).await
    }
}
