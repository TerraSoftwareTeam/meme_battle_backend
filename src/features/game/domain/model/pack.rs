use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, utoipa::ToSchema,
)]
#[sqlx(type_name = "content_safety_level", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ContentSafetyLevel {
    FamilyFriendly,
    Spicy,
    Explicit,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemePack {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PackMeme {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub media_id: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PackMemeDetails {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub media_id: Option<i64>,
    pub media_url: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SituationPack {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PackSituation {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub prompt_text: String,
}
