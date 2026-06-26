use async_trait::async_trait;

use crate::{
    common::http::error::AppError,
    features::media::{StoredFile, UploadFile},
};

#[async_trait]
pub trait FileStorage: Send + Sync {
    async fn upload(&self, file: UploadFile) -> Result<StoredFile, AppError>;

    async fn upload_from_url(&self, url: &str) -> Result<StoredFile, AppError>;

    async fn delete(&self, provider_file_id: &str) -> Result<(), AppError>;
}
