use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::{Game, GameMode, GameStatus},
            ports::GameRepository,
        },
    },
};

pub struct UpdateGameCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl UpdateGameCommand {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        notification_sender: Arc<dyn GameNotificationSender>,
    ) -> Self {
        Self {
            repo,
            notification_sender,
        }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        game_id: Uuid,
        name: Option<String>,
        mode: Option<GameMode>,
        selected_situation_pack_ids: Option<Vec<Uuid>>,
        selected_meme_pack_ids: Option<Vec<Uuid>>,
        max_rounds: Option<i32>,
        hand_size: Option<i32>,
    ) -> Result<Game, AppError> {
        let trimmed_name = match name {
            Some(n) => {
                let trimmed = n.trim().to_string();
                if trimmed.is_empty() {
                    return Err(AppError::ValidationError("Game name cannot be empty".to_string()));
                }
                if trimmed.chars().count() > 100 {
                    return Err(AppError::ValidationError("Game name cannot exceed 100 characters".to_string()));
                }
                Some(trimmed)
            }
            None => None,
        };

        // Validate situation pack IDs exist if updating them
        if let Some(sit_pack_ids) = &selected_situation_pack_ids {
            for &pack_id in sit_pack_ids {
                if self.repo.find_situation_pack(pack_id).await?.is_none() {
                    return Err(AppError::NotFound(format!("Situation pack not found: {}", pack_id)));
                }
            }
        }

        // Validate meme pack IDs exist if updating them
        if let Some(meme_pack_ids) = &selected_meme_pack_ids {
            for &pack_id in meme_pack_ids {
                if self.repo.find_meme_pack(pack_id).await?.is_none() {
                    return Err(AppError::NotFound(format!("Meme pack not found: {}", pack_id)));
                }
            }
        }

        let mut tx = self.repo.begin().await?;

        // 1. Lock game
        let game = self.repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        if game.host_id != user_id {
            return Err(AppError::Forbidden("Only host can modify game settings".to_string()));
        }

        if game.status != GameStatus::Lobby {
            return Err(AppError::Conflict("Cannot update settings of a game that has started or finished".to_string()));
        }

        // Apply setting updates
        let new_mode = mode.unwrap_or(game.mode);
        let new_max_rounds = max_rounds.unwrap_or(game.max_rounds);
        let new_hand_size = hand_size.unwrap_or(game.hand_size);

        let effective_situation_pack_ids = match &selected_situation_pack_ids {
            Some(ids) => ids.clone(),
            None => self.repo.get_selected_situation_pack_ids(&mut tx, game_id).await?,
        };
        let effective_meme_pack_ids = match &selected_meme_pack_ids {
            Some(ids) => ids.clone(),
            None => self.repo.get_selected_meme_pack_ids(&mut tx, game_id).await?,
        };

        let total_memes = self.repo.count_cards_in_meme_packs(&effective_meme_pack_ids).await?;
        let total_situations = self.repo.count_cards_in_situation_packs(&effective_situation_pack_ids).await?;

        let new_max_players = Game::calculate_max_players(
            new_mode,
            total_memes,
            total_situations,
            new_hand_size,
            new_max_rounds,
        );

        if new_max_players < 2 {
            return Err(AppError::ValidationError(
                "Selected packs do not contain enough cards for at least 2 players".to_string(),
            ));
        }

        let current_players = self.repo.get_players_tx(&mut tx, game_id).await?;
        if current_players.len() > new_max_players as usize {
            return Err(AppError::Conflict(format!(
                "Cannot change settings: current lobby has {} players, but new settings only support up to {} players",
                current_players.len(),
                new_max_players
            )));
        }

        // Update games table
        self.repo
            .update_game_settings(
                &mut tx,
                game_id,
                trimmed_name.clone(),
                new_mode,
                new_max_rounds,
                new_hand_size,
                new_max_players,
            )
            .await?;

        // Update selected situation packs if specified
        if let Some(sit_pack_ids) = selected_situation_pack_ids {
            self.repo.clear_selected_situation_packs(&mut tx, game_id).await?;
            for pack_id in sit_pack_ids {
                self.repo
                    .add_selected_situation_pack(&mut tx, game_id, pack_id)
                    .await?;
            }
        }

        // Update selected meme packs if specified
        if let Some(meme_pack_ids) = selected_meme_pack_ids {
            self.repo.clear_selected_meme_packs(&mut tx, game_id).await?;
            for pack_id in meme_pack_ids {
                self.repo
                    .add_selected_meme_pack(&mut tx, game_id, pack_id)
                    .await?;
            }
        }

        // Increment version
        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // Publish event (only to event sourced table game_events)
        self.repo.insert_game_event(
            &mut tx,
            Uuid::new_v4(),
            game_id,
            new_version,
            "GameSettingsUpdated",
            json!({
                "name": trimmed_name,
                "mode": new_mode,
                "max_rounds": new_max_rounds,
                "hand_size": new_hand_size,
                "max_players": new_max_players
            }),
        )
        .await?;

        self.notification_sender
            .notify_lobby_updated(&mut tx, game_id, current_players.len() as i32)
            .await?;

        tx.commit().await?;

        // Reload the game to return it
        let reloaded = self.repo
            .find_game(game_id)
            .await?
            .ok_or(AppError::InternalError)?;
        
        Ok(reloaded)
    }
}
