use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::{MediaAsset, MediaRepository},
};

pub struct GetUserMediaQuery {
    media_repository: Arc<dyn MediaRepository>,
}

impl GetUserMediaQuery {
    pub fn new(media_repository: Arc<dyn MediaRepository>) -> Self {
        Self { media_repository }
    }

    pub async fn execute(&self, owner_user_id: String) -> Result<Vec<MediaAsset>, AppError> {
        self.media_repository.list_by_owner(&owner_user_id).await
    }
}
