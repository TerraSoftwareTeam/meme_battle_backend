use std::sync::Arc;
use rand::SeedableRng;
use rand::rngs::StdRng;
use rand::seq::SliceRandom;
use sha2::{Sha256, Digest};
use uuid::Uuid;
use chrono::Utc;

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

pub struct StartGameCommand {
    repo: Arc<dyn GameRepository>,
    notification_sender: Arc<dyn GameNotificationSender>,
}

impl StartGameCommand {
    pub fn new(repo: Arc<dyn GameRepository>, notification_sender: Arc<dyn GameNotificationSender>) -> Self {
        Self { repo, notification_sender }
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
        if players.len() < 3 {
            return Err(AppError::ValidationError("Need at least 3 players to start".to_string()));
        }

        if !players.iter().all(|p| p.is_ready) {
            return Err(AppError::Conflict("Not all players are ready".to_string()));
        }

        // Fetch available cards
        let available_memes = self.repo.get_available_memes(game_id).await?;
        let available_situations = self.repo.get_available_situations(game_id).await?;

        // 2. Count requirements
        let p = players.len() as i32;
        let h = game.hand_size;
        let r = game.max_rounds;

        let (required_memes, required_situations) = match game.mode {
            GameMode::SituationToMeme => (p * h + p * r, r),
            GameMode::MemeToSituation => (r, p * h + p * r),
        };

        // 3. Verify availability
        if available_memes.len() < required_memes as usize {
            return Err(AppError::ValidationError("not_enough_memes".to_string()));
        }
        if available_situations.len() < required_situations as usize {
            return Err(AppError::ValidationError("not_enough_situations".to_string()));
        }

        // 4. Deterministic Seed
        let secret_seed_key = std::env::var("SECRET_SEED_KEY")
            .unwrap_or_else(|_| "dummysecretseedkeydummysecretseedkey".to_string());

        let mut hasher = Sha256::new();
        hasher.update(game_id.as_bytes());
        hasher.update(secret_seed_key.as_bytes());
        let hash_result = hasher.finalize();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&hash_result);

        let mut rng = StdRng::from_seed(seed);

        let mut shuffled_memes = available_memes.clone();
        shuffled_memes.shuffle(&mut rng);

        let mut shuffled_situations = available_situations.clone();
        shuffled_situations.shuffle(&mut rng);

        // 5. Update game settings & status
        self.repo.start_game(&mut tx, game_id, Utc::now()).await?;

        let mut round_1_details = None;

