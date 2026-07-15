use chrono::{DateTime, Utc};

use crate::{
    common::http::error::AppError,
    features::user::User,
};

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
}

impl UserProfile {
    pub async fn resolve(
        user: User,
    ) -> Result<Self, AppError> {
        let username = user.username.unwrap_or_else(|| format!("player-{}", user.id));
        Ok(Self {
            id: user.id,
            username,
            created_at: user.created_at,
            modified_at: user.modified_at,
        })
    }
}
