use crate::common::http::error::AppError;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait GameMediaManager: Send + Sync {
    async fn resolve_url(&self, media_id: i64) -> Result<Option<String>, AppError>;
    async fn validate_media_exists(&self, media_ids: &[i64]) -> Result<(), AppError>;
    async fn validate_media_ownership(&self, media_ids: &[i64], author_id: Uuid) -> Result<(), AppError>;
    async fn enqueue_purge_for_media_ids(&self, media_ids: &[i64]) -> Result<(), AppError>;
}
