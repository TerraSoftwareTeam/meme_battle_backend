use async_trait::async_trait;
use crate::common::http::error::AppError;

#[async_trait]
pub trait RealtimePublisher: Send + Sync {
    async fn publish(&self, channel: &str, payload: serde_json::Value) -> Result<(), AppError>;
}
