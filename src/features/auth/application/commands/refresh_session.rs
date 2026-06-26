use std::sync::Arc;

use crate::{
    common::{
        http::{error::AppError, role::Role},
        security::{hash_util, jwt::AuthBody},
    },
    features::auth::{
        application::commands::session_tokens::issue_tokens_with_family, AuthRepository,
        RefreshSession,
    },
};

pub struct RefreshSessionCommand {
    repo: Arc<dyn AuthRepository>,
    admin_user_ids: Vec<String>,
}

impl RefreshSessionCommand {
    pub fn new(repo: Arc<dyn AuthRepository>, admin_user_ids: Vec<String>) -> Self {
        Self {
            repo,
            admin_user_ids,
        }
    }

    pub async fn execute(&self, input: RefreshSession) -> Result<AuthBody, AppError> {
        let old_token_hash = hash_util::hash_refresh_token(&input.refresh_token)
            .map_err(|_| AppError::InternalError)?;

        let token = self
            .repo
            .find_refresh_token(&old_token_hash)
            .await?
            .ok_or(AppError::WrongCredentials)?;

        if token.is_used {
            let _ = self.repo.revoke_token_family(token.family_id).await;
            return Err(AppError::WrongCredentials);
        }

        self.repo.mark_token_as_used(&old_token_hash).await?;

        let mut user_auth = self
            .repo
            .find_by_id(&token.user_id)
            .await?
            .ok_or(AppError::UserNotFound)?;

        if self.admin_user_ids.contains(&user_auth.user_id) {
            user_auth.role = Role::Admin;
        }

        issue_tokens_with_family(&self.repo, token.user_id, token.family_id, user_auth.role).await
    }
}
