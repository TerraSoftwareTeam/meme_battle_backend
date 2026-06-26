use std::sync::Arc;

use crate::{common::http::error::AppError, features::media::MediaRepository};

pub struct GetMediaAssetUrlQuery {
    media_repository: Arc<dyn MediaRepository>,
}

impl GetMediaAssetUrlQuery {
    pub fn new(media_repository: Arc<dyn MediaRepository>) -> Self {
        Self { media_repository }
    }

    pub async fn execute(&self, media_asset_id: i64) -> Result<Option<String>, AppError> {
        Ok(self
            .media_repository
            .find_by_id(media_asset_id)
            .await?
            .map(|asset| asset.url))
    }
}
