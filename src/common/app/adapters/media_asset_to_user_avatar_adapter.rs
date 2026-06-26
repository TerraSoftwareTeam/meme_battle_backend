use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::{
        media::{GetMediaAssetUrlQuery, UploadFile, UploadMediaCommand},
        user::{AvatarMediaUploader, AvatarUploadFile, MediaAssetResolver, UploadedAvatar},
    },
};

#[derive(Clone)]
pub struct MediaAssetToUserAvatarAdapter {
    get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
    upload_media: Arc<UploadMediaCommand>,
}

impl MediaAssetToUserAvatarAdapter {
    pub fn new(
        get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
        upload_media: Arc<UploadMediaCommand>,
    ) -> Self {
        Self {
            get_media_asset_url,
            upload_media,
        }
    }
}

#[async_trait]
impl MediaAssetResolver for MediaAssetToUserAvatarAdapter {
    async fn resolve_url(&self, media_asset_id: i64) -> Result<Option<String>, AppError> {
        self.get_media_asset_url.execute(media_asset_id).await
    }
}

#[async_trait]
impl AvatarMediaUploader for MediaAssetToUserAvatarAdapter {
    async fn upload_avatar(
        &self,
        owner_user_id: String,
        file: AvatarUploadFile,
    ) -> Result<UploadedAvatar, AppError> {
        let media = self
            .upload_media
            .execute(
                owner_user_id,
                UploadFile {
                    filename: file.filename,
                    content_type: file.content_type,
                    bytes: file.bytes,
                },
            )
            .await?;

        Ok(UploadedAvatar {
            media_asset_id: media.id,
            url: media.url,
        })
    }
}
