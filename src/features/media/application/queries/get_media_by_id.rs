use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::{MediaAsset, MediaRepository},
};

pub struct GetMediaByIdQuery {
    media_repository: Arc<dyn MediaRepository>,
}

impl GetMediaByIdQuery {
    pub fn new(media_repository: Arc<dyn MediaRepository>) -> Self {
        Self { media_repository }
    }

    pub async fn execute(
        &self,
        owner_user_id: String,
        media_id: i64,
    ) -> Result<MediaAsset, AppError> {
        self.media_repository
            .find_by_id_for_owner(media_id, &owner_user_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Media asset not found".into()))
    }
}
