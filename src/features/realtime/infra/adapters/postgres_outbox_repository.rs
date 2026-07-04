use async_trait::async_trait;
use sqlx::{Postgres, Transaction, PgPool};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::common::http::error::AppError;
use crate::features::realtime::domain::ports::outbox_repository::{OutboxRepository, OutboxEvent};

pub struct PostgresOutboxRepository {
    pool: PgPool,
}

impl PostgresOutboxRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl OutboxRepository for PostgresOutboxRepository {
    async fn queue_event(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        event_id: Uuid,
        game_id: Uuid,
        channel: &str,
        payload: serde_json::Value,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            INSERT INTO realtime_outbox (event_id, game_id, channel, payload)
            VALUES ($1, $2, $3, $4)
            "#,
        )
        .bind(event_id)
        .bind(game_id)
        .bind(channel)
        .bind(payload)
        .execute(&mut **tx)
        .await?;

        Ok(())
    }

    async fn fetch_and_lock_pending(&self) -> Result<Vec<OutboxEvent>, AppError> {
        let mut tx = self.pool.begin().await?;

        let rows = sqlx::query_as::<_, OutboxEvent>(
            r#"
            SELECT event_id, game_id, channel, payload, retry_count 
            FROM realtime_outbox o
            WHERE o.retry_count = 0 
              AND o.next_retry_at <= now()
              AND NOT EXISTS (
                  SELECT 1 FROM realtime_outbox active
                  WHERE active.game_id = o.game_id
                    AND active.next_retry_at > now()
                    AND active.retry_count <= 10
              )
            ORDER BY o.created_at ASC 
            LIMIT 100
            FOR UPDATE SKIP LOCKED
            "#
        )
        .fetch_all(&mut *tx)
        .await?;

        if !rows.is_empty() {
            let ids = rows.iter().map(|r| r.event_id).collect::<Vec<_>>();
            sqlx::query(
                r#"
                UPDATE realtime_outbox
                SET next_retry_at = now() + INTERVAL '1 minute'
                WHERE event_id = ANY($1)
                "#
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(rows)
    }

    async fn fetch_and_lock_retryable(&self) -> Result<Vec<OutboxEvent>, AppError> {
        let mut tx = self.pool.begin().await?;

        let rows = sqlx::query_as::<_, OutboxEvent>(
            r#"
            SELECT event_id, game_id, channel, payload, retry_count
            FROM realtime_outbox o
            WHERE o.retry_count > 0 AND o.retry_count <= 10 
              AND o.next_retry_at <= now()
              AND NOT EXISTS (
                  SELECT 1 FROM realtime_outbox active
                  WHERE active.game_id = o.game_id
                    AND active.next_retry_at > now()
                    AND active.retry_count <= 10
              )
            ORDER BY o.next_retry_at ASC, o.created_at ASC
            LIMIT 50
            FOR UPDATE SKIP LOCKED
            "#
        )
        .fetch_all(&mut *tx)
        .await?;

        if !rows.is_empty() {
            let ids = rows.iter().map(|r| r.event_id).collect::<Vec<_>>();
            sqlx::query(
                r#"
                UPDATE realtime_outbox
                SET next_retry_at = now() + INTERVAL '1 minute'
                WHERE event_id = ANY($1)
                "#
            )
            .bind(&ids)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(rows)
    }

    async fn delete_event(&self, event_id: Uuid) -> Result<(), AppError> {
        sqlx::query("DELETE FROM realtime_outbox WHERE event_id = $1")
            .bind(event_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn update_retry(&self, event_id: Uuid, retry_count: i32, next_retry_at: DateTime<Utc>) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE realtime_outbox
            SET retry_count = $1, next_retry_at = $2
            WHERE event_id = $3
            "#
        )
        .bind(retry_count)
        .bind(next_retry_at)
        .bind(event_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn update_processed_results(
        &self,
        successes: &[Uuid],
        failures: &[(Uuid, i32, DateTime<Utc>)],
    ) -> Result<(), AppError> {
        if successes.is_empty() && failures.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        if !successes.is_empty() {
            sqlx::query(
                "DELETE FROM realtime_outbox WHERE event_id = ANY($1)"
            )
            .bind(successes)
            .execute(&mut *tx)
            .await?;
        }

        for &(event_id, retry_count, next_retry_at) in failures {
            sqlx::query(
                r#"
                UPDATE realtime_outbox
                SET retry_count = $1, next_retry_at = $2
                WHERE event_id = $3
                "#
            )
            .bind(retry_count)
            .bind(next_retry_at)
            .bind(event_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
