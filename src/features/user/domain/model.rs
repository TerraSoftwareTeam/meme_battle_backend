use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct User {
    pub id: String,
    pub username: String,
    pub handle: String,
    pub avatar_media_asset_id: Option<i64>,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SearchUser {
    pub id: Option<String>,
    pub username: Option<String>,
    pub handle: Option<String>,
}

#[derive(Debug, Clone)]
pub struct UpdateUserProfile {
    pub username: Option<String>,
    pub handle: Option<String>,
}
