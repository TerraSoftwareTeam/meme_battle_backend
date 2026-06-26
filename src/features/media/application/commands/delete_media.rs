use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::{FileStorage, MediaRepository},
};

pub struct DeleteMediaCommand {
    storage: Arc<dyn FileStorage>,
    media_repository: Arc<dyn MediaRepository>,
}

impl DeleteMediaCommand {
    pub fn new(storage: Arc<dyn FileStorage>, media_repository: Arc<dyn MediaRepository>) -> Self {
        Self {
            storage,
            media_repository,
        }
    }

    pub async fn execute(&self, owner_user_id: String, media_id: i64) -> Result<(), AppError> {
        let asset = self
            .media_repository
            .find_by_id_for_owner(media_id, &owner_user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Media asset not found".into()))?;

        self.storage.delete(&asset.provider_file_id).await?;
        self.media_repository
            .delete_by_id_for_owner(media_id, &owner_user_id)
            .await?;

        Ok(())
    }
}
