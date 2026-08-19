use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::user::{
        application::ports::UserAuthService, UpdateMeInput, UpdateUserProfile, UserProfile,
        UserRepository,
    },
};

pub struct UpdateMeCommand {
    repo: Arc<dyn UserRepository>,
    user_auth_service: Arc<dyn UserAuthService>,
}

impl UpdateMeCommand {
    pub fn new(
        repo: Arc<dyn UserRepository>,
        user_auth_service: Arc<dyn UserAuthService>,
    ) -> Self {
        Self {
            repo,
            user_auth_service,
        }
    }

    pub async fn execute(
        &self,
        user_id: String,
        input: UpdateMeInput,
    ) -> Result<UserProfile, AppError> {
        let username = normalize_optional_field(input.username)?;
        let password = normalize_optional_field(input.password)?;

        if username.is_none() && password.is_none() {
            return Err(AppError::ValidationError(
                "At least one field (username or password) must be provided".into(),
            ));
        }

        if let Some(ref new_password) = password {
            self.user_auth_service
                .change_password(&user_id, new_password)
                .await?;
        }

        let user = if let Some(username) = username {
            self.repo
                .update_profile(&user_id, UpdateUserProfile { username: Some(username) })
                .await?
                .ok_or_else(|| AppError::NotFound("User not found".into()))?
        } else {
            self.repo
                .find_by_id(&user_id)
                .await?
                .ok_or_else(|| AppError::NotFound("User not found".into()))?
        };

        let is_guest = self.user_auth_service.is_guest(&user_id).await?;

        Ok(UserProfile::new(user, is_guest))
    }
}

fn normalize_optional_field(field: Option<String>) -> Result<Option<String>, AppError> {
    match field {
        Some(f) => {
            let trimmed = f.trim().to_string();
            if trimmed.is_empty() {
                Err(AppError::ValidationError("Field cannot be empty".into()))
            } else {
                Ok(Some(trimmed))
            }
        }
        None => Ok(None),
    }
}
