use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{GameMode, GameStatus, RoundPhase},
        ports::GameRepository,
    },
    features::game::application::commands::outbox_helper::publish_event,
};

pub struct SubmitCardCommand {
    repo: Arc<dyn GameRepository>,
}

impl SubmitCardCommand {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        game_id: Uuid,
        round_id: Uuid,
        card_id: Uuid,
    ) -> Result<(), AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Lock Game
        let game = self.repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        if game.status != GameStatus::Playing {
            return Err(AppError::Conflict("Game is not active".to_string()));
        }

        // 2. Lock Round
        let round = self.repo
            .get_round_for_update(&mut tx, round_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Round not found: {}", round_id)))?;

        if round.phase != RoundPhase::Submitting {
            return Err(AppError::Conflict("Round is not in submitting phase".to_string()));
        }

        // 3. Find card in hand
        let hand_card = self.repo
            .check_player_hand_card(&mut tx, game_id, user_id, card_id)
            .await?
            .ok_or_else(|| AppError::ValidationError("Card not in hand or already used".to_string()))?;

        // 4. Validate Mode Compatibility
        match game.mode {
            GameMode::SituationToMeme => {
                if hand_card.meme_id.is_none() {
                    return Err(AppError::ValidationError(
                        "Cannot submit situation card in SituationToMeme mode".to_string(),
                    ));
                }
            }
            GameMode::MemeToSituation => {
                if hand_card.situation_id.is_none() {
                    return Err(AppError::ValidationError(
                        "Cannot submit meme card in MemeToSituation mode".to_string(),
                    ));
                }
            }
        }

        // 5. Submit card
        self.repo
            .insert_submission(&mut tx, round_id, user_id, hand_card.meme_id, hand_card.situation_id)
            .await?;

        // 6. Mark hand card used
        self.repo.mark_card_used(&mut tx, hand_card.id).await?;

        // 7. Check if all players have submitted
        let players = self.repo.get_players(game_id).await?;
        let submissions_count = self.repo.get_submissions_count(&mut tx, round_id).await?;

        let mut round_phase_changed = false;
        if submissions_count >= players.len() as i64 {
            self.repo
                .update_round_phase(&mut tx, round_id, RoundPhase::Voting)
                .await?;
            round_phase_changed = true;
        }

        // 8. Increment version
        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // 9. Publish event
        publish_event(
            self.repo.as_ref(),
            &mut tx,
            game_id,
            new_version,
            "CardSubmitted",
            json!({
                "round_id": round_id,
                "user_id": user_id
            }),
        )
        .await?;

        if round_phase_changed {
            publish_event(
                self.repo.as_ref(),
                &mut tx,
                game_id,
                new_version,
                "RoundPhaseChanged",
                json!({
                    "round_id": round_id,
                    "phase": RoundPhase::Voting
                }),
            )
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
