use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::{CreateMediaAsset, FileStorage, MediaAsset, MediaRepository, UploadFile},
};

pub struct UploadMediaCommand {
    storage: Arc<dyn FileStorage>,
    media_repository: Arc<dyn MediaRepository>,
}

impl UploadMediaCommand {
    pub fn new(storage: Arc<dyn FileStorage>, media_repository: Arc<dyn MediaRepository>) -> Self {
        Self {
            storage,
            media_repository,
        }
    }

    pub async fn execute(
        &self,
        owner_user_id: String,
        file: UploadFile,
    ) -> Result<MediaAsset, AppError> {
        let stored_file = self.storage.upload(file).await?;

        match self
            .media_repository
            .create(CreateMediaAsset {
                owner_user_id,
                stored_file: stored_file.clone(),
            })
            .await
        {
            Ok(asset) => Ok(asset),
            Err(err) => {
                let _ = self.storage.delete(&stored_file.provider_file_id).await;
                Err(err)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::media::{
        MediaProvider, MediaPurgeTask, MediaStatus, MediaVisibility, StoredFile,
    };
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use std::sync::Mutex;

    struct MockFileStorage {
        deleted_ids: Mutex<Vec<String>>,
    }

    #[async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, file: UploadFile) -> Result<StoredFile, AppError> {
            Ok(StoredFile {
                provider: MediaProvider::HackClubCdn,
                provider_file_id: "test_uploaded_file_id".to_string(),
                url: format!("https://cdn.example.com/{}", file.filename),
                filename: file.filename,
                content_type: file.content_type,
                size_bytes: file.bytes.len() as i64,
            })
        }
        async fn upload_from_url(&self, _url: &str) -> Result<StoredFile, AppError> {
            unimplemented!()
        }
        async fn delete(&self, provider_file_id: &str) -> Result<(), AppError> {
            self.deleted_ids
                .lock()
                .unwrap()
                .push(provider_file_id.to_string());
            Ok(())
        }
    }

    struct MockMediaRepo {
        should_fail: bool,
    }

    #[async_trait]
    impl MediaRepository for MockMediaRepo {
        async fn create(&self, asset: CreateMediaAsset) -> Result<MediaAsset, AppError> {
            if self.should_fail {
                return Err(AppError::ProviderError("DB failure".to_string()));
            }
            Ok(MediaAsset {
                id: 10,
                owner_user_id: asset.owner_user_id,
                provider: asset.stored_file.provider,
                provider_file_id: asset.stored_file.provider_file_id,
                url: asset.stored_file.url,
                filename: asset.stored_file.filename,
                content_type: asset.stored_file.content_type,
                size_bytes: asset.stored_file.size_bytes,
                status: MediaStatus::Pending,
                visibility: MediaVisibility::Private,
                created_at: Utc::now(),
            })
        }
        async fn find_by_id(&self, _id: i64) -> Result<Option<MediaAsset>, AppError> {
            unimplemented!()
        }
        async fn find_by_id_for_owner(
            &self,
            _id: i64,
            _owner_user_id: &str,
        ) -> Result<Option<MediaAsset>, AppError> {
            unimplemented!()
        }
        async fn list_by_owner(&self, _owner_user_id: &str) -> Result<Vec<MediaAsset>, AppError> {
            unimplemented!()
        }
        async fn delete_by_id_for_owner(
            &self,
            _id: i64,
            _owner_user_id: &str,
        ) -> Result<bool, AppError> {
            unimplemented!()
        }
        async fn delete_by_id(&self, _id: i64) -> Result<bool, AppError> {
            unimplemented!()
        }
        async fn provider_exists(
            &self,
            _provider: MediaProvider,
            _provider_file_id: &str,
        ) -> Result<bool, AppError> {
            unimplemented!()
        }
        async fn mark_attached(&self, _ids: &[i64]) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn validate_exists(&self, _ids: &[i64]) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn validate_ownership(
            &self,
            _ids: &[i64],
            _owner_user_id: &str,
        ) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn enqueue_purge_tasks_for_media_ids(
            &self,
            _media_ids: &[i64],
        ) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn fetch_stale_pending_media(
            &self,
            _older_than: DateTime<Utc>,
            _limit: i64,
        ) -> Result<Vec<MediaAsset>, AppError> {
            unimplemented!()
        }
        async fn fetch_pending_purge_tasks(
            &self,
            _limit: i64,
        ) -> Result<Vec<MediaPurgeTask>, AppError> {
            unimplemented!()
        }
        async fn complete_purge_task(&self, _task_id: uuid::Uuid) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn fail_purge_task(
            &self,
            _task_id: uuid::Uuid,
            _error: &str,
            _next_retry_at: DateTime<Utc>,
        ) -> Result<(), AppError> {
            unimplemented!()
        }
        async fn is_media_used_in_active_packs(
            &self,
            _media_asset_id: i64,
        ) -> Result<bool, AppError> {
            unimplemented!()
        }
        async fn is_media_locked_in_games(&self, _media_asset_id: i64) -> Result<bool, AppError> {
            unimplemented!()
        }
    }

    #[tokio::test]
    async fn test_upload_media_success() {
        let storage = Arc::new(MockFileStorage {
            deleted_ids: Mutex::new(Vec::new()),
        });
        let repo = Arc::new(MockMediaRepo { should_fail: false });
        let cmd = UploadMediaCommand::new(storage.clone(), repo);
        let res = cmd
            .execute(
                "u1".to_string(),
                UploadFile {
                    filename: "test.png".to_string(),
                    content_type: "image/png".to_string(),
                    bytes: vec![1, 2, 3, 4],
                },
            )
            .await
            .unwrap();

        assert_eq!(res.id, 10);
        assert!(storage.deleted_ids.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_upload_media_rolls_back_cdn_on_repo_error() {
        let storage = Arc::new(MockFileStorage {
            deleted_ids: Mutex::new(Vec::new()),
        });
        let repo = Arc::new(MockMediaRepo { should_fail: true });
        let cmd = UploadMediaCommand::new(storage.clone(), repo);
        let res = cmd
            .execute(
                "u1".to_string(),
                UploadFile {
                    filename: "test.png".to_string(),
                    content_type: "image/png".to_string(),
                    bytes: vec![1, 2, 3, 4],
                },
            )
            .await;

        assert!(res.is_err());
        assert_eq!(
            *storage.deleted_ids.lock().unwrap(),
            vec!["test_uploaded_file_id"]
        );
    }
}
