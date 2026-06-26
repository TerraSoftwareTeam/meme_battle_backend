use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{MediaAssetResolver, UpdateUserProfile, UserProfile, UserRepository},
};

pub struct UpdateMeCommand {
    repo: Arc<dyn UserRepository>,
    media_asset_resolver: Arc<dyn MediaAssetResolver>,
}

impl UpdateMeCommand {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        media_asset_resolver: Arc<dyn MediaAssetResolver>,
    ) -> Self {
        Self {
            repo,
            media_asset_resolver,
        }
    }

    pub async fn execute(
        &self,
        user_id: String,
        update: UpdateUserProfile,
    ) -> Result<UserProfile, AppError> {
        let update = normalize_update(update)?;
        let user = self
            .repo
            .update_profile(&user_id, update)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        UserProfile::resolve(user, self.media_asset_resolver.as_ref()).await
    }
}

fn normalize_update(update: UpdateUserProfile) -> Result<UpdateUserProfile, AppError> {
    let username = normalize_optional_field(update.username);
    let handle = normalize_optional_field(update.handle);

    if username.is_none() && handle.is_none() {
        return Err(AppError::ValidationError(
            "At least one profile field must be provided".into(),
        ));
    }

    Ok(UpdateUserProfile { username, handle })
}

fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
