use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::media::{MediaAsset, MediaProvider, StoredFile},
};

#[derive(Debug, Clone)]
pub struct CreateMediaAsset {
    pub owner_user_id: String,
    pub stored_file: StoredFile,
}

#[async_trait]
pub trait MediaRepository: Send + Sync {
    async fn create(&self, asset: CreateMediaAsset) -> Result<MediaAsset, AppError>;

    async fn find_by_id(&self, id: i64) -> Result<Option<MediaAsset>, AppError>;

    async fn find_by_id_for_owner(
        &self,
        id: i64,
        owner_user_id: &str,
    ) -> Result<Option<MediaAsset>, AppError>;

    async fn list_by_owner(&self, owner_user_id: &str) -> Result<Vec<MediaAsset>, AppError>;

    async fn delete_by_id_for_owner(&self, id: i64, owner_user_id: &str) -> Result<bool, AppError>;

    async fn provider_exists(
        &self,
        provider: MediaProvider,
        provider_file_id: &str,
    ) -> Result<bool, AppError>;

    async fn mark_attached(&self, ids: &[i64]) -> Result<(), AppError>;

    async fn validate_exists(&self, ids: &[i64]) -> Result<(), AppError>;
}
