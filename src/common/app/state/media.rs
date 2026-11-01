use std::sync::Arc;

use crate::features::media::{
    GetMediaAssetUrlQuery, ImportTelegramStickersCommand, MarkMediaAttachedCommand,
    MediaCleanupWorker, UploadMediaCommand,
};

#[derive(Clone)]
pub struct MediaState {
    pub upload_media: Arc<UploadMediaCommand>,
    pub import_telegram_stickers: Arc<ImportTelegramStickersCommand>,
    pub get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
    pub mark_media_attached: Arc<MarkMediaAttachedCommand>,
    pub cleanup_worker: Arc<MediaCleanupWorker>,
    pub purge_worker: Arc<MediaCleanupWorker>,
    pub max_file_size_bytes: usize,
}

impl MediaState {
    pub fn new(
        upload_media: Arc<UploadMediaCommand>,
        import_telegram_stickers: Arc<ImportTelegramStickersCommand>,
        get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
        mark_media_attached: Arc<MarkMediaAttachedCommand>,
        cleanup_worker: Arc<MediaCleanupWorker>,
        max_file_size_bytes: usize,
    ) -> Self {
        Self {
            upload_media,
            import_telegram_stickers,
            get_media_asset_url,
            mark_media_attached,
            purge_worker: cleanup_worker.clone(),
            cleanup_worker,
            max_file_size_bytes,
        }
    }
}
