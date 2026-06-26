use std::sync::Arc;

use crate::{
    common::{
        http::{error::AppError, role::Role},
        security::{hash_util, jwt::AuthBody},
    },
    features::auth::{
        application::commands::session_tokens::issue_tokens_with_family, AuthRepository, LoginUser,
    },
};

pub struct LoginUserCommand {
    repo: Arc<dyn AuthRepository>,
    admin_user_ids: Vec<String>,
}

impl LoginUserCommand {
    pub fn new(repo: Arc<dyn AuthRepository>, admin_user_ids: Vec<String>) -> Self {
        Self {
            repo,
            admin_user_ids,
        }
    }

    pub async fn execute(&self, input: LoginUser) -> Result<AuthBody, AppError> {
        let password = input.password.as_deref();
        let has_valid_password = password.is_some_and(|value| !value.is_empty());

        if input.handle.is_empty() || !has_valid_password {
            return Err(AppError::MissingCredentials);
        }

        let mut user_auth = self
            .repo
            .find_by_handle(&input.handle)
            .await?
            .ok_or(AppError::UserNotFound)?;

        let stored_hash = user_auth
            .password_hash
            .as_deref()
            .ok_or(AppError::WrongCredentials)?;

        if !hash_util::verify_password(stored_hash, password.unwrap()) {
            return Err(AppError::WrongCredentials);
        }

        if self.admin_user_ids.contains(&user_auth.user_id) {
            user_auth.role = Role::Admin;
        }

        issue_tokens_with_family(
            &self.repo,
            user_auth.user_id,
            uuid::Uuid::new_v4(),
            user_auth.role,
        )
        .await
    }
}
