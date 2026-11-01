use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::{ContentSafetyLevel, LanguageCode},
            ports::GameRepository,
        },
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
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
        prompts: Vec<String>,
    ) -> Result<Uuid, AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Insert the pack
        let pack_id = self
            .repo
            .insert_situation_pack(
                &mut tx,
                author_id,
                &name,
                description.as_deref(),
                language_code,
                safety_level,
                is_public,
            )
            .await?;

        // 2. Insert prompts
        for prompt in &prompts {
            self.repo
                .insert_pack_situation(&mut tx, pack_id, prompt)
                .await?;
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
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError> {
        let pack = self
            .repo
            .find_situation_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can update it".to_string(),
            ));
        }

        let mut tx = self.repo.begin().await?;
        self.repo
            .update_situation_pack(
                &mut tx,
                pack_id,
                &name,
                description.as_deref(),
                language_code,
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
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl DeleteSituationPackCommand {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        notification_sender: Arc<dyn GameNotificationSender>,
    ) -> Self {
        Self {
            repo,
            notification_sender,
        }
    }

    pub async fn execute(&self, author_id: Uuid, pack_id: Uuid) -> Result<(), AppError> {
        let pack = self
            .repo
            .find_situation_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can delete it".to_string(),
            ));
        }

        let mut tx = self.repo.begin().await?;
        self.repo.delete_situation_pack(&mut tx, pack_id).await?;

        // Remove from unstarted lobbies and notify them
        let affected_lobbies = self
            .repo
            .remove_pack_from_unstarted_lobbies(&mut tx, pack_id, false)
            .await?;
        for lobby_id in affected_lobbies {
            let players = self.repo.get_players(lobby_id).await?;
            let meme_packs = self
                .repo
                .get_selected_meme_pack_ids(&mut tx, lobby_id)
                .await?;
            let situation_packs = self
                .repo
                .get_selected_situation_pack_ids(&mut tx, lobby_id)
                .await?;
            let new_version = self.repo.increment_game_version(&mut tx, lobby_id).await?;
            self.notification_sender
                .notify_lobby_packs_updated(
                    &mut tx,
                    lobby_id,
                    meme_packs,
                    situation_packs,
                    new_version,
                )
                .await?;
            self.notification_sender
                .notify_lobby_updated(&mut tx, lobby_id, players.len() as i32)
                .await?;
        }

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
        let pack = self
            .repo
            .find_situation_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can add situations to it".to_string(),
            ));
        }

        let mut tx = self.repo.begin().await?;
        for prompt in &prompts {
            self.repo
                .insert_pack_situation(&mut tx, pack_id, prompt)
                .await?;
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
        let sit = self
            .repo
            .find_pack_situation_by_id(situation_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Pack situation not found".to_string()))?;

        let pack = self
            .repo
            .find_situation_pack(sit.pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Situation pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can delete situations from it".to_string(),
            ));
        }

        let mut tx = self.repo.begin().await?;
        self.repo
            .delete_pack_situation(&mut tx, situation_id)
            .await?;
        tx.commit().await?;

        Ok(())
    }
}
