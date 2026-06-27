use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{Game, GameMode},
        ports::GameRepository,
    },
    features::game::application::commands::outbox_helper::publish_event,
};

pub struct CreateGameCommand {
    repo: Arc<dyn GameRepository>,
}

impl CreateGameCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        creator_id: Uuid,
        mode: GameMode,
        situation_pack_ids: Vec<Uuid>,
        meme_pack_ids: Vec<Uuid>,
    ) -> Result<Game, AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Create Game
        let game = self.repo.create_game(&mut tx, creator_id, mode).await?;

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

        // 4. Publish Event
        publish_event(
            self.repo.as_ref(),
            &mut tx,
            game.id,
            game.version,
            "GameCreated",
            json!({
                "host_id": creator_id,
                "mode": game.mode
            }),
        )
        .await?;

        tx.commit().await?;

        Ok(game)
    }
}
