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

pub struct UpdateMemePackCommand {
    repo: Arc<dyn GameRepository>,
}

impl UpdateMemePackCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        author_id: Uuid,
        pack_id: Uuid,
        name: String,
        description: Option<String>,
        language_code: String,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError> {
        let pack = self.repo.find_meme_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can update it".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        self.repo
            .update_meme_pack(
                &mut tx,
                pack_id,
                &name,
                description.as_deref(),
                &language_code,
                safety_level,
                is_public,
            )
            .await?;
        tx.commit().await?;

        Ok(())
    }
}

pub struct DeleteMemePackCommand {
    repo: Arc<dyn GameRepository>,
}

impl DeleteMemePackCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, author_id: Uuid, pack_id: Uuid) -> Result<(), AppError> {
        let pack = self.repo.find_meme_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can delete it".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        self.repo.delete_meme_pack(&mut tx, pack_id).await?;
        tx.commit().await?;

        Ok(())
    }
}

pub struct AddMemesToPackCommand {
    repo: Arc<dyn GameRepository>,
    mark_media_attached: Arc<MarkMediaAttachedCommand>,
}

impl AddMemesToPackCommand {
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
        pack_id: Uuid,
        media_ids: Vec<i64>,
    ) -> Result<(), AppError> {
        let pack = self.repo.find_meme_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can add memes to it".to_string()));
        }

        // Validate that all provided media assets exist before writing anything
        self.repo.validate_media_exists(&media_ids).await?;

        let mut tx = self.repo.begin().await?;
        for media_id in &media_ids {
            self.repo.insert_pack_meme(&mut tx, pack_id, *media_id).await?;
        }

        if !media_ids.is_empty() {
            self.mark_media_attached.execute(&media_ids).await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

pub struct DeletePackMemeCommand {
    repo: Arc<dyn GameRepository>,
}

impl DeletePackMemeCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, author_id: Uuid, meme_id: Uuid) -> Result<(), AppError> {
        let meme = self.repo.find_pack_meme_by_id(meme_id).await?
            .ok_or_else(|| AppError::NotFound("Pack meme not found".to_string()))?;

        let pack = self.repo.find_meme_pack(meme.pack_id).await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can delete memes from it".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        self.repo.delete_pack_meme(&mut tx, meme_id).await?;
        tx.commit().await?;

        Ok(())
    }
}
