use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::media::domain::telegram::{DownloadedSticker, TelegramStickerSet},
};

#[async_trait]
pub trait TelegramStickerClient: Send + Sync {
    async fn get_sticker_set(&self, set_name: &str) -> Result<TelegramStickerSet, AppError>;
    async fn download_sticker_file(&self, file_id: &str) -> Result<DownloadedSticker, AppError>;
}