        // 6. Lay out and save content
        match game.mode {
            GameMode::SituationToMeme => {
                let round_1_deadline = Utc::now() + chrono::Duration::seconds(game.submit_time_limit as i64);
                // R situations -> rounds
                for i in 1..=r {
                    let prompt_situation_id = shuffled_situations[(i - 1) as usize];
                    let phase = if i == 1 { RoundPhase::Submitting } else { RoundPhase::Waiting };
                    let phase_expires_at = if i == 1 { Some(round_1_deadline) } else { None };
                    let round = self.repo
                        .insert_round(&mut tx, game_id, i, Some(prompt_situation_id), None, phase, phase_expires_at)
                        .await?;

                    if i == 1 {
                        round_1_details = Some(round);
                    }

                    // lock situation
                    self.repo
                        .insert_content_lock(&mut tx, game_id, None, Some(prompt_situation_id))
                        .await?;
                }

                // memes: first P * H -> starting hands, next P * R -> player reserves
                for (j, player) in players.iter().enumerate() {
                    // Hand
                    let hand_start = j * h as usize;
                    for k in 0..h as usize {
                        let meme_id = shuffled_memes[hand_start + k];
                        self.repo
                            .insert_hand_card(&mut tx, game_id, player.user_id, Some(meme_id), None)
                            .await?;
                        self.repo
                            .insert_content_lock(&mut tx, game_id, Some(meme_id), None)
                            .await?;
                    }

                    // Reserve
                    let reserve_start = (p * h) as usize + j * r as usize;
                    for k in 0..r as usize {
                        let meme_id = shuffled_memes[reserve_start + k];
                        self.repo
                            .insert_player_reserve(&mut tx, game_id, player.user_id, (k + 1) as i32, Some(meme_id), None)
                            .await?;
                        self.repo
                            .insert_content_lock(&mut tx, game_id, Some(meme_id), None)
                            .await?;
                    }
                }
            }
            GameMode::MemeToSituation => {
                let round_1_deadline = Utc::now() + chrono::Duration::seconds(game.submit_time_limit as i64);
                // R memes -> rounds
                for i in 1..=r {
                    let prompt_meme_id = shuffled_memes[(i - 1) as usize];
                    let phase = if i == 1 { RoundPhase::Submitting } else { RoundPhase::Waiting };
                    let phase_expires_at = if i == 1 { Some(round_1_deadline) } else { None };
                    let round = self.repo
                        .insert_round(&mut tx, game_id, i, None, Some(prompt_meme_id), phase, phase_expires_at)
                        .await?;

                    if i == 1 {
                        round_1_details = Some(round);
                    }

                    // lock meme
                    self.repo
                        .insert_content_lock(&mut tx, game_id, Some(prompt_meme_id), None)
                        .await?;
                }

                // situations: first P * H -> starting hands, next P * R -> player reserves
                for (j, player) in players.iter().enumerate() {
                    // Hand
                    let hand_start = j * h as usize;
                    for k in 0..h as usize {
                        let situation_id = shuffled_situations[hand_start + k];
                        self.repo
                            .insert_hand_card(&mut tx, game_id, player.user_id, None, Some(situation_id))
                            .await?;
                        self.repo
                            .insert_content_lock(&mut tx, game_id, None, Some(situation_id))
                            .await?;
                    }

                    // Reserve
                    let reserve_start = (p * h) as usize + j * r as usize;
                    for k in 0..r as usize {
                        let situation_id = shuffled_situations[reserve_start + k];
                        self.repo
                            .insert_player_reserve(&mut tx, game_id, player.user_id, (k + 1) as i32, None, Some(situation_id))
                            .await?;
                        self.repo
                            .insert_content_lock(&mut tx, game_id, None, Some(situation_id))
                            .await?;
                    }
                }
            }
        }

        // 7. Increment version
        let new_version = self.repo.increment_game_version(&mut tx, game_id).await?;

        // 8. Publish GameStarted and RoundStarted events
        let round1 = round_1_details.ok_or(AppError::InternalError)?;

        // Insert GameStarted into event store
        self.repo
            .insert_game_event(
                &mut tx,
                Uuid::new_v4(),
                game_id,
                new_version,
                "GameStarted",
                serde_json::json!({
                    "rounds_count": game.max_rounds,
                    "hand_size": game.hand_size,
                    "current_round_number": 1,
                    "phase": "submitting",
                    "players": players.iter().map(|p| serde_json::json!({
                        "player_id": p.user_id,
                        "handle": p.handle
                    })).collect::<Vec<_>>()
                }),
            )
            .await?;

        self.notification_sender
            .notify_game_started(&mut tx, game_id, game.max_rounds, game.hand_size, players.clone(), new_version)
            .await?;

        self.notification_sender
            .notify_lobby_removed(&mut tx, game_id)
            .await?;

        let prompt_kind = match game.mode {
            GameMode::SituationToMeme => "situation".to_string(),
            GameMode::MemeToSituation => "meme".to_string(),
        };
        let prompt_id = match game.mode {
            GameMode::SituationToMeme => round1.prompt_situation_id.unwrap(),
            GameMode::MemeToSituation => round1.prompt_meme_id.unwrap(),
        };
        let (prompt_media_id, prompt_text) = self
            .repo
            .get_prompt_details(&mut tx, &prompt_kind, prompt_id)
            .await?;

        let round_1_deadline = round1.phase_expires_at.unwrap_or_else(|| Utc::now() + chrono::Duration::seconds(game.submit_time_limit as i64));
        self.notification_sender
            .notify_round_started(&mut tx, game_id, round1.id, 1, prompt_kind, prompt_media_id, prompt_text, round_1_deadline, new_version)
            .await?;

        // For each player, send hand updated event
        for player in &players {
            let cards = self.repo.get_player_hand_with_media(&mut tx, game_id, player.user_id).await?;
            self.notification_sender
                .notify_hand_updated(&mut tx, game_id, player.user_id, round1.id, cards, new_version)
                .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

