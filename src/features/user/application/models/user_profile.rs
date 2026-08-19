use chrono::{DateTime, Utc};

use crate::features::user::User;

#[derive(Debug, Clone)]
pub struct UserProfile {
    pub id: String,
    pub username: String,
    pub created_at: Option<DateTime<Utc>>,
    pub modified_at: Option<DateTime<Utc>>,
    pub is_guest: bool,
}

impl UserProfile {
    pub fn new(user: User, is_guest: bool) -> Self {
        let username = user
            .username
            .unwrap_or_else(|| format!("player-{}", user.id));
        Self {
            id: user.id,
            username,
            created_at: user.created_at,
            modified_at: user.modified_at,
            is_guest,
        }
    }
}
