use chrono::{DateTime, Utc};

use crate::{
    common::http::error::AppError,
    features::user::{MediaAssetResolver, User},
};

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub handle: String,
    pub avatar_url: Option<String>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
}

impl UserProfile {
    pub async fn resolve(
        user: User,
        media_asset_resolver: &dyn MediaAssetResolver,
    ) -> Result<Self, AppError> {
        let avatar_url = match user.avatar_media_asset_id {
            Some(media_asset_id) => media_asset_resolver.resolve_url(media_asset_id).await?,
            None => None,
        };

        Ok(Self {
            id: user.id,
            username: user.username,
            handle: user.handle,
            avatar_url,
            created_at: user.created_at,
            modified_at: user.modified_at,
        })
    }
}
