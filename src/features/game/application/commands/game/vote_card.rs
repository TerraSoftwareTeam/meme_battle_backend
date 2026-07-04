use std::sync::Arc;
use serde_json::json;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        application::ports::game_notification_sender::GameNotificationSender,
        domain::{
            model::{GameAggregate, GameEvent, GameStatus, RoundPhase},
            ports::GameRepository,
        },
    },
};

/// Command Handler for `SubmitVoteCommand`.
///
/// # Event Sourcing flow
///
/// ```text
/// 1.  Load aggregate (games row)  ──►  GameAggregate::from_game
/// 2.  Validate invariants          (guard clauses)
/// 3.  Produce domain events        (Vec<GameEvent>)
/// 4.  Save to DB in ONE transaction:
///       a. UPDATE games read-model (version, status, current_round)
///       b. UPDATE game_rounds read-model (winner, phase)
///       c. UPDATE game_players read-model (score)
///       d. INSERT game_events  ← OCC: UNIQUE(game_id, version) catches races
///       e. INSERT realtime_outbox (Transactional Outbox pattern)
/// 5.  apply_events in-memory (optional projection for tests / return value)
/// ```
pub struct VoteCardCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl VoteCardCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
    }

    pub async fn execute(
        &self,
        user_id: Uuid,
        game_id: Uuid,
        round_id: Uuid,
        submission_id: Uuid,
    ) -> Result<(), AppError> {
        // ── Phase 1: Load & validate (outside transaction for cheaper reads) ──

        // 1a. Fetch aggregate with all fields we need for state decisions
        let mut tx = self.repo.begin().await?;
        let game = self
            .repo
            .find_game_for_update(&mut tx, game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        if game.status != GameStatus::Playing {
            return Err(AppError::Conflict("Game is not active".to_string()));
        }

        // 1b. Build in-memory aggregate from the read-model snapshot
        let mut aggregate = GameAggregate::from_game(&game);
        let is_last_round = aggregate.is_last_round();

        // 1c. Lock the round row so no concurrent request can mutate it
        let round = self
            .repo
            .get_round_for_update(&mut tx, round_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Round not found: {}", round_id)))?;

        if round.phase != RoundPhase::Voting {
            return Err(AppError::Conflict(
                "Round is not in voting phase".to_string(),
            ));
        }

        if round.game_id != game_id {
            return Err(AppError::ValidationError(
                "Round does not belong to this game".to_string(),
            ));
        }

        // 1d. Verify voter is a registered player
        let players = self.repo.get_players(game_id).await?;
        if !players.iter().any(|p| p.user_id == user_id) {
            return Err(AppError::Forbidden(
                "Only registered players can vote".to_string(),
            ));
        }

        // 1e. Idempotency: reject duplicate votes
        if self
            .repo
            .check_player_voted(&mut tx, round_id, user_id)
            .await?
        {
            return Err(AppError::Conflict(
                "You have already voted in this round".to_string(),
            ));
        }

        // 1f. Verify the target submission exists and belongs to this round
        let submission = self
            .repo
            .get_submission_by_id(submission_id)
            .await?
            .ok_or_else(|| AppError::NotFound("Submission not found".to_string()))?;

        if submission.round_id != round_id {
            return Err(AppError::ValidationError(
                "Submission does not belong to this round".to_string(),
            ));
        }

        // 1g. Anti-cheat: cannot vote for own submission
        if submission.user_id == user_id {
            return Err(AppError::ValidationError(
                "Cannot vote for your own submission".to_string(),
            ));
        }

        // ── Phase 2: Mutate state & produce events ────────────────────────────

        // 2a. Register the vote
        self.repo
            .insert_vote(&mut tx, round_id, user_id, submission_id)
            .await?;

        let votes_count = self.repo.get_votes_count(&mut tx, round_id).await?;

        // Collect all events produced in this command invocation
        let mut events: Vec<GameEvent> = Vec::new();

        // VoteRegistered is always emitted
        events.push(GameEvent::VoteRegistered {
            round_id,
            voter_id: user_id,
        });

        // 2b. Check if every player has now voted → finish the round
        if votes_count >= players.len() as i64 {
            // Determine the winner by highest vote count (stable tiebreak: first found)
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

            // Update game_rounds read-model
            self.repo
                .update_round_winner_and_phase(&mut tx, round_id, winner_user_id.unwrap_or(Uuid::nil()), RoundPhase::Finished)
                .await?;

            // Award point to winner
            if let Some(winner) = winner_user_id {
                self.repo
                    .increment_player_score(&mut tx, game_id, winner)
                    .await?;
            }

            // Draw reserve card for each player
            let round_number = round.round_number;
            for player in &players {
                self.repo
                    .draw_reserve_card(&mut tx, game_id, player.user_id, round_number)
                    .await?;
            }

            // Fetch fresh scores for the event payload
            let updated_players = self.repo.get_players(game_id).await?;
            let scores: Vec<(Uuid, i32)> = updated_players
                .iter()
                .map(|p| (p.user_id, p.score))
                .collect();

            events.push(GameEvent::RoundFinished {
                round_id,
                winner_user_id,
                scores: scores.clone(),
            });

            // 2c. Apply events in-memory to know new current_round after RoundFinished
            aggregate.apply_events(&events);

            // Update games read-model: current_round
            self.repo
                .update_game_current_round(&mut tx, game_id, aggregate.current_round)
                .await?;

            // 2d. If this was the last round → emit GameFinished
            if is_last_round {
                let final_scores: Vec<(Uuid, i32)> = updated_players
                    .iter()
                    .map(|p| (p.user_id, p.score))
                    .collect();

                events.push(GameEvent::GameFinished {
                    final_scores: final_scores.clone(),
                });

                // Update games read-model: status
                self.repo
                    .update_game_status(&mut tx, game_id, GameStatus::Finished)
                    .await?;

                // Delete content locks since game is finished
                self.repo
                    .delete_game_content_locks(&mut tx, game_id)
                    .await?;

                // Re-apply to capture the Finished status change in aggregate
                aggregate.apply_events(&[GameEvent::GameFinished {
                    final_scores: final_scores.clone(),
                }]);
            } else {
                self.repo
                    .activate_next_round(&mut tx, game_id, aggregate.current_round)
                    .await?;
            }
        } else {
            // No round completion yet — still apply the VoteRegistered in-memory
            aggregate.apply_events(&events);
        }

        // ── Phase 3: Persist events (OCC + Transactional Outbox) ─────────────

        let base_version = game.version; // the version we read under the lock
        for (i, event) in events.iter().enumerate() {
            let slot = base_version + 1 + i as i64;
            let payload = event_payload(event);

            // 1. Write to game_events table (event store)
            self.repo
                .insert_game_event(
                    &mut tx,
                    Uuid::new_v4(),
                    game_id,
                    slot,
                    event.event_type(),
                    payload.clone(),
                )
                .await?;

            // 2. Map to centrifugo realtime envelope and insert into outbox
            match event {
                GameEvent::VoteRegistered { round_id, voter_id } => {
                    self.notification_sender
                        .notify_vote_received(&mut tx, game_id, *round_id, *voter_id, slot)
                        .await?;
                }
                GameEvent::RoundFinished { round_id, winner_user_id, scores } => {
                    self.notification_sender
                        .notify_round_finished(
                            &mut tx,
                            game_id,
                            *round_id,
                            round.round_number,
                            winner_user_id.unwrap_or(Uuid::nil()),
                            scores.clone(),
                            slot,
                        )
                        .await?;

                    // Also send hand updated events for each player
                    for player in &players {
                        let cards = self.repo.get_player_hand_with_media(&mut tx, game_id, player.user_id).await?;
                        self.notification_sender
                            .notify_hand_updated(&mut tx, game_id, player.user_id, *round_id, cards, slot)
                            .await?;
                    }

                    // If not last round, send next RoundStarted event
                    if !is_last_round {
                        let next_round = sqlx::query_as::<_, crate::features::game::domain::model::GameRound>(
                            r#"
                            SELECT id, game_id, round_number, prompt_situation_id, prompt_meme_id, phase, winner_user_id, created_at
                            FROM game_rounds
                            WHERE game_id = $1 AND round_number = $2
                            "#,
                        )
                        .bind(game_id)
                        .bind(aggregate.current_round)
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

                        self.notification_sender
                            .notify_round_started(
                                &mut tx,
                                game_id,
                                next_round.id,
                                next_round.round_number,
                                prompt_kind,
                                prompt_id,
                                slot,
                            )
                            .await?;
                    }
                }
                GameEvent::GameFinished { final_scores } => {
                    let updated_players = self.repo.get_players(game_id).await?;
                    let winner_user_id = updated_players
                        .iter()
                        .max_by_key(|p| p.score)
                        .map(|p| p.user_id)
                        .unwrap_or(Uuid::nil());

                    self.notification_sender
                        .notify_game_finished(&mut tx, game_id, winner_user_id, final_scores.clone(), slot)
                        .await?;
                }
            }

            // Advance the games.version read-model for every event we commit
            self.repo.increment_game_version(&mut tx, game_id).await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Serialize a domain event to the JSON payload stored in `game_events.payload`
/// and forwarded through the Centrifugo Outbox.
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
        } => {
            let score_list: Vec<serde_json::Value> = scores
                .iter()
                .map(|(uid, s)| json!({ "user_id": uid, "score": s }))
                .collect();
            json!({
                "round_id": round_id,
                "winner_user_id": winner_user_id,
                "scores": score_list,
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

