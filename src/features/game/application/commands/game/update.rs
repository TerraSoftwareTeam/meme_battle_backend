use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{Game, GameMode, GameStatus},
        ports::GameRepository,
    },
};

pub struct UpdateGameCommand {
    repo: Arc<dyn GameRepository>,
}

impl UpdateGameCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        game_id: Uuid,
        mode: Option<GameMode>,
        selected_situation_pack_ids: Option<Vec<Uuid>>,
        selected_meme_pack_ids: Option<Vec<Uuid>>,
        max_rounds: Option<i32>,
        hand_size: Option<i32>,
    ) -> Result<Game, AppError> {
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

        // Update games table
        self.repo
            .update_game_settings(&mut tx, game_id, new_mode, new_max_rounds, new_hand_size)
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
                "mode": new_mode,
                "max_rounds": new_max_rounds,
                "hand_size": new_hand_size
            }),
        )
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
