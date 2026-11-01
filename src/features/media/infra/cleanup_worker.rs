use chrono::Utc;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info};

use crate::common::http::error::AppError;
use crate::features::media::{FileStorage, MediaRepository};

pub struct MediaCleanupWorker {
    media_repo: Arc<dyn MediaRepository>,
    file_storage: Arc<dyn FileStorage>,
}

impl MediaCleanupWorker {
    pub fn new(media_repo: Arc<dyn MediaRepository>, file_storage: Arc<dyn FileStorage>) -> Self {
        Self {
            media_repo,
            file_storage,
        }
    }

    pub fn start(
        self: Arc<Self>,
        mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
    ) -> tokio::task::JoinHandle<()> {
        info!("Starting media cleanup background worker");
        tokio::spawn(async move {
            loop {
                if *shutdown_rx.borrow() {
                    info!("Shutdown signal received, stopping media cleanup worker");
                    break;
                }

                if let Err(err) = self.process_pending_purges().await {
                    error!("Error in media cleanup worker iteration: {:?}", err);
                }

                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        info!("Shutdown signal received during sleep, stopping media cleanup worker");
                        break;
                    }
                    _ = tokio::time::sleep(Duration::from_secs(10)) => {}
                }
            }
        })
    }

    pub async fn process_pending_purges(&self) -> Result<(), AppError> {
        let tasks = self.media_repo.fetch_pending_purge_tasks(20).await?;
        if tasks.is_empty() {
            return Ok(());
        }

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
                    media_asset_id = task.media_asset_id,
                    "Media is still in use by another active pack. Canceling purge task."
                );
                self.media_repo.complete_purge_task(task.id).await?;
                continue;
            }

            // 3. Purge from CDN
            let delete_result = self.file_storage.delete(&task.provider_file_id).await;
            match delete_result {
                Ok(()) | Err(AppError::NotFound(_)) => {
                    info!(
                        media_asset_id = task.media_asset_id,
                        file_id = %task.provider_file_id,
                        "Successfully deleted file from CDN, removing media asset record"
                    );
                    self.media_repo.delete_by_id(task.media_asset_id).await?;
                    self.media_repo.complete_purge_task(task.id).await?;
                }
                Err(err) => {
                    error!(
                        media_asset_id = task.media_asset_id,
                        file_id = %task.provider_file_id,
                        "Failed to delete file from CDN: {:?}", err
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

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::media::{
        CreateMediaAsset, MediaAsset, MediaProvider, MediaPurgeTask, StoredFile, UploadFile,
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
        async fn mark_attached(&self, _ids: &[i64]) -> Result<(), AppError> {
            todo!()
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
}
