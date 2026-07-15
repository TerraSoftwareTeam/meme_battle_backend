use std::sync::Arc;

use crate::{
    common::{http::error::AppError, security::hash_util},
    features::auth::{AuthRepository, RegisterUser},
};

pub struct RegisterUserCommand {
    repo: Arc<dyn AuthRepository>,
}

impl RegisterUserCommand {
    pub fn new(repo: Arc<dyn AuthRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, input: RegisterUser) -> Result<(), AppError> {
        let password_hash = match input.password {
            Some(ref password) if !password.is_empty() => {
                Some(hash_util::hash_password(password).map_err(|_| AppError::InternalError)?)
            }
            _ => None,
        };

        self.repo
            .create_user_with_auth(Some(input.username), password_hash)
            .await?;

        Ok(())
    }
}
