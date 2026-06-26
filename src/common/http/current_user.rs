use axum::{extract::FromRequestParts, http::request::Parts};

use crate::common::http::{error::AppError, role::Role};

#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub user_id: String,
    pub role: Role,
}

impl<S> FromRequestParts<S> for CurrentUser
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        parts
            .extensions
            .get::<CurrentUser>()
            .cloned()
            .ok_or(AppError::InvalidToken)
    }
}
