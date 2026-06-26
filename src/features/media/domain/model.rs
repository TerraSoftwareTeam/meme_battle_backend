use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaProvider {
    HackClubCdn,
}

impl MediaProvider {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::HackClubCdn => "hackclub_cdn",
        }
    }
}

impl TryFrom<String> for MediaProvider {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "hackclub_cdn" => Ok(Self::HackClubCdn),
            _ => Err(format!("Unsupported media provider: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaStatus {
    Pending,
    Attached,
    Deleted,
}

impl TryFrom<String> for MediaStatus {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "pending" => Ok(Self::Pending),
            "attached" => Ok(Self::Attached),
            "deleted" => Ok(Self::Deleted),
            _ => Err(format!("Unsupported media status: {value}")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum MediaVisibility {
    Public,
    Private,
}

impl TryFrom<String> for MediaVisibility {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        match value.as_str() {
            "public" => Ok(Self::Public),
            "private" => Ok(Self::Private),
            _ => Err(format!("Unsupported media visibility: {value}")),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MediaAsset {
    pub id: i64,
    pub owner_user_id: String,
    pub provider: MediaProvider,
    pub provider_file_id: String,
    pub url: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
    pub status: MediaStatus,
    pub visibility: MediaVisibility,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct UploadFile {
    pub filename: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    pub provider: MediaProvider,
    pub provider_file_id: String,
    pub url: String,
    pub filename: String,
    pub content_type: String,
    pub size_bytes: i64,
}
