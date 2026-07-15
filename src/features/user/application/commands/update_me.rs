use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{UpdateUserProfile, UserProfile, UserRepository},
};

pub struct UpdateMeCommand {
    repo: Arc<dyn UserRepository>,
}

impl UpdateMeCommand {
    pub fn new(
        repo: Arc<dyn UserRepository>,
    ) -> Self {
        Self {
            repo,
        }
    }

    pub async fn execute(
        &self,
        user_id: String,
        update: UpdateUserProfile,
    ) -> Result<UserProfile, AppError> {
        let update = normalize_update(update)?;
        let user = self
            .repo
            .update_profile(&user_id, update)
            .await?
            .ok_or_else(|| AppError::NotFound("User not found".into()))?;

        UserProfile::resolve(user).await
    }
}

fn normalize_update(update: UpdateUserProfile) -> Result<UpdateUserProfile, AppError> {
    let username = normalize_optional_field(update.username);

    if username.is_none() {
        return Err(AppError::ValidationError(
            "Username must be provided".into(),
        ));
    }

    Ok(UpdateUserProfile { username })
}

fn normalize_optional_field(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}
