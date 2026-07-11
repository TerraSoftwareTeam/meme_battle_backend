use async_trait::async_trait;
use crate::common::http::error::AppError;

#[async_trait]
pub trait GameMediaManager: Send + Sync {
    async fn resolve_url(&self, media_id: i64) -> Result<Option<String>, AppError>;
    async fn validate_media_exists(&self, media_ids: &[i64]) -> Result<(), AppError>;
}
