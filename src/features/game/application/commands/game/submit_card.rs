use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::{GameMode, GameStatus, RoundPhase},
            ports::GameRepository,
        },
    },
};

pub struct SubmitCardCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl SubmitCardCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        game_id: Uuid,
        card_id: Uuid,
    ) -> Result<(), AppError> {
        let current_round = self.repo
            .get_current_round(game_id)
            .await?
            .ok_or_else(|| AppError::NotFound("No active round found for this game".to_string()))?;
        let round_id = current_round.id;

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
        let mut next_expires_at = None;
        if submissions_count >= players.len() as i64 {
            let expires_at = chrono::Utc::now() + chrono::Duration::seconds(game.vote_time_limit as i64);
            self.repo
                .update_round_phase(&mut tx, round_id, RoundPhase::Voting, Some(expires_at))
                .await?;
            round_phase_changed = true;
            next_expires_at = Some(expires_at);
        }

        // 8. Increment version
        let mut new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // 9. Publish event
        self.repo
            .insert_game_event(
                &mut tx,
                Uuid::new_v4(),
                game_id,
                new_version,
                "SubmissionReceived",
                serde_json::json!({
                    "round_id": round_id,
                    "user_id": user_id
                }),
            )
            .await?;

        self.notification_sender
            .notify_submission_received(&mut tx, game_id, round_id, user_id, new_version)
            .await?;

        if round_phase_changed {
            new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

            self.repo
                .insert_game_event(
                    &mut tx,
                    Uuid::new_v4(),
                    game_id,
                    new_version,
                    "RoundPhaseChanged",
                    serde_json::json!({
                        "round_id": round_id,
                        "phase": "voting"
                    }),
                )
                .await?;

            self.notification_sender
                .notify_round_phase_changed(
                    &mut tx,
                    game_id,
                    round_id,
                    "voting".to_string(),
                    next_expires_at,
                    new_version,
                )
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

