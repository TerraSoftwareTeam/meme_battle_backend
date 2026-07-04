use async_trait::async_trait;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::common::http::error::AppError;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OutboxEvent {
    pub event_id: Uuid,
    pub game_id: Uuid,
    pub channel: String,
    pub payload: serde_json::Value,
    pub retry_count: i32,
}

#[async_trait]
pub trait OutboxRepository: Send + Sync {
    async fn queue_event(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        game_id: Uuid,
        channel: &str,
        payload: serde_json::Value,
    ) -> Result<(), AppError>;

    async fn fetch_and_lock_pending(&self) -> Result<Vec<OutboxEvent>, AppError>;
    async fn fetch_and_lock_retryable(&self) -> Result<Vec<OutboxEvent>, AppError>;
    async fn delete_event(&self, event_id: Uuid) -> Result<(), AppError>;
    async fn update_retry(&self, event_id: Uuid, retry_count: i32, next_retry_at: DateTime<Utc>) -> Result<(), AppError>;
    async fn update_processed_results(
        &self,
        successes: &[Uuid],
        failures: &[(Uuid, i32, DateTime<Utc>)],
    ) -> Result<(), AppError>;
}
