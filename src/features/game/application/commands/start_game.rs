use std::sync::Arc;
use rand::seq::{SliceRandom, IndexedRandom};
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{GameMode, GameStatus},
        ports::GameRepository,
    },
    features::game::application::commands::outbox_helper::publish_event,
};

pub struct StartGameCommand {
    repo: Arc<dyn GameRepository>,
}

impl StartGameCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Lock game
        let game = self.repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        if game.host_id != user_id {
            return Err(AppError::Forbidden("Only host can start the game".to_string()));
        }

        if game.status != GameStatus::Lobby {
            return Err(AppError::Conflict("Game has already started or finished".to_string()));
        }

        let players = self.repo.get_players(game_id).await?;
        if players.len() < 2 {
            return Err(AppError::ValidationError("Need at least 2 players to start".to_string()));
        }

        if !players.iter().all(|p| p.is_ready) {
            return Err(AppError::Conflict("Not all players are ready".to_string()));
        }

        // Fetch available cards
        let available_memes = self.repo.get_available_memes(game_id).await?;
        let available_situations = self.repo.get_available_situations(game_id).await?;

        // Variables to store assignments
        let mut dealt_cards: Vec<(Uuid, Option<Uuid>, Option<Uuid>)> = Vec::new(); // (player_user_id, meme_id, situation_id)
        let prompt_situation_id: Option<Uuid>;
        let prompt_meme_id: Option<Uuid>;

        // Sync block to use ThreadRng and drop it before any await calls
        {
            let mut rng = rand::rng();

            match game.mode {
                GameMode::SituationToMeme => {
                    if available_memes.len() < players.len() * 5 {
                        return Err(AppError::ValidationError("Not enough memes in packs to deal hands".to_string()));
                    }
                    let mut memes = available_memes.clone();
                    memes.shuffle(&mut rng);

                    let mut card_idx = 0;
                    for player in &players {
                        for _ in 0..5 {
                            dealt_cards.push((player.user_id, Some(memes[card_idx]), None));
                            card_idx += 1;
                        }
                    }

                    if available_situations.is_empty() {
                        return Err(AppError::ValidationError("No prompt situations available".to_string()));
                    }
                    let sit_id = *available_situations.choose(&mut rng).ok_or_else(|| {
                        AppError::ValidationError("No prompt situations available".to_string())
                    })?;
                    prompt_situation_id = Some(sit_id);
                    prompt_meme_id = None;
                }
                GameMode::MemeToSituation => {
                    if available_situations.len() < players.len() * 5 {
                        return Err(AppError::ValidationError("Not enough situations in packs to deal hands".to_string()));
                    }
                    let mut situations = available_situations.clone();
                    situations.shuffle(&mut rng);

                    let mut card_idx = 0;
                    for player in &players {
                        for _ in 0..5 {
                            dealt_cards.push((player.user_id, None, Some(situations[card_idx])));
                            card_idx += 1;
                        }
                    }

                    if available_memes.is_empty() {
                        return Err(AppError::ValidationError("No prompt memes available".to_string()));
                    }
                    let m_id = *available_memes.choose(&mut rng).ok_or_else(|| {
                        AppError::ValidationError("No prompt memes available".to_string())
                    })?;
                    prompt_situation_id = None;
                    prompt_meme_id = Some(m_id);
                }
            }
        }

        // 2. Update status to Playing
        self.repo
            .update_game_status(&mut tx, game_id, GameStatus::Playing)
            .await?;

        // 3. Save dealt hand cards (awaiting DB inserts)
        for (player_user_id, meme_id, situation_id) in dealt_cards {
            self.repo
                .insert_hand_card(&mut tx, game_id, player_user_id, meme_id, situation_id)
                .await?;
        }

        // 4. Create the first round
        let round = self.repo
            .insert_round(&mut tx, game_id, 1, prompt_situation_id, prompt_meme_id)
            .await?;

        // 5. Increment version
        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // 6. Publish event
        let prompt_card = self.repo
            .get_prompt_card(prompt_situation_id, prompt_meme_id)
            .await?
            .ok_or_else(|| AppError::InternalError)?;

        publish_event(
            self.repo.as_ref(),
            &mut tx,
            game_id,
            new_version,
            "GameStarted",
            json!({
                "round_id": round.id,
                "prompt": prompt_card
            }),
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
