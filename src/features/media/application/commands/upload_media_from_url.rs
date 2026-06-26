use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::{CreateMediaAsset, FileStorage, MediaAsset, MediaRepository},
};

pub struct UploadMediaFromUrlCommand {
    storage: Arc<dyn FileStorage>,
    media_repository: Arc<dyn MediaRepository>,
}

impl UploadMediaFromUrlCommand {
    pub fn new(storage: Arc<dyn FileStorage>, media_repository: Arc<dyn MediaRepository>) -> Self {
        Self {
            storage,
            media_repository,
        }
    }

    pub async fn execute(
        &self,
        owner_user_id: String,
        source_url: String,
    ) -> Result<MediaAsset, AppError> {
        let stored_file = self.storage.upload_from_url(&source_url).await?;

        self.media_repository
            .create(CreateMediaAsset {
                owner_user_id,
                stored_file,
            })
            .await
    }
}
