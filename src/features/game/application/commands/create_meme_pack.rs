use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::{
        game::domain::{
            model::ContentSafetyLevel,
            ports::GameRepository,
        },
        media::MarkMediaAttachedCommand,
    },
};

pub struct CreateMemePackCommand {
    repo: Arc<dyn GameRepository>,
    mark_media_attached: Arc<MarkMediaAttachedCommand>,
}

impl CreateMemePackCommand {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        mark_media_attached: Arc<MarkMediaAttachedCommand>,
    ) -> Self {
        Self {
            repo,
            mark_media_attached,
        }
    }

    pub async fn execute(
        &self,
        author_id: Uuid,
        name: String,
        description: Option<String>,
        language_code: String,
        safety_level: ContentSafetyLevel,
        is_public: bool,
        media_ids: Vec<i64>,
    ) -> Result<Uuid, AppError> {
        // Validate that all provided media assets exist before writing anything
        self.repo.validate_media_exists(&media_ids).await?;

        let mut tx = self.repo.begin().await?;

        // 1. Insert the pack
        let pack_id = self.repo
            .insert_meme_pack(
                &mut tx,
                author_id,
                &name,
                description.as_deref(),
                &language_code,
                safety_level,
                is_public,
            )
            .await?;

        // 2. Insert the pack memes
        for media_id in &media_ids {
            self.repo.insert_pack_meme(&mut tx, pack_id, *media_id).await?;
        }

        // 3. Mark media attached as in terra_backend
        if !media_ids.is_empty() {
            self.mark_media_attached.execute(&media_ids).await?;
        }

        tx.commit().await?;

        Ok(pack_id)
    }
}
