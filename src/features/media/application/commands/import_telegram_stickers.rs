use futures_util::stream::{self, StreamExt};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::{
    common::http::error::AppError,
    features::media::{
        domain::{
            ports::telegram_client::TelegramStickerClient, telegram::parse_telegram_pack_name,
        },
        CreateMediaAsset, FileStorage, MediaRepository, UploadFile,
    },
};

#[derive(Debug, Clone)]
pub struct ImportTelegramStickersResult {
    pub name: String,
    pub title: String,
    pub total_imported: usize,
    pub skipped_animated: usize,
    pub stickers: Vec<ImportedStickerItem>,
}

#[derive(Debug, Clone)]
pub struct ImportedStickerItem {
    pub media_id: i64,
    pub url: String,
    pub emoji: Option<String>,
    pub width: i32,
    pub height: i32,
}

pub struct ImportTelegramStickersCommand {
    telegram_client: Arc<dyn TelegramStickerClient>,
    file_storage: Arc<dyn FileStorage>,
    media_repository: Arc<dyn MediaRepository>,
    max_file_size_bytes: usize,
}

impl ImportTelegramStickersCommand {
    pub fn new(
        telegram_client: Arc<dyn TelegramStickerClient>,
        file_storage: Arc<dyn FileStorage>,
        media_repository: Arc<dyn MediaRepository>,
        max_file_size_bytes: usize,
    ) -> Self {
        Self {
            telegram_client,
            file_storage,
            media_repository,
            max_file_size_bytes,
        }
    }

