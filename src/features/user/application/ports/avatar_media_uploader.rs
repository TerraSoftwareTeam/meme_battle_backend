use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::user::{AvatarUploadFile, UploadedAvatar},
};

#[async_trait]
pub trait AvatarMediaUploader: Send + Sync {
    async fn upload_avatar(
        &self,
        owner_user_id: String,
        file: AvatarUploadFile,
    ) -> Result<UploadedAvatar, AppError>;
}
