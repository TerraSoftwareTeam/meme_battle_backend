use std::sync::Arc;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use crate::common::http::error::AppError;
use crate::features::realtime::domain::{
    ports::OutboxRepository,
    model::{RealtimePayload, RealtimeEnvelope},
};

pub struct PublishNotificationCommand {
    repo: Arc<dyn OutboxRepository>,
}

impl PublishNotificationCommand {
    pub fn new(repo: Arc<dyn OutboxRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        channel: &str,
        version: i64,
        payload: RealtimePayload,
        user_id: Option<Uuid>,
    ) -> Result<(), AppError> {
        let event = match user_id {
            Some(uid) => RealtimeEnvelope::personal(game_id, uid, version, payload),
            None => RealtimeEnvelope::all(game_id, version, payload),
        };
        let val = serde_json::to_value(&event).map_err(|_| AppError::InternalError)?;
        self.repo.queue_event(tx, event.event_id, game_id, channel, val).await
    }
}
