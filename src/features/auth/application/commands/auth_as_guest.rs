use std::sync::Arc;

use uuid::Uuid;

use crate::{
    common::{
        http::{error::AppError, role::Role},
        security::jwt::AuthBody,
    },
    features::auth::{
        application::commands::session_tokens::issue_tokens_with_family, AuthRepository,
    },
};

pub struct AuthAsGuestCommand {
    repo: Arc<dyn AuthRepository>,
}

impl AuthAsGuestCommand {
    pub fn new(repo: Arc<dyn AuthRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self) -> Result<AuthBody, AppError> {
        let user_id = self
            .repo
            .create_user_with_auth(None, None)
            .await?;

        issue_tokens_with_family(&self.repo, user_id, Uuid::new_v4(), Role::User).await
    }
}
