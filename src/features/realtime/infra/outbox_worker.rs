use sqlx::{postgres::PgListener, PgPool};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};

use crate::common::http::error::AppError;
use crate::features::realtime::domain::ports::{OutboxRepository, RealtimePublisher};

/// Background worker that reads queued realtime notifications from PostgreSQL outbox
/// and reliably publishes them to Centrifugo WebSocket channels.
pub struct RealtimeOutboxWorker {
    repo: Arc<dyn OutboxRepository>,
    client: Arc<dyn RealtimePublisher>,
    pool: PgPool,
}

/// Backward compatibility alias
pub type OutboxProcessor = RealtimeOutboxWorker;

impl RealtimeOutboxWorker {
    pub fn new(
        repo: Arc<dyn OutboxRepository>,
        client: Arc<dyn RealtimePublisher>,
        pool: PgPool,
    ) -> Self {
        Self { repo, client, pool }
    }

    /// Starts the background worker loop that handles database notifications and periodic retries.
    pub fn start(
        self: Arc<Self>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        let pool = self.pool.clone();
        info!(
            worker = "realtime_outbox",
            "Starting realtime outbox worker with PostgreSQL LISTEN/NOTIFY"
        );

        tokio::spawn(async move {
            let mut retry_ticker = tokio::time::interval(Duration::from_secs(5));
            retry_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut listener = Self::connect_listener(&pool).await;
            let mut backoff_secs = 1;

            loop {
                if *shutdown_rx.borrow() {
                    info!(
                        worker = "realtime_outbox",
                        "Shutdown signal received, stopping realtime outbox worker"
                    );
                    break;
                }

                let (run_pending, run_retry) = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!(
                            worker = "realtime_outbox",
                            "Shutdown signal received during select, stopping realtime outbox worker"
                        );
                        break;
                    }
                    notification = async {
                        if let Some(ref mut l) = listener {
                            Some(l.recv().await)
                        } else {
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            None
                        }
                    } => {
                        match notification {
                            Some(Ok(n)) => {
                                debug!(
                                    worker = "realtime_outbox",
                                    channel = n.channel(),
                                    "Received database notification"
                                );
                                (true, false)
                            }
                            Some(Err(err)) => {
                                error!(
                                    worker = "realtime_outbox",
                                    error = ?err,
                                    "PostgreSQL listener error. Reconnecting..."
                                );
                                tokio::select! {
                                    _ = shutdown_rx.changed() => {
                                        info!(
                                            worker = "realtime_outbox",
                                            "Shutdown signal received during reconnect sleep, stopping worker"
                                        );
                                        break;
                                    }
                                    _ = tokio::time::sleep(Duration::from_secs(2)) => {}
                                }
                                listener = Self::connect_listener(&pool).await;
                                (true, true)
                            }
                            None => {
                                listener = Self::connect_listener(&pool).await;
                                (true, true)
                            }
                        }
                    }
                    _ = retry_ticker.tick() => {
                        (true, true)
                    }
                };

                // Work execution is isolated in a separate task to guarantee panic safety.
                // Outbox tasks run sequentially outside select! to prevent cancellation mid-flight.
                let self_clone = self.clone();
                let task_handle = tokio::spawn(async move {
                    let mut pending_count = 0;
                    let mut retry_count = 0;
                    if run_pending {
                        pending_count = self_clone.process_pending().await?;
                    }
                    if run_retry {
                        retry_count = self_clone.retry_failed().await?;
                    }
                    Ok::<_, AppError>((pending_count, retry_count))
                });

                match task_handle.await {
                    Ok(Ok((pending, retried))) => {
                        backoff_secs = 1;
                        if pending > 0 || retried > 0 {
                            debug!(
                                worker = "realtime_outbox",
                                pending_count = pending,
                                retry_count = retried,
                                "Successfully processed outbox batch"
                            );
                        }

                        // Queue draining: if either pending or retries reached batch limit (100),
                        // continue immediately to process next items without waiting for next ticker tick.
                        if pending >= 100 || retried >= 100 {
                            debug!(
                                worker = "realtime_outbox",
                                "Batch limit reached, draining next outbox items immediately"
                            );
                            tokio::task::yield_now().await;
                            continue;
                        }
                    }
                    Ok(Err(err)) => {
                        error!(
                            worker = "realtime_outbox",
                            error = ?err,
                            "Error processing outbox events"
                        );
                        backoff_secs = std::cmp::min(backoff_secs * 2, 16);
                        tokio::select! {
                            _ = shutdown_rx.changed() => break,
                            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                        }
                    }
                    Err(join_err) => {
                        if join_err.is_panic() {
                            error!(
                                worker = "realtime_outbox",
                                "Panic detected in outbox dispatch task! Continuing safely."
                            );
                        } else {
                            error!(
                                worker = "realtime_outbox",
                                error = ?join_err,
                                "Outbox dispatch task joined with error"
                            );
                        }
                        backoff_secs = std::cmp::min(backoff_secs * 2, 16);
                        tokio::select! {
                            _ = shutdown_rx.changed() => break,
                            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                        }
                    }
                }
            }
        })
    }

    async fn connect_listener(pool: &PgPool) -> Option<PgListener> {
        match PgListener::connect_with(pool).await {
            Ok(mut l) => {
                if let Err(err) = l.listen("realtime_outbox_inserted").await {
                    error!(
                        worker = "realtime_outbox",
                        error = ?err,
                        "Failed to listen to channel 'realtime_outbox_inserted'"
                    );
                    None
                } else {
                    info!(
                        worker = "realtime_outbox",
                        "Successfully listening to 'realtime_outbox_inserted' notification channel"
                    );
                    Some(l)
                }
            }
            Err(err) => {
                error!(
                    worker = "realtime_outbox",
                    error = ?err,
                    "Failed to connect PgListener"
                );
                None
            }
        }
    }

    /// Fetches pending rows, publishes them sequentially to Centrifugo, and updates their status.
    /// Returns the number of events processed.
    pub async fn process_pending(&self) -> Result<usize, AppError> {
        let rows = self.repo.fetch_and_lock_pending().await?;
        if rows.is_empty() {
            return Ok(0);
        }

        let count = rows.len();
        debug!(
            worker = "realtime_outbox",
            count = count,
            "Publishing pending realtime outbox events sequentially"
        );

        let mut successes = Vec::new();
        let mut failures = Vec::new();

        for row in rows {
            match self.client.publish(&row.channel, row.payload).await {
                Ok(_) => {
                    debug!(
                        worker = "realtime_outbox",
                        event_id = %row.event_id,
                        "Successfully published realtime event"
                    );
                    successes.push(row.event_id);
                }
                Err(err) => {
                    error!(
                        worker = "realtime_outbox",
                        event_id = %row.event_id,
                        error = ?err,
                        "Failed to publish event to Centrifugo"
                    );
                    let (next_retry, next_retry_at) = self.calculate_next_retry(row.retry_count);
                    failures.push((row.event_id, next_retry, next_retry_at));
                }
            }
        }

        self.repo
            .update_processed_results(&successes, &failures)
            .await?;

        Ok(count)
    }

    /// Fetches retryable rows, publishes them sequentially to Centrifugo, and updates their status.
    /// Returns the number of events processed.
    pub async fn retry_failed(&self) -> Result<usize, AppError> {
        let rows = self.repo.fetch_and_lock_retryable().await?;
        if rows.is_empty() {
            return Ok(0);
        }

        let count = rows.len();
        debug!(
            worker = "realtime_outbox",
            count = count,
            "Retrying failed outbox events sequentially"
        );

        let mut successes = Vec::new();
        let mut failures = Vec::new();

        for row in rows {
            match self.client.publish(&row.channel, row.payload).await {
                Ok(_) => {
                    debug!(
                        worker = "realtime_outbox",
                        event_id = %row.event_id,
                        "Successfully published event on retry"
                    );
                    successes.push(row.event_id);
                }
                Err(err) => {
                    error!(
                        worker = "realtime_outbox",
                        event_id = %row.event_id,
                        error = ?err,
                        "Failed to retry publish request to Centrifugo"
                    );
                    let (next_retry, next_retry_at) = self.calculate_next_retry(row.retry_count);
                    failures.push((row.event_id, next_retry, next_retry_at));
                }
            }
        }

        self.repo
            .update_processed_results(&successes, &failures)
            .await?;

        Ok(count)
    }

    fn calculate_next_retry(&self, current_retry: i32) -> (i32, chrono::DateTime<chrono::Utc>) {
        let next_retry = current_retry + 1;
        let capped_retry = std::cmp::min(next_retry, 10);
        // Exponential backoff: 2^(capped_retry) * 2 seconds
        let delay_secs = 2_i64.saturating_pow(capped_retry as u32).saturating_mul(2);
        let next_retry_at = chrono::Utc::now() + chrono::Duration::seconds(delay_secs);
        (next_retry, next_retry_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::realtime::domain::ports::outbox_repository::OutboxEvent;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use serde_json::json;
    use sqlx::{Postgres, Transaction};
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockOutboxRepository {
        pending_rows: Mutex<Vec<OutboxEvent>>,
        retryable_rows: Mutex<Vec<OutboxEvent>>,
        updated_successes: Mutex<Vec<Uuid>>,
        updated_failures: Mutex<Vec<(Uuid, i32, DateTime<Utc>)>>,
    }

    impl MockOutboxRepository {
        fn new(pending: Vec<OutboxEvent>, retryable: Vec<OutboxEvent>) -> Self {
            Self {
                pending_rows: Mutex::new(pending),
                retryable_rows: Mutex::new(retryable),
                updated_successes: Mutex::new(Vec::new()),
                updated_failures: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl OutboxRepository for MockOutboxRepository {
        async fn queue_event(
            &self,
            _tx: &mut Transaction<'_, Postgres>,
            _event_id: Uuid,
            _game_id: Uuid,
            _channel: &str,
            _payload: serde_json::Value,
        ) -> Result<(), AppError> {
            Ok(())
        }

        async fn fetch_and_lock_pending(&self) -> Result<Vec<OutboxEvent>, AppError> {
            Ok(self.pending_rows.lock().unwrap().clone())
        }

        async fn fetch_and_lock_retryable(&self) -> Result<Vec<OutboxEvent>, AppError> {
            Ok(self.retryable_rows.lock().unwrap().clone())
        }

        async fn delete_event(&self, _event_id: Uuid) -> Result<(), AppError> {
            Ok(())
        }

        async fn update_retry(
            &self,
            _event_id: Uuid,
            _retry_count: i32,
            _next_retry_at: DateTime<Utc>,
        ) -> Result<(), AppError> {
            Ok(())
        }

        async fn update_processed_results(
            &self,
            successes: &[Uuid],
            failures: &[(Uuid, i32, DateTime<Utc>)],
        ) -> Result<(), AppError> {
            self.updated_successes
                .lock()
                .unwrap()
                .extend_from_slice(successes);
            self.updated_failures
                .lock()
                .unwrap()
                .extend_from_slice(failures);
            Ok(())
        }
    }

    struct MockPublisher {
        should_fail: Mutex<bool>,
        published: Mutex<Vec<(String, serde_json::Value)>>,
    }

    impl MockPublisher {
        fn new(should_fail: bool) -> Self {
            Self {
                should_fail: Mutex::new(should_fail),
                published: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl RealtimePublisher for MockPublisher {
        async fn publish(&self, channel: &str, payload: serde_json::Value) -> Result<(), AppError> {
            if *self.should_fail.lock().unwrap() {
                return Err(AppError::ProviderError(
                    "Centrifugo unavailable".to_string(),
                ));
            }
            self.published
                .lock()
                .unwrap()
                .push((channel.to_string(), payload));
            Ok(())
        }
    }

    fn sample_event(event_id: Uuid, retry_count: i32) -> OutboxEvent {
        OutboxEvent {
            event_id,
            game_id: Uuid::new_v4(),
            channel: "game:123".to_string(),
            payload: json!({"key": "val"}),
            retry_count,
        }
    }

    #[tokio::test]
    async fn test_process_pending_publishes_and_updates_status() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let repo = Arc::new(MockOutboxRepository::new(
            vec![sample_event(id1, 0), sample_event(id2, 0)],
            Vec::new(),
        ));
        let publisher = Arc::new(MockPublisher::new(false));
        let pool = PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let worker = RealtimeOutboxWorker::new(repo.clone(), publisher.clone(), pool);

        let count = worker.process_pending().await.unwrap();
        assert_eq!(count, 2);

        assert_eq!(publisher.published.lock().unwrap().len(), 2);
        assert_eq!(*repo.updated_successes.lock().unwrap(), vec![id1, id2]);
        assert!(repo.updated_failures.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_process_pending_handles_publisher_error_with_retry() {
        let id = Uuid::new_v4();
        let repo = Arc::new(MockOutboxRepository::new(
            vec![sample_event(id, 1)],
            Vec::new(),
        ));
        let publisher = Arc::new(MockPublisher::new(true)); // will fail
        let pool = PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let worker = RealtimeOutboxWorker::new(repo.clone(), publisher.clone(), pool);

        let count = worker.process_pending().await.unwrap();
        assert_eq!(count, 1);

        assert!(repo.updated_successes.lock().unwrap().is_empty());
        let failures = repo.updated_failures.lock().unwrap();
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].0, id); // event_id
        assert_eq!(failures[0].1, 2); // next retry count
    }

    #[tokio::test]
    async fn test_retry_failed_publishes_retryable_events() {
        let id = Uuid::new_v4();
        let repo = Arc::new(MockOutboxRepository::new(
            Vec::new(),
            vec![sample_event(id, 2)],
        ));
        let publisher = Arc::new(MockPublisher::new(false));
        let pool = PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let worker = RealtimeOutboxWorker::new(repo.clone(), publisher.clone(), pool);

        let count = worker.retry_failed().await.unwrap();
        assert_eq!(count, 1);

        assert_eq!(*repo.updated_successes.lock().unwrap(), vec![id]);
        assert!(repo.updated_failures.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_calculate_next_retry_exponential_backoff() {
        let repo = Arc::new(MockOutboxRepository::new(Vec::new(), Vec::new()));
        let publisher = Arc::new(MockPublisher::new(false));
        let pool = PgPool::connect_lazy("postgres://localhost/test").unwrap();
        let worker = RealtimeOutboxWorker::new(repo, publisher, pool);

        let (retry1, next_at1) = worker.calculate_next_retry(0);
        assert_eq!(retry1, 1);
        assert!(next_at1 > Utc::now());

        let (retry3, _) = worker.calculate_next_retry(2);
        assert_eq!(retry3, 3);
    }
}
