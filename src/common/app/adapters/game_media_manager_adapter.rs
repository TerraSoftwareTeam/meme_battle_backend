use std::sync::Arc;
use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::{
        media::{GetMediaAssetUrlQuery, MediaRepository},
        game::GameMediaManager,
    },
};

#[derive(Clone)]
pub struct GameMediaManagerAdapter {
    get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
    media_repository: Arc<dyn MediaRepository>,
}

impl GameMediaManagerAdapter {
    pub fn new(
        get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
        media_repository: Arc<dyn MediaRepository>,
    ) -> Self {
        Self {
            get_media_asset_url,
            media_repository,
        }
    }
}

#[async_trait]
impl GameMediaManager for GameMediaManagerAdapter {
    async fn resolve_url(&self, media_id: i64) -> Result<Option<String>, AppError> {
        self.get_media_asset_url.execute(media_id).await
    }

    async fn validate_media_exists(&self, media_ids: &[i64]) -> Result<(), AppError> {
        self.media_repository.validate_exists(media_ids).await
    }
}
