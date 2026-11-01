use async_trait::async_trait;
use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::{
        game::GameMediaManager,
        media::{GetMediaAssetUrlQuery, MediaRepository},
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

    async fn validate_media_ownership(
        &self,
        media_ids: &[i64],
        author_id: uuid::Uuid,
    ) -> Result<(), AppError> {
        self.media_repository
            .validate_ownership(media_ids, &author_id.to_string())
            .await
    }

    async fn enqueue_purge_for_media_ids(&self, media_ids: &[i64]) -> Result<(), AppError> {
        self.media_repository
            .enqueue_purge_tasks_for_media_ids(media_ids)
            .await
    }
}
