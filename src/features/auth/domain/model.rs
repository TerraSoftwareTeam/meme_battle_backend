use serde::{Deserialize, Serialize};

use crate::common::http::role::Role;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserAuth {
    pub user_id: String,
    pub password_hash: Option<String>,
    pub role: Role,
}

#[derive(Debug, Clone)]
pub struct RefreshTokenRecord {
    pub user_id: String,
    pub family_id: uuid::Uuid,
    pub is_used: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterUser {
    pub username: String,
    pub handle: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginUser {
    pub handle: String,
    pub password: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefreshSession {
    pub refresh_token: String,
}
