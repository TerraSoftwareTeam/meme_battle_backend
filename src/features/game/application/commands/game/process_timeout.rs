use std::sync::Arc;
use uuid::Uuid;
use chrono::Utc;
use serde_json::json;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::{GameEvent, GameStatus, RoundPhase, GameRound},
            ports::GameRepository,
        },
    },
};

pub struct ProcessTimeoutCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl ProcessTimeoutCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
    }

    #[allow(unused_assignments)]
    pub async fn execute(&self, round_id: Uuid) -> Result<(), AppError> {
        let mut tx = self.repo.begin().await?;

        // 1. Lock Round
        let round = self.repo
            .get_round_for_update(&mut tx, round_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Round not found: {}", round_id)))?;

        // 2. Lock Game
        let game = self.repo
            .find_game_for_update(&mut tx, round.game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", round.game_id)))?;

        if game.status != GameStatus::Playing {
            return Ok(()); // Game is not active, nothing to do
        }

        // 3. Concurrency guard: check if the round phase has actually expired.
        // If phase_expires_at is in the future or not set, it has already been transitioned by a concurrent process.
        if let Some(expires_at) = round.phase_expires_at {
            if expires_at > Utc::now() {
                return Ok(());
            }
        } else {
            return Ok(());
        }

        match round.phase {
            RoundPhase::Submitting => {
                // Determine who hasn't submitted yet
                let submission_states = self.repo.get_players_with_submissions(game.id, Some(round_id)).await?;

                let mut new_version = game.version;

                for state in submission_states {
                    if !state.has_submitted {
                        // Find first unused card in hand
                        let unused_cards = self.repo.get_unused_hand_cards(&mut tx, game.id, state.user_id).await?;
                        if let Some(hand_card) = unused_cards.first() {
                            // Auto-submit the card
                            self.repo
                                .insert_submission(
                                    &mut tx,
                                    round_id,
                                    state.user_id,
                                    hand_card.meme_id,
                                    hand_card.situation_id,
                                )
                                .await?;
                            self.repo.mark_card_used(&mut tx, hand_card.id).await?;

                            // Notify Centrifugo & insert event
                            new_version = self.repo.increment_game_version(&mut tx, game.id).await?;
                            self.repo
                                .insert_game_event(
                                    &mut tx,
                                    Uuid::new_v4(),
                                    game.id,
                                    new_version,
                                    "SubmissionReceived",
                                    json!({
                                        "round_id": round_id,
                                        "user_id": state.user_id
                                    }),
                                )
                                .await?;
                            self.notification_sender
                                .notify_submission_received(&mut tx, game.id, round_id, state.user_id, new_version)
                                .await?;
                        }
                    }
                }

                // Transition to Voting phase
                let expires_at = Utc::now() + chrono::Duration::seconds(game.vote_time_limit as i64);
                self.repo
                    .update_round_phase(&mut tx, round_id, RoundPhase::Voting, Some(expires_at))
                    .await?;

                new_version = self.repo.increment_game_version(&mut tx, game.id).await?;
                self.repo
                    .insert_game_event(
                        &mut tx,
                        Uuid::new_v4(),
                        game.id,
                        new_version,
                        "RoundPhaseChanged",
                        json!({
                            "round_id": round_id,
                            "phase": "voting"
                        }),
                    )
                    .await?;

                self.notification_sender
                    .notify_round_phase_changed(
                        &mut tx,
                        game.id,
                        round_id,
                        "voting".to_string(),
                        Some(expires_at),
                        new_version,
                    )
                    .await?;
            }
            RoundPhase::Voting => {
                // Finish the round using whatever votes are registered
                let players = self.repo.get_players(game.id).await?;
                let is_last_round = game.current_round >= game.max_rounds;

                let tally = self.repo.get_votes_by_submission(&mut tx, round_id).await?;
                let winner_user_id = if let Some((winning_sub_id, _)) =
                    tally.into_iter().max_by_key(|&(_, c)| c)
                {
                    let winning_sub = self
                        .repo
                        .get_submission_by_id(winning_sub_id)
                        .await?
                        .ok_or(AppError::InternalError)?;
                    Some(winning_sub.user_id)
                } else {
                    None
                };

                // Update round read-model
                self.repo
                    .update_round_winner_and_phase(
                        &mut tx,
                        round_id,
                        winner_user_id,
                        RoundPhase::Finished,
                    )
                    .await?;

                // Award point to winner
                if let Some(winner) = winner_user_id {
                    self.repo
                        .increment_player_score(&mut tx, game.id, winner)
                        .await?;
                }

                // Draw reserve card for each player
                let round_number = round.round_number;
                for player in &players {
                    self.repo
                        .draw_reserve_card(&mut tx, game.id, player.user_id, round_number)
                        .await?;
                }

                // Fetch fresh scores
                let updated_players = self.repo.get_players(game.id).await?;
                let scores: Vec<(Uuid, i32)> = updated_players
                    .iter()
                    .map(|p| (p.user_id, p.score))
                    .collect();

                let round_scoreboard = self
                    .repo
                    .get_round_scoreboard(&mut tx, game.id, round_id)
                    .await?;

                let mut events = vec![GameEvent::RoundFinished {
                    round_id,
                    winner_user_id,
                    scores: scores.clone(),
                    round_scores: round_scoreboard,
                }];

                let mut next_current_round = game.current_round;

                if is_last_round {
                    events.push(GameEvent::GameFinished {
                        final_scores: scores.clone(),
                    });
                    self.repo
                        .update_game_status(&mut tx, game.id, GameStatus::Finished)
                        .await?;
                    self.repo
                        .delete_game_content_locks(&mut tx, game.id)
                        .await?;
                } else {
                    next_current_round += 1;
                    self.repo
                        .update_game_current_round(&mut tx, game.id, next_current_round)
                        .await?;

                    let expires_at = Utc::now() + chrono::Duration::seconds(game.submit_time_limit as i64);
                    self.repo
                        .activate_next_round(&mut tx, game.id, next_current_round, Some(expires_at))
                        .await?;
                }

                let base_version = game.version;
                for (i, event) in events.iter().enumerate() {
                    let slot = base_version + 1 + i as i64;
                    let payload = event_payload(event);

                    self.repo
                        .insert_game_event(
                            &mut tx,
                            Uuid::new_v4(),
                            game.id,
                            slot,
                            event.event_type(),
                            payload,
                        )
                        .await?;

                    match event {
                        GameEvent::RoundFinished { round_id, winner_user_id, scores, round_scores } => {
                            self.notification_sender
                                .notify_round_finished(
                                    &mut tx,
                                    game.id,
                                    *round_id,
                                    round.round_number,
                                    winner_user_id.unwrap_or(Uuid::nil()),
                                    scores.clone(),
                                    round_scores.clone(),
                                    slot,
                                )
                                .await?;

                            for player in &players {
                                let cards = self.repo.get_player_hand_with_media(&mut tx, game.id, player.user_id).await?;
                                self.notification_sender
                                    .notify_hand_updated(&mut tx, game.id, player.user_id, *round_id, cards, slot)
                                    .await?;
                            }

                            if !is_last_round {
                                // Fetch next round to get prompt
                                let next_round = sqlx::query_as::<_, GameRound>(
                                    r#"
                                    SELECT id, game_id, round_number, prompt_situation_id, prompt_meme_id, phase, winner_user_id, phase_expires_at, claimed_at, claimed_by, created_at
                                    FROM game_rounds
                                    WHERE game_id = $1 AND round_number = $2
                                    "#,
                                )
                                .bind(game.id)
                                .bind(next_current_round)
                                .fetch_one(&mut *tx)
                                .await?;

                                let prompt_kind = if next_round.prompt_situation_id.is_some() {
                                    "situation".to_string()
                                } else {
                                    "meme".to_string()
                                };
                                let prompt_id = next_round.prompt_situation_id
                                    .or(next_round.prompt_meme_id)
                                    .unwrap();
                                let (prompt_media_id, prompt_text) = self
                                    .repo
                                    .get_prompt_details(&mut tx, &prompt_kind, prompt_id)
                                    .await?;

                                let next_expires_at = next_round.phase_expires_at.unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(game.submit_time_limit as i64));
                                self.notification_sender
                                    .notify_round_started(
                                        &mut tx,
                                        game.id,
                                        next_round.id,
                                        next_round.round_number,
                                        prompt_kind,
                                        prompt_media_id,
                                        prompt_text,
                                        next_expires_at,
                                        slot,
                                    )
                                    .await?;
                            }
                        }
                        GameEvent::GameFinished { final_scores } => {
                            let winner = updated_players
                                .iter()
                                .max_by_key(|p| p.score)
                                .map(|p| p.user_id)
                                .unwrap_or(Uuid::nil());
                            self.notification_sender
                                .notify_game_finished(&mut tx, game.id, winner, final_scores.clone(), slot)
                                .await?;
                        }
                        _ => {}
                    }

                    self.repo.increment_game_version(&mut tx, game.id).await?;
                }
            }
            _ => {} // Waiting, Finished - no timers/timeouts
        }

        tx.commit().await?;
        Ok(())
    }
}

fn event_payload(event: &GameEvent) -> serde_json::Value {
    match event {
        GameEvent::VoteRegistered { round_id, voter_id } => json!({
            "round_id": round_id,
            "voter_id": voter_id,
        }),
        GameEvent::RoundFinished {
            round_id,
            winner_user_id,
            scores,
            round_scores,
        } => {
            let score_list: Vec<serde_json::Value> = scores
                .iter()
                .map(|(uid, s)| json!({ "user_id": uid, "score": s }))
                .collect();
            let round_score_list: Vec<serde_json::Value> = round_scores
                .iter()
                .map(|(uid, s)| json!({ "user_id": uid, "score": s }))
                .collect();
            json!({
                "round_id": round_id,
                "winner_user_id": winner_user_id,
                "scores": score_list,
                "round_scores": round_score_list,
            })
        }
        GameEvent::GameFinished { final_scores } => {
            let score_list: Vec<serde_json::Value> = final_scores
                .iter()
                .map(|(uid, s)| json!({ "user_id": uid, "score": s }))
                .collect();
            json!({ "final_scores": score_list })
        }
    }
}
