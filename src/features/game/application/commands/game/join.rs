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

pub struct JoinGameCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl JoinGameCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.repo.begin().await?;

        // Row lock game
        let game = self.repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        if game.status != GameStatus::Lobby {
            return Err(AppError::Conflict("Cannot join active or finished game".to_string()));
        }

        // Check if player is already in game
        let players = self.repo.get_players(game_id).await?;
        if players.iter().any(|p| p.user_id == user_id) {
            return Err(AppError::Conflict("Player already in game".to_string()));
        }

        // Add player
        self.repo.add_player(&mut tx, game_id, user_id, false).await?;

        // Increment version
        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // Publish event
        let players_count = (players.len() + 1) as i32;

        self.repo
            .insert_game_event(
                &mut tx,
                Uuid::new_v4(),
                game_id,
                new_version,
                "PlayerJoined",
                serde_json::json!({
                    "user_id": user_id,
                    "players_count": players_count
                }),
            )
            .await?;

        self.notification_sender
            .notify_player_joined(&mut tx, game_id, user_id, players_count, new_version)
            .await?;

        tx.commit().await?;
        Ok(())
    }
}
