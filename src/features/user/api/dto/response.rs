use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::features::user::UserProfile;

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UserDto {
    pub id: String,
    pub username: String,
    #[serde(with = "crate::common::serde::ts_format::option")]
    pub created_at: Option<DateTime<Utc>>,
    #[serde(with = "crate::common::serde::ts_format::option")]
    pub modified_at: Option<DateTime<Utc>>,
}

impl From<UserProfile> for UserDto {
    fn from(user: UserProfile) -> Self {
        Self {
            id: user.id,
            username: user.username,
            created_at: user.created_at,
            modified_at: user.modified_at,
        }
    }
}
