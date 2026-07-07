use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info};
use sqlx::{PgPool, postgres::PgListener};

use crate::common::http::error::AppError;
use crate::features::realtime::domain::ports::{OutboxRepository, RealtimePublisher};

pub struct OutboxProcessor {
    repo: Arc<dyn OutboxRepository>,
    client: Arc<dyn RealtimePublisher>,
}

impl OutboxProcessor {
    pub fn new(repo: Arc<dyn OutboxRepository>, client: Arc<dyn RealtimePublisher>) -> Self {
        Self { repo, client }
    }

    /// Start the background worker loop that handles database notifications and periodic retries
    pub fn start(self: Arc<Self>, pool: PgPool, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
        info!("Starting unified realtime outbox processor with PostgreSQL LISTEN/NOTIFY");
        tokio::spawn(async move {
            let mut retry_ticker = tokio::time::interval(Duration::from_secs(5));
            retry_ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

            let mut listener = Self::connect_listener(&pool).await;

            loop {
                if *shutdown_rx.borrow() {
                    info!("Shutdown signal received, stopping outbox processor");
                    break;
                }

                let (run_pending, run_retry) = tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Shutdown signal received, stopping outbox processor");
                        break;
                    }
                    notification = async {
                        if let Some(ref mut l) = listener {
                            Some(l.recv().await)
                        } else {
                            // Sleep to avoid busy looping when listener is None
                            tokio::time::sleep(Duration::from_secs(2)).await;
                            None
                        }
                    } => {
                        match notification {
                            Some(Ok(n)) => {
                                debug!("Received database notification on channel '{}'", n.channel());
                                (true, false)
                            }
                            Some(Err(err)) => {
                                error!("PostgreSQL listener connection error: {:?}. Attempting to reconnect...", err);
                                tokio::time::sleep(Duration::from_secs(2)).await;
                                listener = Self::connect_listener(&pool).await;
                                (true, true)
                            }
                            None => {
                                // Sleep and retry establishing connection
                                tokio::time::sleep(Duration::from_secs(5)).await;
                                listener = Self::connect_listener(&pool).await;
                                (true, true)
                            }
                        }
                    }
                    _ = retry_ticker.tick() => {
                        (true, true)
                    }
                };

                // Work execution happens outside tokio::select!, guaranteeing that
                // tasks are never cancelled mid-flight and run sequentially.
                if run_pending {
                    if let Err(err) = self.process_pending().await {
                        error!("Error processing pending outbox events: {:?}", err);
                    }
                }
                if run_retry {
                    if let Err(err) = self.retry_failed().await {
                        error!("Error retrying failed outbox events: {:?}", err);
                    }
                }
            }
        })
    }

    async fn connect_listener(pool: &PgPool) -> Option<PgListener> {
        match PgListener::connect_with(pool).await {
            Ok(mut l) => {
                if let Err(err) = l.listen("realtime_outbox_inserted").await {
                    error!("Failed to listen to channel 'realtime_outbox_inserted': {:?}", err);
                    None
                } else {
                    info!("Successfully listening to 'realtime_outbox_inserted' notification channel");
                    Some(l)
                }
            }
            Err(err) => {
                error!("Failed to connect PgListener: {:?}", err);
                None
            }
        }
    }

    async fn process_pending(&self) -> Result<(), AppError> {
        // 1. Fetch pending rows and temporarily lock them (1 min) in a short transaction
        let rows = self.repo.fetch_and_lock_pending().await?;
        if rows.is_empty() {
            return Ok(());
        }

        debug!("Publishing {} pending realtime outbox events sequentially", rows.len());

        let mut successes = Vec::new();
        let mut failures = Vec::new();

        // 2. Publish fetched rows sequentially to preserve strict FIFO delivery order.
        //    Crucially, database connections are NOT held open during these HTTP calls.
        for row in rows {
            match self.client.publish(&row.channel, row.payload).await {
                Ok(_) => {
                    debug!("Successfully published realtime event id={}", row.event_id);
                    successes.push(row.event_id);
                }
                Err(err) => {
                    error!("Failed to publish event id={} to Centrifugo: {:?}", row.event_id, err);
                    let (next_retry, next_retry_at) = self.calculate_next_retry(row.retry_count);
                    failures.push((row.event_id, next_retry, next_retry_at));
                }
            }
        }

        // 3. Update database status based on publishing results in a single short transaction
        self.repo.update_processed_results(&successes, &failures).await?;

        Ok(())
    }

    async fn retry_failed(&self) -> Result<(), AppError> {
        // 1. Fetch retryable rows and temporarily lock them (1 min) in a short transaction
        let rows = self.repo.fetch_and_lock_retryable().await?;
        if rows.is_empty() {
            return Ok(());
        }

        debug!("Retrying {} failed outbox events sequentially", rows.len());

        let mut successes = Vec::new();
        let mut failures = Vec::new();

        // 2. Publish retryable rows sequentially to preserve FIFO order
        for row in rows {
            match self.client.publish(&row.channel, row.payload).await {
                Ok(_) => {
                    debug!("Successfully published event id={} on retry", row.event_id);
                    successes.push(row.event_id);
                }
                Err(err) => {
                    error!(
                        "Failed to retry publish request to Centrifugo for event id={}: {:?}",
                        row.event_id, err
                    );
                    let (next_retry, next_retry_at) = self.calculate_next_retry(row.retry_count);
                    failures.push((row.event_id, next_retry, next_retry_at));
                }
            }
        }

        // 3. Delete/Update state directly on database in a single short transaction
        self.repo.update_processed_results(&successes, &failures).await?;

        Ok(())
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
