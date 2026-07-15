use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::user::domain::model::{SearchUser, UpdateUserProfile, User},
};

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn find_all(&self) -> Result<Vec<User>, AppError>;

    async fn find_by_id(&self, id: &str) -> Result<Option<User>, AppError>;

    async fn find_list(&self, search: SearchUser) -> Result<Vec<User>, AppError>;

    async fn update_profile(
        &self,
        id: &str,
        update: UpdateUserProfile,
    ) -> Result<Option<User>, AppError>;

    async fn promote_to_admin(&self, id: &str) -> Result<Option<User>, AppError>;
}
