use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::{
        game::{
            application::ports::{
                game_media_manager::GameMediaManager,
                game_notification_sender::GameNotificationSender,
            },
            domain::{
                model::{ContentSafetyLevel, LanguageCode},
                ports::GameRepository,
            },
        },
        media::MarkMediaAttachedCommand,
    },
};

pub struct CreateMemePackCommand {
    repo: Arc<dyn GameRepository>,
    mark_media_attached: Arc<MarkMediaAttachedCommand>,
    media_manager: Arc<dyn GameMediaManager>,
}

impl CreateMemePackCommand {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        mark_media_attached: Arc<MarkMediaAttachedCommand>,
        media_manager: Arc<dyn GameMediaManager>,
    ) -> Self {
        Self {
            repo,
            mark_media_attached,
            media_manager,
        }
    }

    pub async fn execute(
        &self,
        author_id: Uuid,
        name: String,
        description: Option<String>,
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
        media_ids: Vec<i64>,
    ) -> Result<Uuid, AppError> {
        // Validate that all provided media assets exist before writing anything
        self.media_manager.validate_media_exists(&media_ids).await?;

        let mut tx = self.repo.begin().await?;

        // 1. Insert the pack
        let pack_id = self
            .repo
            .insert_meme_pack(
                &mut tx,
                author_id,
                &name,
                description.as_deref(),
                language_code,
                safety_level,
                is_public,
            )
            .await?;

        // 2. Insert the pack memes
        for media_id in &media_ids {
            self.repo
                .insert_pack_meme(&mut tx, pack_id, *media_id)
                .await?;
        }

        // 3. Mark media attached
        if !media_ids.is_empty() {
            self.mark_media_attached.execute(&media_ids).await?;
        }

        tx.commit().await?;

        Ok(pack_id)
    }
}

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
        language_code: LanguageCode,
        safety_level: ContentSafetyLevel,
        is_public: bool,
    ) -> Result<(), AppError> {
        let pack = self
            .repo
            .find_meme_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can update it".to_string(),
            ));
        }

        let mut tx = self.repo.begin().await?;
        self.repo
            .update_meme_pack(
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

pub struct DeleteMemePackCommand {
    repo: Arc<dyn GameRepository>,
    media_manager: Arc<dyn GameMediaManager>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl DeleteMemePackCommand {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        media_manager: Arc<dyn GameMediaManager>,
        notification_sender: Arc<dyn GameNotificationSender>,
    ) -> Self {
        Self {
            repo,
            media_manager,
            notification_sender,
        }
    }

    pub async fn execute(&self, author_id: Uuid, pack_id: Uuid) -> Result<(), AppError> {
        let pack = self
            .repo
            .find_meme_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can delete it".to_string(),
            ));
        }

        // Collect media asset IDs before deletion
        let media_ids = self.repo.get_pack_media_ids(pack_id).await?;

        let mut tx = self.repo.begin().await?;
        self.repo.delete_meme_pack(&mut tx, pack_id).await?;

        // Remove from unstarted lobbies and notify them
        let affected_lobbies = self
            .repo
            .remove_pack_from_unstarted_lobbies(&mut tx, pack_id, true)
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

        // Enqueue media purge tasks asynchronously (deferred by worker if currently locked in active games)
        if !media_ids.is_empty() {
            self.media_manager
                .enqueue_purge_for_media_ids(&media_ids)
                .await?;
        }

        Ok(())
    }
}

pub struct AddMemesToPackCommand {
    repo: Arc<dyn GameRepository>,
    mark_media_attached: Arc<MarkMediaAttachedCommand>,
    media_manager: Arc<dyn GameMediaManager>,
}

impl AddMemesToPackCommand {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        mark_media_attached: Arc<MarkMediaAttachedCommand>,
        media_manager: Arc<dyn GameMediaManager>,
    ) -> Self {
        Self {
            repo,
            mark_media_attached,
            media_manager,
        }
    }

    pub async fn execute(
        &self,
        author_id: Uuid,
        pack_id: Uuid,
        media_ids: Vec<i64>,
    ) -> Result<(), AppError> {
        let pack = self
            .repo
            .find_meme_pack(pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can add memes to it".to_string(),
            ));
        }

        // Validate that all provided media assets exist before writing anything
        self.media_manager.validate_media_exists(&media_ids).await?;

        let mut tx = self.repo.begin().await?;
        for media_id in &media_ids {
            self.repo
                .insert_pack_meme(&mut tx, pack_id, *media_id)
                .await?;
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
    media_manager: Arc<dyn GameMediaManager>,
}

impl DeletePackMemeCommand {
    pub fn new(repo: Arc<dyn GameRepository>, media_manager: Arc<dyn GameMediaManager>) -> Self {
        Self {
            repo,
            media_manager,
        }
    }

    pub async fn execute(&self, author_id: Uuid, meme_id: Uuid) -> Result<(), AppError> {
        let meme = self
            .repo
            .find_pack_meme_by_id(meme_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Pack meme not found".to_string()))?;

        let pack = self
            .repo
            .find_meme_pack(meme.pack_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Meme pack not found".to_string()))?;

        if pack.author_id != author_id {
            return Err(AppError::Forbidden(
                "Only pack author can delete memes from it".to_string(),
            ));
        }

        let mut tx = self.repo.begin().await?;
        self.repo.delete_pack_meme(&mut tx, meme_id).await?;
        tx.commit().await?;

        if let Some(media_id) = meme.media_id {
            self.media_manager
                .enqueue_purge_for_media_ids(&[media_id])
                .await?;
        }

        Ok(())
    }
}
