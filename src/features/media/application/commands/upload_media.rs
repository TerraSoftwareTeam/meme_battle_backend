use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::{CreateMediaAsset, FileStorage, MediaAsset, MediaRepository, UploadFile},
};

pub struct UploadMediaCommand {
    storage: Arc<dyn FileStorage>,
    media_repository: Arc<dyn MediaRepository>,
}

impl UploadMediaCommand {
    pub fn new(storage: Arc<dyn FileStorage>, media_repository: Arc<dyn MediaRepository>) -> Self {
        Self {
            storage,
            media_repository,
        }
    }

    pub async fn execute(
        &self,
        owner_user_id: String,
        file: UploadFile,
    ) -> Result<MediaAsset, AppError> {
        let stored_file = self.storage.upload(file).await?;

        self.media_repository
            .create(CreateMediaAsset {
                owner_user_id,
                stored_file,
            })
            .await
    }
}
