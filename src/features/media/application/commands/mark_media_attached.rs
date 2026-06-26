use std::sync::Arc;

use crate::{
    common::http::error::AppError,
    features::media::domain::ports::media_repository::MediaRepository,
};

pub struct MarkMediaAttachedCommand {
    media_repository: Arc<dyn MediaRepository>,
}

impl MarkMediaAttachedCommand {
    pub fn new(media_repository: Arc<dyn MediaRepository>) -> Self {
        Self { media_repository }
    }

    pub async fn execute(&self, ids: &[i64]) -> Result<(), AppError> {
        self.media_repository.mark_attached(ids).await
    }
}
