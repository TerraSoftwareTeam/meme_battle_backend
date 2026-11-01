use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::features::media::{
    ImportTelegramStickersResult, ImportedStickerItem, MediaAsset, MediaProvider, MediaStatus,
};

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct MediaAssetDto {
    pub id: i64,
    pub owner_user_id: String,
    pub provider: MediaProvider,
    pub provider_file_id: String,
    pub url: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: MediaStatus,
    #[serde(with = "crate::common::serde::ts_format")]
    pub created_at: DateTime<Utc>,
}

impl From<MediaAsset> for MediaAssetDto {
    fn from(asset: MediaAsset) -> Self {
        Self {
            id: asset.id,
            owner_user_id: asset.owner_user_id,
            provider: asset.provider,
            provider_file_id: asset.provider_file_id,
            url: asset.url,
            filename: asset.filename,
            content_type: asset.content_type,
            size_bytes: asset.size_bytes,
            status: asset.status,
            created_at: asset.created_at,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportTelegramStickersResponseDto {
    pub name: String,
    pub title: String,
    pub total_imported: usize,
    pub skipped_animated: usize,
    pub stickers: Vec<ImportedStickerDto>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct ImportedStickerDto {
    pub media_id: i64,
    pub url: String,
    pub emoji: Option<String>,
    pub width: i32,
    pub height: i32,
}

impl From<ImportTelegramStickersResult> for ImportTelegramStickersResponseDto {
    fn from(res: ImportTelegramStickersResult) -> Self {
        Self {
            name: res.name,
            title: res.title,
            total_imported: res.total_imported,
            skipped_animated: res.skipped_animated,
            stickers: res.stickers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ImportedStickerItem> for ImportedStickerDto {
    fn from(item: ImportedStickerItem) -> Self {
        Self {
            media_id: item.media_id,
            url: item.url,
            emoji: item.emoji,
            width: item.width,
            height: item.height,
        }
    }
}
