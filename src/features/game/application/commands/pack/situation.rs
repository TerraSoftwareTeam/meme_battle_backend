use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::ContentSafetyLevel,
        ports::GameRepository,
    },
};

pub struct CreateSituationPackCommand {
    repo: Arc<dyn GameRepository>,
}

impl CreateSituationPackCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        author_id: Uuid,
        name: String,
        description: Option<String>,
        language_code: String,
        safety_level: ContentSafetyLevel,
        is_public: bool,
        prompts: Vec<String>,
    ) -> Result<Uuid, AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Insert the pack
        let pack_id = self.repo
            .insert_situation_pack(
                &mut tx,
                author_id,
                &name,
                description.as_deref(),
                &language_code,
                safety_level,
                is_public,
            )
            .await?;

        // 2. Insert prompts
        for prompt in &prompts {
            self.repo.insert_pack_situation(&mut tx, pack_id, prompt).await?;
        }

        tx.commit().await?;
        Ok(pack_id)
    }
}

pub struct UpdateSituationPackCommand {
    repo: Arc<dyn GameRepository>,
}

impl UpdateSituationPackCommand {
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
        let pack = self.repo.find_situation_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can update it".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        self.repo
            .update_situation_pack(
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

pub struct DeleteSituationPackCommand {
    repo: Arc<dyn GameRepository>,
}

impl DeleteSituationPackCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, author_id: Uuid, pack_id: Uuid) -> Result<(), AppError> {
        let pack = self.repo.find_situation_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can delete it".to_string()));
        }

        if self.repo.is_situation_pack_locked(pack_id).await? {
            return Err(AppError::Conflict("Situation pack is currently in use by an active game session".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        self.repo.delete_situation_pack(&mut tx, pack_id).await?;
        tx.commit().await?;

        Ok(())
    }
}

pub struct AddSituationsToPackCommand {
    repo: Arc<dyn GameRepository>,
}

impl AddSituationsToPackCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        author_id: Uuid,
        pack_id: Uuid,
        prompts: Vec<String>,
    ) -> Result<(), AppError> {
        let pack = self.repo.find_situation_pack(pack_id).await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can add situations to it".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        for prompt in &prompts {
            self.repo.insert_pack_situation(&mut tx, pack_id, prompt).await?;
        }
        tx.commit().await?;

        Ok(())
    }
}

pub struct DeletePackSituationCommand {
    repo: Arc<dyn GameRepository>,
}

impl DeletePackSituationCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, author_id: Uuid, situation_id: Uuid) -> Result<(), AppError> {
        let sit = self.repo.find_pack_situation_by_id(situation_id).await?
            .ok_or_else(|| AppError::NotFound("Pack situation not found".to_string()))?;

        let pack = self.repo.find_situation_pack(sit.pack_id).await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden("Only pack author can delete situations from it".to_string()));
        }

        if self.repo.is_situation_locked(situation_id).await? {
            return Err(AppError::Conflict("Situation is currently in use by an active game session".to_string()));
        }

        let mut tx = self.repo.begin().await?;
        self.repo.delete_pack_situation(&mut tx, situation_id).await?;
        tx.commit().await?;

        Ok(())
    }
}
