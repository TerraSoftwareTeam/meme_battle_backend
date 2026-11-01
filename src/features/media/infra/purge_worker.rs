use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::common::http::error::AppError;
use crate::features::media::{FileStorage, MediaRepository};

/// Background worker that purges soft-deleted media assets from CDN storage and database records,
/// respecting active game locks and pack dependencies.
pub struct MediaPurgeWorker {
    media_repo: Arc<dyn MediaRepository>,
    file_storage: Arc<dyn FileStorage>,
    pending_ttl: chrono::Duration,
}

/// Backward compatibility alias
pub type MediaCleanupWorker = MediaPurgeWorker;

impl MediaPurgeWorker {
    pub fn new(media_repo: Arc<dyn MediaRepository>, file_storage: Arc<dyn FileStorage>) -> Self {
        Self {
            media_repo,
            file_storage,
            pending_ttl: chrono::Duration::days(1),
        }
    }

    pub fn with_pending_ttl(mut self, ttl: chrono::Duration) -> Self {
        self.pending_ttl = ttl;
        self
    }

    pub fn pending_ttl(&self) -> chrono::Duration {
        self.pending_ttl
    }

    pub fn start(
        self: Arc<Self>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        info!(
            worker = "media_purge",
            "Starting media purge background worker"
        );
        tokio::spawn(async move {
            let mut backoff_secs = 1;
            loop {
                if *shutdown_rx.borrow() {
                    info!(
                        worker = "media_purge",
                        "Shutdown signal received, stopping media purge worker"
                    );
                    break;
                }

                let self_clone = self.clone();
                let task_handle =
                    tokio::spawn(async move { self_clone.process_pending_purges().await });

                match task_handle.await {
                    Ok(Ok(processed_count)) => {
                        backoff_secs = 1;
                        if processed_count >= 20 {
                            debug!(
                                worker = "media_purge",
                                count = processed_count,
                                "Batch full, draining next purge items immediately"
                            );
                            tokio::task::yield_now().await;
                            continue;
                        }

                        // Queue drained or empty, sleep default 10s interval
                        tokio::select! {
                            _ = shutdown_rx.changed() => {
                                info!(
                                    worker = "media_purge",
                                    "Shutdown signal received during sleep, stopping media purge worker"
                                );
                                break;
                            }
                            _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                        }
                    }
                    Ok(Err(err)) => {
                        error!(
                            worker = "media_purge",
                            error = ?err,
                            "Error in media purge worker iteration"
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
                                worker = "media_purge",
                                "Panic detected in media purge task! Continuing safely."
                            );
                        } else {
                            error!(
                                worker = "media_purge",
                                error = ?join_err,
                                "Media purge task joined with error"
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

    /// Purges stale pending media assets directly and processes up to 20 queue purge tasks sequentially.
    /// Returns the total number of items handled in this batch.
    pub async fn process_pending_purges(&self) -> Result<usize, AppError> {
        let mut total_processed = 0;

        // 1. Directly purge stale pending media assets abandoned > pending_ttl ago (default 1 day)
        let older_than = Utc::now() - self.pending_ttl;
        let stale_assets = self
            .media_repo
            .fetch_stale_pending_media(older_than, 20)
            .await?;

        for asset in stale_assets {
            // Safety check: is this media asset actually used in any non-deleted pack?
            // If so, it shouldn't be pending, so self-heal it to 'attached' instead of deleting!
            if self
                .media_repo
                .is_media_used_in_active_packs(asset.id)
                .await?
            {
                warn!(
                    worker = "media_purge",
                    media_asset_id = asset.id,
                    "Found stale pending media that is actually used in active packs. Self-healing to 'attached'."
                );
                self.media_repo.mark_attached(&[asset.id]).await?;
                continue;
            }

            let delete_result = self.file_storage.delete(&asset.provider_file_id).await;
            match delete_result {
                Ok(()) | Err(AppError::NotFound(_)) | Err(AppError::ValidationError(_)) => {
                    info!(
                        worker = "media_purge",
                        media_asset_id = asset.id,
                        file_id = %asset.provider_file_id,
                        "Purged stale pending media from CDN, deleting database record"
                    );
                    self.media_repo.delete_by_id(asset.id).await?;
                    total_processed += 1;
                }
                Err(err) => {
                    error!(
                        worker = "media_purge",
                        media_asset_id = asset.id,
                        file_id = %asset.provider_file_id,
                        error = ?err,
                        "Failed to delete stale pending media from CDN"
                    );
                }
            }
        }

        // 2. Fetch up to 20 purge tasks from media_purge_tasks (for soft-deleted packs/memes)
        let tasks = self.media_repo.fetch_pending_purge_tasks(20).await?;
        for task in tasks {
            // 1. Is this media asset currently in an active game?
            if self
                .media_repo
                .is_media_locked_in_games(task.media_asset_id)
                .await?
            {
                // Defer task for 15 seconds so active game can finish
                let next_retry = Utc::now() + chrono::Duration::seconds(15);
                self.media_repo
                    .fail_purge_task(task.id, "Media locked in active game session", next_retry)
                    .await?;
                continue;
            }

            // 2. Is this media asset still used in another active pack?
            if self
                .media_repo
                .is_media_used_in_active_packs(task.media_asset_id)
                .await?
            {
                info!(
                    worker = "media_purge",
                    media_asset_id = task.media_asset_id,
                    "Media is still in use by another active pack. Canceling purge task."
                );
                self.media_repo.complete_purge_task(task.id).await?;
                total_processed += 1;
                continue;
            }

            // 3. Purge from CDN
            let delete_result = self.file_storage.delete(&task.provider_file_id).await;
            match delete_result {
                Ok(()) | Err(AppError::NotFound(_)) => {
                    info!(
                        worker = "media_purge",
                        media_asset_id = task.media_asset_id,
                        file_id = %task.provider_file_id,
                        "Successfully deleted file from CDN, removing media asset record"
                    );
                    self.media_repo.delete_by_id(task.media_asset_id).await?;
                    self.media_repo.complete_purge_task(task.id).await?;
                    total_processed += 1;
                }
                Err(err) => {
                    error!(
                        worker = "media_purge",
                        media_asset_id = task.media_asset_id,
                        file_id = %task.provider_file_id,
                        error = ?err,
                        "Failed to delete file from CDN"
                    );
                    let backoff = chrono::Duration::seconds(
                        (2_i64.pow((task.retry_count.min(10) + 1) as u32) * 10).min(3600),
                    );
                    let next_retry = Utc::now() + backoff;
                    self.media_repo
                        .fail_purge_task(task.id, &err.to_string(), next_retry)
                        .await?;
                }
            }
        }

        Ok(total_processed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::media::{
        CreateMediaAsset, MediaAsset, MediaProvider, MediaPurgeTask, MediaStatus, MediaVisibility,
        StoredFile, UploadFile,
    };
    use async_trait::async_trait;
    use chrono::DateTime;
    use std::sync::Mutex;
    use uuid::Uuid;

    struct MockMediaRepository {
        tasks: Mutex<Vec<MediaPurgeTask>>,
        is_locked: Mutex<bool>,
        is_used: Mutex<bool>,
        completed_task_ids: Mutex<Vec<Uuid>>,
        failed_tasks: Mutex<Vec<(Uuid, String, DateTime<Utc>)>>,
        deleted_media_ids: Mutex<Vec<i64>>,
        stale_pending: Mutex<Vec<MediaAsset>>,
        fetched_stale_args: Mutex<Vec<(DateTime<Utc>, i64)>>,
        marked_attached_ids: Mutex<Vec<i64>>,
    }

    impl MockMediaRepository {
        fn new(tasks: Vec<MediaPurgeTask>) -> Self {
            Self {
                tasks: Mutex::new(tasks),
                is_locked: Mutex::new(false),
                is_used: Mutex::new(false),
                completed_task_ids: Mutex::new(Vec::new()),
                failed_tasks: Mutex::new(Vec::new()),
                deleted_media_ids: Mutex::new(Vec::new()),
                stale_pending: Mutex::new(Vec::new()),
                fetched_stale_args: Mutex::new(Vec::new()),
                marked_attached_ids: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl MediaRepository for MockMediaRepository {
        async fn create(&self, _asset: CreateMediaAsset) -> Result<MediaAsset, AppError> {
            todo!()
        }
        async fn find_by_id(&self, _id: i64) -> Result<Option<MediaAsset>, AppError> {
            todo!()
        }
        async fn find_by_id_for_owner(
            &self,
            _id: i64,
            _owner_user_id: &str,
        ) -> Result<Option<MediaAsset>, AppError> {
            todo!()
        }
        async fn list_by_owner(&self, _owner_user_id: &str) -> Result<Vec<MediaAsset>, AppError> {
            todo!()
        }
        async fn delete_by_id_for_owner(
            &self,
            _id: i64,
            _owner_user_id: &str,
        ) -> Result<bool, AppError> {
            todo!()
        }
        async fn delete_by_id(&self, id: i64) -> Result<bool, AppError> {
            self.deleted_media_ids.lock().unwrap().push(id);
            Ok(true)
        }
        async fn provider_exists(
            &self,
            _provider: MediaProvider,
            _provider_file_id: &str,
        ) -> Result<bool, AppError> {
            todo!()
        }
        async fn mark_attached(&self, ids: &[i64]) -> Result<(), AppError> {
            self.marked_attached_ids
                .lock()
                .unwrap()
                .extend_from_slice(ids);
            Ok(())
        }
        async fn validate_exists(&self, _ids: &[i64]) -> Result<(), AppError> {
            todo!()
        }
        async fn enqueue_purge_tasks_for_media_ids(
            &self,
            _media_ids: &[i64],
        ) -> Result<(), AppError> {
            todo!()
        }
        async fn fetch_stale_pending_media(
            &self,
            older_than: DateTime<Utc>,
            limit: i64,
        ) -> Result<Vec<MediaAsset>, AppError> {
            self.fetched_stale_args
                .lock()
                .unwrap()
                .push((older_than, limit));
            Ok(self.stale_pending.lock().unwrap().clone())
        }
        async fn fetch_pending_purge_tasks(
            &self,
            _limit: i64,
        ) -> Result<Vec<MediaPurgeTask>, AppError> {
            Ok(self.tasks.lock().unwrap().clone())
        }
        async fn complete_purge_task(&self, task_id: Uuid) -> Result<(), AppError> {
            self.completed_task_ids.lock().unwrap().push(task_id);
            Ok(())
        }
        async fn fail_purge_task(
            &self,
            task_id: Uuid,
            error: &str,
            next_retry_at: DateTime<Utc>,
        ) -> Result<(), AppError> {
            self.failed_tasks
                .lock()
                .unwrap()
                .push((task_id, error.to_string(), next_retry_at));
            Ok(())
        }
        async fn is_media_used_in_active_packs(
            &self,
            _media_asset_id: i64,
        ) -> Result<bool, AppError> {
            Ok(*self.is_used.lock().unwrap())
        }
        async fn is_media_locked_in_games(&self, _media_asset_id: i64) -> Result<bool, AppError> {
            Ok(*self.is_locked.lock().unwrap())
        }
    }

    struct MockFileStorage {
        delete_result: Mutex<Option<Result<(), AppError>>>,
        deleted_file_ids: Mutex<Vec<String>>,
    }

    impl MockFileStorage {
        fn new(result: Result<(), AppError>) -> Self {
            Self {
                delete_result: Mutex::new(Some(result)),
                deleted_file_ids: Mutex::new(Vec::new()),
            }
        }
    }

    #[async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, _file: UploadFile) -> Result<StoredFile, AppError> {
            todo!()
        }
        async fn upload_from_url(&self, _url: &str) -> Result<StoredFile, AppError> {
            todo!()
        }
        async fn delete(&self, provider_file_id: &str) -> Result<(), AppError> {
            self.deleted_file_ids
                .lock()
                .unwrap()
                .push(provider_file_id.to_string());
            self.delete_result.lock().unwrap().take().unwrap_or(Ok(()))
        }
    }

    fn sample_task(id: Uuid, media_asset_id: i64, file_id: &str) -> MediaPurgeTask {
        MediaPurgeTask {
            id,
            media_asset_id,
            provider: MediaProvider::HackClubCdn,
            provider_file_id: file_id.to_string(),
            retry_count: 0,
            next_retry_at: Utc::now(),
            last_error: None,
            created_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_cleanup_defers_when_locked_in_game() {
        let task_id = Uuid::new_v4();
        let repo = Arc::new(MockMediaRepository::new(vec![sample_task(
            task_id, 101, "file101",
        )]));
        *repo.is_locked.lock().unwrap() = true;

        let storage = Arc::new(MockFileStorage::new(Ok(())));
        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());

        worker.process_pending_purges().await.unwrap();

        // Must fail with retry backoff and message
        let failed = repo.failed_tasks.lock().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, task_id);
        assert!(failed[0].1.contains("Media locked in active game session"));

        // Must NOT delete from CDN or DB
        assert!(storage.deleted_file_ids.lock().unwrap().is_empty());
        assert!(repo.deleted_media_ids.lock().unwrap().is_empty());
        assert!(repo.completed_task_ids.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_skips_when_used_in_another_pack() {
        let task_id = Uuid::new_v4();
        let repo = Arc::new(MockMediaRepository::new(vec![sample_task(
            task_id, 102, "file102",
        )]));
        *repo.is_locked.lock().unwrap() = false;
        *repo.is_used.lock().unwrap() = true;

        let storage = Arc::new(MockFileStorage::new(Ok(())));
        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());

        worker.process_pending_purges().await.unwrap();

        // Must complete task (cancel purge) without deleting from CDN or DB
        assert_eq!(*repo.completed_task_ids.lock().unwrap(), vec![task_id]);
        assert!(storage.deleted_file_ids.lock().unwrap().is_empty());
        assert!(repo.deleted_media_ids.lock().unwrap().is_empty());
        assert!(repo.failed_tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_deletes_when_unlocked_and_unused() {
        let task_id = Uuid::new_v4();
        let repo = Arc::new(MockMediaRepository::new(vec![sample_task(
            task_id, 103, "file103",
        )]));
        *repo.is_locked.lock().unwrap() = false;
        *repo.is_used.lock().unwrap() = false;

        let storage = Arc::new(MockFileStorage::new(Ok(())));
        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());

        worker.process_pending_purges().await.unwrap();

        // Must call storage.delete, delete_by_id, and complete_purge_task
        assert_eq!(*storage.deleted_file_ids.lock().unwrap(), vec!["file103"]);
        assert_eq!(*repo.deleted_media_ids.lock().unwrap(), vec![103]);
        assert_eq!(*repo.completed_task_ids.lock().unwrap(), vec![task_id]);
        assert!(repo.failed_tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_treats_not_found_on_cdn_as_success() {
        let task_id = Uuid::new_v4();
        let repo = Arc::new(MockMediaRepository::new(vec![sample_task(
            task_id, 104, "file104",
        )]));
        *repo.is_locked.lock().unwrap() = false;
        *repo.is_used.lock().unwrap() = false;

        // CDN returns NotFound (already gone from CDN)
        let storage = Arc::new(MockFileStorage::new(Err(AppError::NotFound(
            "File not found on CDN".to_string(),
        ))));
        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());

        worker.process_pending_purges().await.unwrap();

        // Must still clean up DB record and complete task
        assert_eq!(*repo.deleted_media_ids.lock().unwrap(), vec![104]);
        assert_eq!(*repo.completed_task_ids.lock().unwrap(), vec![task_id]);
        assert!(repo.failed_tasks.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_backs_off_on_storage_error() {
        let task_id = Uuid::new_v4();
        let repo = Arc::new(MockMediaRepository::new(vec![sample_task(
            task_id, 105, "file105",
        )]));
        *repo.is_locked.lock().unwrap() = false;
        *repo.is_used.lock().unwrap() = false;

        let storage = Arc::new(MockFileStorage::new(Err(AppError::ProviderError(
            "CDN 500 error".to_string(),
        ))));
        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());

        worker.process_pending_purges().await.unwrap();

        // Must fail with backoff
        let failed = repo.failed_tasks.lock().unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].0, task_id);
        assert!(failed[0].1.contains("CDN 500 error"));

        // Must NOT delete from DB
        assert!(repo.deleted_media_ids.lock().unwrap().is_empty());
        assert!(repo.completed_task_ids.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_cleanup_worker_graceful_shutdown() {
        let repo = Arc::new(MockMediaRepository::new(Vec::new()));
        let storage = Arc::new(MockFileStorage::new(Ok(())));
        let worker = Arc::new(MediaCleanupWorker::new(repo, storage));

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let handle = worker.start(shutdown_rx);

        // Send shutdown
        shutdown_tx.send(true).unwrap();

        tokio::time::timeout(Duration::from_millis(500), handle)
            .await
            .expect("Worker must stop promptly upon shutdown signal")
            .unwrap();
    }

    #[tokio::test]
    async fn test_cleanup_worker_purges_stale_pending_media() {
        let repo = Arc::new(MockMediaRepository::new(Vec::new()));
        let storage = Arc::new(MockFileStorage::new(Ok(())));

        let sample_stale_asset = MediaAsset {
            id: 201,
            owner_user_id: Uuid::new_v4().to_string(),
            provider: MediaProvider::HackClubCdn,
            provider_file_id: "stale_file_201".to_string(),
            url: "https://cdn.example.com/stale_201.png".to_string(),
            filename: "stale.png".to_string(),
            content_type: "image/png".to_string(),
            size_bytes: 1024,
            status: MediaStatus::Pending,
            visibility: MediaVisibility::Private,
            created_at: Utc::now() - chrono::Duration::days(2),
        };
        repo.stale_pending.lock().unwrap().push(sample_stale_asset);

        // 1. Default TTL is 1 day (24 hours)
        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());
        assert_eq!(worker.pending_ttl(), chrono::Duration::days(1));

        let now_before = Utc::now();
        let processed = worker.process_pending_purges().await.unwrap();
        assert_eq!(processed, 1);

        // Verify CDN delete was called
        assert_eq!(
            *storage.deleted_file_ids.lock().unwrap(),
            vec!["stale_file_201"]
        );
        // Verify DB delete was called
        assert_eq!(*repo.deleted_media_ids.lock().unwrap(), vec![201]);

        {
            let args = repo.fetched_stale_args.lock().unwrap();
            assert_eq!(args.len(), 1);
            let (older_than, limit) = args[0];
            assert_eq!(limit, 20);
            let expected = now_before - chrono::Duration::days(1);
            assert!((older_than - expected).num_seconds().abs() < 5);
        }

        // 2. Custom TTL with with_pending_ttl
        let custom_ttl = chrono::Duration::hours(2);
        let custom_worker =
            MediaCleanupWorker::new(repo.clone(), storage.clone()).with_pending_ttl(custom_ttl);
        assert_eq!(custom_worker.pending_ttl(), custom_ttl);

        let now_custom = Utc::now();
        custom_worker.process_pending_purges().await.unwrap();

        {
            let args_custom = repo.fetched_stale_args.lock().unwrap();
            assert_eq!(args_custom.len(), 2);
            let (older_than_custom, _) = args_custom[1];
            let expected_custom = now_custom - custom_ttl;
            assert!((older_than_custom - expected_custom).num_seconds().abs() < 5);
        }
    }

    #[tokio::test]
    async fn test_cleanup_worker_self_heals_pending_media_used_in_active_packs() {
        let repo = Arc::new(MockMediaRepository::new(Vec::new()));
        let storage = Arc::new(MockFileStorage::new(Ok(())));

        let sample_stale_asset = MediaAsset {
            id: 301,
            owner_user_id: Uuid::new_v4().to_string(),
            provider: MediaProvider::HackClubCdn,
            provider_file_id: "in_use_file_301".to_string(),
            url: "https://cdn.example.com/in_use_301.png".to_string(),
            filename: "in_use.png".to_string(),
            content_type: "image/png".to_string(),
            size_bytes: 1024,
            status: MediaStatus::Pending,
            visibility: MediaVisibility::Private,
            created_at: Utc::now() - chrono::Duration::days(2),
        };
        repo.stale_pending.lock().unwrap().push(sample_stale_asset);

        // Mark that this media IS used in an active pack!
        *repo.is_used.lock().unwrap() = true;

        let worker = MediaCleanupWorker::new(repo.clone(), storage.clone());
        let processed = worker.process_pending_purges().await.unwrap();

        // Must NOT delete from CDN
        assert!(storage.deleted_file_ids.lock().unwrap().is_empty());
        // Must NOT delete from DB
        assert!(repo.deleted_media_ids.lock().unwrap().is_empty());
        // Must self-heal to 'attached'
        assert_eq!(*repo.marked_attached_ids.lock().unwrap(), vec![301]);
        assert_eq!(processed, 0);
    }
}
