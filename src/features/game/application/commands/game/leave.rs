use std::sync::Arc;
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

pub struct LeaveGameCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl LeaveGameCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Lock Game
        let game = self.repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        // Can only leave while in Lobby state
        if game.status != GameStatus::Lobby {
            return Err(AppError::Conflict("Cannot leave an active or finished game".to_string()));
        }

        // 2. Check if player is in game
        let players = self.repo.get_players_tx(&mut tx, game_id).await?;
        if !players.iter().any(|p| p.user_id == user_id) {
            // Already not in game - idempotent success
            return Ok(());
        }

        // 3. Remove player
        self.repo.remove_player(&mut tx, game_id, user_id).await?;

        let remaining_players_count = (players.len() as i32) - 1;

        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        self.repo
            .insert_game_event(
                &mut tx,
                Uuid::new_v4(),
                game_id,
                new_version,
                "PlayerLeft",
                serde_json::json!({
                    "user_id": user_id,
                    "players_count": remaining_players_count,
                }),
            )
            .await?;

        self.notification_sender
            .notify_player_left(&mut tx, game_id, user_id, remaining_players_count, new_version)
            .await?;

        self.notification_sender
            .notify_lobby_updated(&mut tx, game_id, remaining_players_count)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
