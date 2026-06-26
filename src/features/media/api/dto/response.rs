use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::features::media::{MediaAsset, MediaProvider, MediaStatus};

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