    pub async fn execute(
        &self,
        owner_user_id: String,
        input_url_or_name: String,
    ) -> Result<ImportTelegramStickersResult, AppError> {
        let pack_name = parse_telegram_pack_name(&input_url_or_name)?;

        info!(
            pack_name = %pack_name,
            owner_user_id = %owner_user_id,
            "Fetching sticker set from Telegram"
        );

        let sticker_set = self.telegram_client.get_sticker_set(&pack_name).await?;

        let mut skipped_animated = 0;
        let mut static_stickers = Vec::new();

        for s in sticker_set.stickers {
            if s.is_animated || s.is_video {
                skipped_animated += 1;
            } else {
                static_stickers.push(s);
            }
        }

        if static_stickers.is_empty() {
            return Err(AppError::ValidationError(
                "This Telegram sticker set does not contain any static image stickers (animated and video stickers are not supported)"
                    .to_string(),
            ));
        }

        // Limit maximum stickers to 120 per import
        if static_stickers.len() > 120 {
            static_stickers.truncate(120);
        }

        const CONCURRENT_IMPORT_LIMIT: usize = 8;
        let import_tasks = static_stickers.into_iter().map(|sticker| {
            let owner_user_id = owner_user_id.clone();
            async move {
                let downloaded = match self
                    .telegram_client
                    .download_sticker_file(&sticker.file_id)
                    .await
                {
                    Ok(file) => file,
                    Err(err) => {
                        warn!(
                            file_id = %sticker.file_id,
                            error = ?err,
                            "Failed to download sticker from Telegram, skipping"
                        );
                        return None;
                    }
                };

                if downloaded.bytes.is_empty() {
                    return None;
                }

                if downloaded.bytes.len() > self.max_file_size_bytes {
                    warn!(
                        file_id = %sticker.file_id,
                        size = downloaded.bytes.len(),
                        limit = self.max_file_size_bytes,
                        "Sticker exceeds maximum file size, skipping"
                    );
                    return None;
                }

                let upload_file = UploadFile {
                    filename: downloaded.filename,
                    content_type: downloaded.content_type,
                    bytes: downloaded.bytes,
                };

                let stored_file = match self.file_storage.upload(upload_file).await {
                    Ok(stored) => stored,
                    Err(err) => {
                        error!(
                            file_id = %sticker.file_id,
                            error = ?err,
                            "Failed to upload sticker to CDN storage, skipping"
                        );
                        return None;
                    }
                };

                match self
                    .media_repository
                    .create(CreateMediaAsset {
                        owner_user_id,
                        stored_file: stored_file.clone(),
                    })
                    .await
                {
                    Ok(asset) => Some(ImportedStickerItem {
                        media_id: asset.id,
                        url: asset.url,
                        emoji: sticker.emoji,
                        width: sticker.width,
                        height: sticker.height,
                    }),
                    Err(err) => {
                        error!(
                            file_id = %sticker.file_id,
                            error = ?err,
                            "Failed to save sticker media asset to database, rolling back CDN"
                        );
                        if let Err(delete_err) = self
                            .file_storage
                            .delete(&stored_file.provider_file_id)
                            .await
                        {
                            warn!(
                                provider_file_id = %stored_file.provider_file_id,
                                error = ?delete_err,
                                "Failed to delete orphaned file from CDN during rollback"
                            );
                        }
                        None
                    }
                }
            }
        });

        let imported_stickers: Vec<ImportedStickerItem> = stream::iter(import_tasks)
            .buffered(CONCURRENT_IMPORT_LIMIT)
            .filter_map(|res| async move { res })
            .collect()
            .await;

        if imported_stickers.is_empty() {
            return Err(AppError::ProviderError(
                "Failed to import any stickers from the Telegram sticker set".to_string(),
            ));
        }

        let total_imported = imported_stickers.len();

        Ok(ImportTelegramStickersResult {
            name: sticker_set.name,
            title: sticker_set.title,
            total_imported,
            skipped_animated,
            stickers: imported_stickers,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::features::media::{
        domain::{
            ports::telegram_client::TelegramStickerClient,
            telegram::{DownloadedSticker, TelegramSticker, TelegramStickerSet},
        },
        CreateMediaAsset, MediaAsset, MediaProvider, MediaPurgeTask, MediaStatus, MediaVisibility,
        StoredFile,
    };
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use std::{collections::HashMap, sync::Mutex};

    struct MockTelegramClient {
        sticker_set: Mutex<Option<TelegramStickerSet>>,
        error_msg: Mutex<Option<String>>,
        download_results: Mutex<HashMap<String, Result<DownloadedSticker, String>>>,
    }

    impl MockTelegramClient {
        fn new(set: TelegramStickerSet) -> Self {
            Self {
                sticker_set: Mutex::new(Some(set)),
                error_msg: Mutex::new(None),
                download_results: Mutex::new(HashMap::new()),
            }
        }

        fn with_error(err: &str) -> Self {
            Self {
                sticker_set: Mutex::new(None),
                error_msg: Mutex::new(Some(err.to_string())),
                download_results: Mutex::new(HashMap::new()),
            }
        }

        fn add_download(&self, file_id: &str, result: Result<DownloadedSticker, String>) {
            self.download_results
                .lock()
                .unwrap()
                .insert(file_id.to_string(), result);
        }
    }

    #[async_trait]
    impl TelegramStickerClient for MockTelegramClient {
        async fn get_sticker_set(&self, _name: &str) -> Result<TelegramStickerSet, AppError> {
            if let Some(err) = self.error_msg.lock().unwrap().as_ref() {
                return Err(AppError::ProviderError(err.clone()));
            }
            Ok(self.sticker_set.lock().unwrap().clone().unwrap())
        }

        async fn download_sticker_file(
            &self,
            file_id: &str,
        ) -> Result<DownloadedSticker, AppError> {
            let map = self.download_results.lock().unwrap();
            match map.get(file_id) {
                Some(Ok(s)) => Ok(s.clone()),
                Some(Err(e)) => Err(AppError::ProviderError(e.clone())),
                None => Err(AppError::NotFound("File not found".to_string())),
            }
        }
    }

    struct MockFileStorage {
        uploaded_files: Mutex<Vec<UploadFile>>,
        deleted_file_ids: Mutex<Vec<String>>,
        upload_fail: Mutex<bool>,
    }

    impl MockFileStorage {
        fn new() -> Self {
            Self {
                uploaded_files: Mutex::new(Vec::new()),
                deleted_file_ids: Mutex::new(Vec::new()),
                upload_fail: Mutex::new(false),
            }
        }
    }

    #[async_trait]
    impl FileStorage for MockFileStorage {
        async fn upload(&self, file: UploadFile) -> Result<StoredFile, AppError> {
            if *self.upload_fail.lock().unwrap() {
                return Err(AppError::ProviderError("Upload to CDN failed".to_string()));
            }
            let provider_file_id = format!("cdn_{}", file.filename);
            let url = format!("https://cdn.example.com/{}", provider_file_id);
            let size_bytes = file.bytes.len() as i64;
            self.uploaded_files.lock().unwrap().push(file.clone());

            Ok(StoredFile {
                provider: MediaProvider::HackClubCdn,
                provider_file_id,
                url,
                filename: file.filename,
                content_type: file.content_type,
                size_bytes,
            })
        }

        async fn upload_from_url(&self, _url: &str) -> Result<StoredFile, AppError> {
            todo!()
        }

        async fn delete(&self, provider_file_id: &str) -> Result<(), AppError> {
            self.deleted_file_ids
                .lock()
                .unwrap()
                .push(provider_file_id.to_string());
            Ok(())
        }
    }

    struct MockMediaRepository {
        created_assets: Mutex<Vec<MediaAsset>>,
        fail_create: Mutex<bool>,
    }

    impl MockMediaRepository {
        fn new() -> Self {
            Self {
                created_assets: Mutex::new(Vec::new()),
                fail_create: Mutex::new(false),
            }
        }
    }

    #[async_trait]
    impl MediaRepository for MockMediaRepository {
        async fn create(&self, asset: CreateMediaAsset) -> Result<MediaAsset, AppError> {
            if *self.fail_create.lock().unwrap() {
                return Err(AppError::InternalError);
            }
            let mut list = self.created_assets.lock().unwrap();
            let id = (list.len() + 1) as i64;
            let media_asset = MediaAsset {
                id,
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
            };
            list.push(media_asset.clone());
            Ok(media_asset)
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
        async fn delete_by_id(&self, _id: i64) -> Result<bool, AppError> {
            todo!()
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
        async fn validate_ownership(
            &self,
            _ids: &[i64],
            _owner_user_id: &str,
        ) -> Result<(), AppError> {
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
            _older_than: DateTime<Utc>,
            _limit: i64,
        ) -> Result<Vec<MediaAsset>, AppError> {
            todo!()
        }
        async fn fetch_pending_purge_tasks(
            &self,
            _limit: i64,
        ) -> Result<Vec<MediaPurgeTask>, AppError> {
            todo!()
        }
        async fn complete_purge_task(&self, _task_id: uuid::Uuid) -> Result<(), AppError> {
            todo!()
        }
        async fn fail_purge_task(
            &self,
            _task_id: uuid::Uuid,
            _error: &str,
            _next_retry_at: DateTime<Utc>,
        ) -> Result<(), AppError> {
            todo!()
        }
        async fn is_media_used_in_active_packs(
            &self,
            _media_asset_id: i64,
        ) -> Result<bool, AppError> {
            todo!()
        }
        async fn is_media_locked_in_games(&self, _media_asset_id: i64) -> Result<bool, AppError> {
            todo!()
        }
    }

    #[tokio::test]
    async fn test_import_telegram_stickers_success() {
        let sticker1 = TelegramSticker {
            file_id: "file_1".to_string(),
            file_unique_id: "uniq_1".to_string(),
            width: 512,
            height: 512,
            is_animated: false,
            is_video: false,
            emoji: Some("🐱".to_string()),
        };
        let sticker_animated = TelegramSticker {
            file_id: "file_anim".to_string(),
            file_unique_id: "uniq_anim".to_string(),
            width: 512,
            height: 512,
            is_animated: true,
            is_video: false,
            emoji: Some("✨".to_string()),
        };
        let sticker2 = TelegramSticker {
            file_id: "file_2".to_string(),
            file_unique_id: "uniq_2".to_string(),
            width: 512,
            height: 384,
            is_animated: false,
            is_video: false,
            emoji: Some("🐶".to_string()),
        };

        let set = TelegramStickerSet {
            name: "animals_pack".to_string(),
            title: "Animals".to_string(),
            stickers: vec![sticker1, sticker_animated, sticker2],
        };

        let tg_client = Arc::new(MockTelegramClient::new(set));
        tg_client.add_download(
            "file_1",
            Ok(DownloadedSticker {
                filename: "file_1.webp".to_string(),
                content_type: "image/webp".to_string(),
                bytes: vec![1, 2, 3, 4],
            }),
        );
        tg_client.add_download(
            "file_2",
            Ok(DownloadedSticker {
                filename: "file_2.webp".to_string(),
                content_type: "image/webp".to_string(),
                bytes: vec![5, 6, 7, 8],
            }),
        );

        let storage = Arc::new(MockFileStorage::new());
        let repo = Arc::new(MockMediaRepository::new());

        let cmd = ImportTelegramStickersCommand::new(
            tg_client,
            storage.clone(),
            repo.clone(),
            10 * 1024 * 1024,
        );

        let result = cmd
            .execute(
                "user_123".to_string(),
                "https://t.me/addstickers/animals_pack".to_string(),
            )
            .await
            .unwrap();

        assert_eq!(result.name, "animals_pack");
        assert_eq!(result.title, "Animals");
        assert_eq!(result.total_imported, 2);
        assert_eq!(result.skipped_animated, 1);
        assert_eq!(result.stickers.len(), 2);

        assert_eq!(result.stickers[0].media_id, 1);
        assert_eq!(result.stickers[0].emoji.as_deref(), Some("🐱"));
        assert_eq!(result.stickers[0].width, 512);
        assert_eq!(result.stickers[0].height, 512);

        assert_eq!(result.stickers[1].media_id, 2);
        assert_eq!(result.stickers[1].emoji.as_deref(), Some("🐶"));
        assert_eq!(result.stickers[1].width, 512);
        assert_eq!(result.stickers[1].height, 384);

        // Check storage and repo
        let uploaded = storage.uploaded_files.lock().unwrap();
        assert_eq!(uploaded.len(), 2);
        let created = repo.created_assets.lock().unwrap();
        assert_eq!(created.len(), 2);
        assert_eq!(created[0].status, MediaStatus::Pending);
        assert_eq!(created[0].owner_user_id, "user_123");
    }

    #[tokio::test]
    async fn test_import_telegram_stickers_all_animated_fails() {
        let set = TelegramStickerSet {
            name: "anim_pack".to_string(),
            title: "Animated Only".to_string(),
            stickers: vec![TelegramSticker {
                file_id: "anim_1".to_string(),
                file_unique_id: "u1".to_string(),
                width: 512,
                height: 512,
                is_animated: true,
                is_video: false,
                emoji: None,
            }],
        };

        let tg_client = Arc::new(MockTelegramClient::new(set));
        let storage = Arc::new(MockFileStorage::new());
        let repo = Arc::new(MockMediaRepository::new());

        let cmd = ImportTelegramStickersCommand::new(tg_client, storage, repo, 10 * 1024 * 1024);

        let err = cmd
            .execute("user_123".to_string(), "anim_pack".to_string())
            .await
            .unwrap_err();

        match err {
            AppError::ValidationError(msg) => {
                assert!(msg.contains("does not contain any static image stickers"));
            }
            other => panic!("Expected ValidationError, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_import_telegram_stickers_skips_oversized_and_partial_failures() {
        let sticker_normal = TelegramSticker {
            file_id: "normal".to_string(),
            file_unique_id: "u_norm".to_string(),
            width: 512,
            height: 512,
            is_animated: false,
            is_video: false,
            emoji: None,
        };
        let sticker_huge = TelegramSticker {
            file_id: "huge".to_string(),
            file_unique_id: "u_huge".to_string(),
            width: 512,
            height: 512,
            is_animated: false,
            is_video: false,
            emoji: None,
        };
        let sticker_broken = TelegramSticker {
            file_id: "broken".to_string(),
            file_unique_id: "u_broken".to_string(),
            width: 512,
            height: 512,
            is_animated: false,
            is_video: false,
            emoji: None,
        };

        let set = TelegramStickerSet {
            name: "mixed_pack".to_string(),
            title: "Mixed".to_string(),
            stickers: vec![sticker_normal, sticker_huge, sticker_broken],
        };

        let tg_client = Arc::new(MockTelegramClient::new(set));
        tg_client.add_download(
            "normal",
            Ok(DownloadedSticker {
                filename: "normal.webp".to_string(),
                content_type: "image/webp".to_string(),
                bytes: vec![1, 2, 3],
            }),
        );
        tg_client.add_download(
            "huge",
            Ok(DownloadedSticker {
                filename: "huge.webp".to_string(),
                content_type: "image/webp".to_string(),
                bytes: vec![0; 1000],
            }),
        );
        tg_client.add_download("broken", Err("Download failed".to_string()));

        let storage = Arc::new(MockFileStorage::new());
        let repo = Arc::new(MockMediaRepository::new());

        // Max file size is 500 bytes -> "huge" will be skipped
        let cmd = ImportTelegramStickersCommand::new(tg_client, storage.clone(), repo.clone(), 500);

        let result = cmd
            .execute("user_123".to_string(), "mixed_pack".to_string())
            .await
            .unwrap();

        assert_eq!(result.total_imported, 1);
        assert_eq!(storage.uploaded_files.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn test_import_telegram_stickers_rolls_back_cdn_if_db_save_fails() {
        let sticker = TelegramSticker {
            file_id: "f1".to_string(),
            file_unique_id: "u1".to_string(),
            width: 512,
            height: 512,
            is_animated: false,
            is_video: false,
            emoji: None,
        };

        let set = TelegramStickerSet {
            name: "test_pack".to_string(),
            title: "Test".to_string(),
            stickers: vec![sticker],
        };

        let tg_client = Arc::new(MockTelegramClient::new(set));
        tg_client.add_download(
            "f1",
            Ok(DownloadedSticker {
                filename: "f1.webp".to_string(),
                content_type: "image/webp".to_string(),
                bytes: vec![1, 2, 3],
            }),
        );

        let storage = Arc::new(MockFileStorage::new());
        let repo = Arc::new(MockMediaRepository::new());
        // Simulate DB create failure
        *repo.fail_create.lock().unwrap() = true;

        let cmd = ImportTelegramStickersCommand::new(tg_client, storage.clone(), repo, 1024);

        let err = cmd
            .execute("user_123".to_string(), "test_pack".to_string())
            .await
            .unwrap_err();

        match err {
            AppError::ProviderError(msg) => {
                assert!(msg.contains("Failed to import any stickers"));
            }
            other => panic!("Expected ProviderError, got {other:?}"),
        }

        // Verify CDN delete was called for rollback
        let deleted = storage.deleted_file_ids.lock().unwrap();
        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0], "cdn_f1.webp");
    }

    #[tokio::test]
    async fn test_import_telegram_stickers_invalid_pack_name() {
        let tg_client = Arc::new(MockTelegramClient::with_error("pack not found"));
        let storage = Arc::new(MockFileStorage::new());
        let repo = Arc::new(MockMediaRepository::new());

        let cmd = ImportTelegramStickersCommand::new(tg_client, storage, repo, 1024);

        let err = cmd
            .execute(
                "user_123".to_string(),
                "https://invalid.com/pack".to_string(),
            )
            .await
            .unwrap_err();

        match err {
            AppError::ValidationError(msg) => {
                assert!(msg.contains("Invalid Telegram sticker link"));
            }
            other => panic!("Expected ValidationError, got {other:?}"),
        }
    }
}
