use async_trait::async_trait;

use crate::common::http::error::AppError;

#[async_trait]
pub trait MediaAssetResolver: Send + Sync {
    async fn resolve_url(&self, media_asset_id: i64) -> Result<Option<String>, AppError>;
}
