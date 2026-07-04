use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::GameStatus,
            ports::GameRepository,
        },
    },
};

pub struct SetReadyCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl SetReadyCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid, is_ready: bool) -> Result<(), AppError> {
        let mut tx = self.repo.begin().await?;

        // Row lock game
        let game = self.repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        if game.status != GameStatus::Lobby {
            return Err(AppError::Conflict("Cannot change ready status during game".to_string()));
        }

        // Check if player exists in the game
        let players = self.repo.get_players(game_id).await?;
        if !players.iter().any(|p| p.user_id == user_id) {
            return Err(AppError::NotFound("Player not found in game".to_string()));
        }

        // Update player ready
        self.repo
            .update_player_ready(&mut tx, game_id, user_id, is_ready)
            .await?;

        // Increment version
        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // Publish event
        let payload = json!({
            "user_id": user_id,
            "is_ready": is_ready
        });

        self.repo
            .insert_game_event(
                &mut tx,
                Uuid::new_v4(),
                game_id,
                new_version,
                "PlayerReadyChanged",
                payload,
            )
            .await?;

        self.notification_sender
            .notify_player_ready_changed(&mut tx, game_id, user_id, is_ready, new_version)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
