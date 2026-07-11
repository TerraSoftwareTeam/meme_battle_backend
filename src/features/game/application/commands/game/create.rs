use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::{Game, GameMode},
            ports::GameRepository,
        },
    },
};

pub struct CreateGameCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl CreateGameCommand {
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
        creator_id: Uuid,
        mode: GameMode,
        situation_pack_ids: Vec<Uuid>,
        meme_pack_ids: Vec<Uuid>,
        max_rounds: i32,
        hand_size: i32,
    ) -> Result<Game, AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Create Game
        let game = self.repo.create_game(&mut tx, creator_id, mode, max_rounds, hand_size).await?;

        // 2. Select Packs
        for pack_id in situation_pack_ids {
            self.repo
                .add_selected_situation_pack(&mut tx, game.id, pack_id)
                .await?;
        }
        for pack_id in meme_pack_ids {
            self.repo
                .add_selected_meme_pack(&mut tx, game.id, pack_id)
                .await?;
        }

        // 3. Add Host as Player (default ready since they are starting)
        self.repo.add_player(&mut tx, game.id, creator_id, true).await?;

        // 4. Insert GameCreated event in event store
        self.repo.insert_game_event(
            &mut tx,
            Uuid::new_v4(),
            game.id,
            game.version,
            "GameCreated",
            json!({
                "host_id": creator_id,
                "mode": game.mode
            }),
        )
        .await?;

        let mode_str = match game.mode {
            GameMode::SituationToMeme => "situation_to_meme".to_string(),
            GameMode::MemeToSituation => "meme_to_situation".to_string(),
        };

        self.notification_sender
            .notify_lobby_created(&mut tx, game.id, creator_id, mode_str, max_rounds, hand_size, 1, game.created_at)
            .await?;

        tx.commit().await?;

        Ok(game)
    }
}
