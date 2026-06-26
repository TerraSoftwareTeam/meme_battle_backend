use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{AvatarMediaUploader, AvatarUploadFile, UserProfile, UserRepository},
};

pub struct UpdateMyAvatarCommand {
    repo: Arc<dyn UserRepository>,
    avatar_media_uploader: Arc<dyn AvatarMediaUploader>,
}

impl UpdateMyAvatarCommand {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        avatar_media_uploader: Arc<dyn AvatarMediaUploader>,
    ) -> Self {
        Self {
            repo,
            avatar_media_uploader,
        }
    }

    pub async fn execute(
        &self,
        user_id: String,
        file: AvatarUploadFile,
    ) -> Result<UserProfile, AppError> {
        let avatar = self
            .avatar_media_uploader
            .upload_avatar(user_id.clone(), file)
            .await?;
        let user = self
            .repo
            .update_avatar_media_asset_id(&user_id, avatar.media_asset_id)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        Ok(UserProfile {
            id: user.id,
            username: user.username,
            handle: user.handle,
            avatar_url: Some(avatar.url),
            created_at: user.created_at,
            modified_at: user.modified_at,
        })
    }
}
