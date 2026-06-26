use std::sync::Arc;

use chrono::Utc;

use crate::{
    common::{
        http::{error::AppError, role::Role},
        security::{
            hash_util,
            jwt::{generate_random_string, make_jwt_token, AuthBody},
        },
    },
    features::auth::AuthRepository,
};

pub(super) async fn issue_tokens_with_family(
    repo: &Arc<dyn AuthRepository>,
    user_id: String,
    family_id: uuid::Uuid,
    role: Role,
) -> Result<AuthBody, AppError> {
    let access_token = make_jwt_token(&user_id, &role).map_err(|_| AppError::InternalError)?;
    let refresh_token = generate_random_string();
    let token_hash =
        hash_util::hash_refresh_token(&refresh_token).map_err(|_| AppError::InternalError)?;
    let expires_at = Utc::now() + chrono::Duration::days(30);

    repo.save_refresh_token(user_id, token_hash, family_id, expires_at)
        .await?;

    Ok(AuthBody::new(access_token, refresh_token))
}
