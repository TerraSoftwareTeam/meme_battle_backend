use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        domain::ports::GameRepository,
        application::commands::game::ProcessTimeoutCommand,
    },
};

pub struct GameTimerWorker {
    repo: Arc<dyn GameRepository>,
    process_timeout: Arc<ProcessTimeoutCommand>,
}

impl GameTimerWorker {
    pub fn new(
        repo: Arc<dyn GameRepository>,
        process_timeout: Arc<ProcessTimeoutCommand>,
    ) -> Self {
        Self { repo, process_timeout }
    }

    pub fn start(self: Arc<Self>, mut shutdown_rx: tokio::sync::watch::Receiver<bool>) -> tokio::task::JoinHandle<()> {
        let worker_id = Uuid::new_v4();
        tracing::info!(worker_id = %worker_id, "Starting game timer background worker");
        tokio::spawn(async move {
            let mut backoff_secs = 1;
            loop {
                if *shutdown_rx.borrow() {
                    tracing::info!(worker_id = %worker_id, "Shutdown signal received, stopping game timer background worker");
                    break;
                }

                let self_clone = self.clone();
                let task_handle = tokio::spawn(async move {
                    self_clone.process_single_expired_round(worker_id).await
                });

                match task_handle.await {
                    Ok(Ok(true)) => {
                        backoff_secs = 1;
                        continue;
                    }
                    Ok(Ok(false)) => {
                        backoff_secs = 1;
                        tokio::select! {
                            _ = shutdown_rx.changed() => {
                                tracing::info!(worker_id = %worker_id, "Shutdown signal received during sleep, stopping game timer background worker");
                                break;
                            }
                            _ = tokio::time::sleep(Duration::from_millis(500)) => {}
                        }
                    }
                    Ok(Err(err)) => {
                        tracing::error!("Error claiming/processing expired round: {:?}", err);
                        backoff_secs = std::cmp::min(backoff_secs * 2, 16);
                        tokio::select! {
                            _ = shutdown_rx.changed() => {
                                break;
                            }
                            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                        }
                    }
                    Err(join_err) => {
                        if join_err.is_panic() {
                            tracing::error!("Panic detected in round processing task!");
                        } else {
                            tracing::error!("Round processing task joined with error: {:?}", join_err);
                        }
                        backoff_secs = std::cmp::min(backoff_secs * 2, 16);
                        tokio::select! {
                            _ = shutdown_rx.changed() => {
                                break;
                            }
                            _ = tokio::time::sleep(Duration::from_secs(backoff_secs)) => {}
                        }
                    }
                }
            }
        })
    }

    pub async fn process_single_expired_round(&self, worker_id: Uuid) -> Result<bool, AppError> {
        let now = chrono::Utc::now();
        // A claim lease lasts for 30 seconds. If a worker hasn't finished in 30 seconds (or crashed),
        // other workers can reclaim the round.
        let stale_timeout = now - chrono::Duration::seconds(30);

        if let Some(round) = self.repo.claim_next_expired_round(worker_id, now, stale_timeout).await? {
            tracing::info!(
                round_id = %round.id,
                game_id = %round.game_id,
                phase = ?round.phase,
                worker_id = %worker_id,
                "Successfully claimed expired round"
            );
            if let Err(err) = self.process_timeout.execute(round.id).await {
                tracing::error!(
                    round_id = %round.id,
                    error = ?err,
                    "Failed to process round timeout"
                );
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;
    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use uuid::Uuid;
    use sqlx::{Postgres, Transaction};
    use crate::features::game::domain::model::*;
    use crate::features::game::application::ports::game_notification_sender::GameNotificationSender;
    use crate::features::game::application::commands::game::ProcessTimeoutCommand;
    use tokio::sync::watch;

    struct MockGameRepository {
        claim_result: Arc<Mutex<Option<Result<Option<GameRound>, AppError>>>>,
    }

    #[async_trait]
    impl GameRepository for MockGameRepository {
        async fn claim_next_expired_round(
            &self,
            _worker_id: Uuid,
            _now: DateTime<Utc>,
            _stale_timeout: DateTime<Utc>,
        ) -> Result<Option<GameRound>, AppError> {
            let res = self.claim_result.lock().unwrap().take();
            match res {
                Some(r) => r,
                None => panic!("Simulated database query panic"),
            }
        }

        async fn find_game(&self, _game_id: Uuid) -> Result<Option<Game>, AppError> { todo!() }
        async fn find_active_lobby_games(&self) -> Result<Vec<ActiveGame>, AppError> { todo!() }
        async fn get_players(&self, _game_id: Uuid) -> Result<Vec<GamePlayer>, AppError> { todo!() }
        async fn get_player_hand(&self, _game_id: Uuid, _user_id: Uuid) -> Result<Vec<RawGameCard>, AppError> { todo!() }
        async fn get_player_hand_with_media(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid) -> Result<Vec<GamePlayerHandCardWithMedia>, AppError> { todo!() }
        async fn get_current_round(&self, _game_id: Uuid) -> Result<Option<GameRound>, AppError> { todo!() }
        async fn get_round(&self, _round_id: Uuid) -> Result<Option<GameRound>, AppError> { todo!() }
        async fn get_prompt_card(&self, _situation_id: Option<Uuid>, _meme_id: Option<Uuid>) -> Result<Option<RawGameCard>, AppError> { todo!() }
        async fn get_available_memes(&self, _game_id: Uuid) -> Result<Vec<Uuid>, AppError> { todo!() }
        async fn get_available_situations(&self, _game_id: Uuid) -> Result<Vec<Uuid>, AppError> { todo!() }
        async fn get_submission_by_id(&self, _submission_id: Uuid) -> Result<Option<RoundSubmission>, AppError> { todo!() }
        async fn get_players_with_submissions(&self, _game_id: Uuid, _round_id: Option<Uuid>) -> Result<Vec<PlayerSubmissionState>, AppError> { todo!() }
        async fn begin(&self) -> Result<Transaction<'static, Postgres>, AppError> { todo!() }
        async fn create_game(&self, _tx: &mut Transaction<'_, Postgres>, _host_id: Uuid, _mode: GameMode, _max_rounds: i32, _hand_size: i32) -> Result<Game, AppError> { todo!() }
        async fn add_selected_situation_pack(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _pack_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn add_selected_meme_pack(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _pack_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn find_game_for_update(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid) -> Result<Option<Game>, AppError> { todo!() }
        async fn increment_game_version(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid) -> Result<i64, AppError> { todo!() }
        async fn update_game_status(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _status: GameStatus) -> Result<(), AppError> { todo!() }
        async fn add_player(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _is_ready: bool) -> Result<(), AppError> { todo!() }
        async fn update_player_ready(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _is_ready: bool) -> Result<(), AppError> { todo!() }
        async fn get_round_for_update(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid) -> Result<Option<GameRound>, AppError> { todo!() }
        async fn insert_round(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_number: i32, _prompt_situation_id: Option<Uuid>, _prompt_meme_id: Option<Uuid>, _phase: RoundPhase, _phase_expires_at: Option<DateTime<Utc>>) -> Result<GameRound, AppError> { todo!() }
        async fn activate_next_round(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_number: i32, _phase_expires_at: Option<DateTime<Utc>>) -> Result<(), AppError> { todo!() }
        async fn check_player_hand_card(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _card_id: Uuid) -> Result<Option<GamePlayerHandCard>, AppError> { todo!() }
        async fn mark_card_used(&self, _tx: &mut Transaction<'_, Postgres>, _hand_card_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn insert_hand_card(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _meme_id: Option<Uuid>, _situation_id: Option<Uuid>) -> Result<(), AppError> { todo!() }
        async fn insert_submission(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid, _user_id: Uuid, _meme_id: Option<Uuid>, _situation_id: Option<Uuid>) -> Result<(), AppError> { todo!() }
        async fn get_submissions_count(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid) -> Result<i64, AppError> { todo!() }
        async fn update_round_phase(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid, _phase: RoundPhase, _phase_expires_at: Option<DateTime<Utc>>) -> Result<(), AppError> { todo!() }
        async fn update_round_winner_and_phase(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid, _winner_user_id: Option<Uuid>, _phase: RoundPhase) -> Result<(), AppError> { todo!() }
        async fn increment_player_score(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn check_player_voted(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid, _voter_id: Uuid) -> Result<bool, AppError> { todo!() }
        async fn insert_vote(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid, _voter_id: Uuid, _submission_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn get_votes_count(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid) -> Result<i64, AppError> { todo!() }
        async fn get_votes_by_submission(&self, _tx: &mut Transaction<'_, Postgres>, _round_id: Uuid) -> Result<Vec<(Uuid, i64)>, AppError> { todo!() }
        async fn get_round_scoreboard(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_id: Uuid) -> Result<Vec<(Uuid, i32)>, AppError> { todo!() }
        async fn get_prompt_details(&self, _tx: &mut Transaction<'_, Postgres>, _prompt_kind: &str, _prompt_id: Uuid) -> Result<(Option<i64>, Option<String>), AppError> { todo!() }
        async fn insert_game_event(&self, _tx: &mut Transaction<'_, Postgres>, _event_id: Uuid, _game_id: Uuid, _version: i64, _event_type: &str, _payload: serde_json::Value) -> Result<(), AppError> { todo!() }
        async fn update_game_current_round(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _new_round: i32) -> Result<(), AppError> { todo!() }
        async fn insert_meme_pack(&self, _tx: &mut Transaction<'_, Postgres>, _author_id: Uuid, _name: &str, _description: Option<&str>, _language_code: LanguageCode, _safety_level: ContentSafetyLevel, _is_public: bool) -> Result<Uuid, AppError> { todo!() }
        async fn insert_pack_meme(&self, _tx: &mut Transaction<'_, Postgres>, _pack_id: Uuid, _media_id: i64) -> Result<(), AppError> { todo!() }
        async fn find_meme_pack(&self, _pack_id: Uuid) -> Result<Option<MemePack>, AppError> { todo!() }
        async fn list_meme_packs(&self, _author_id: Uuid) -> Result<Vec<MemePack>, AppError> { todo!() }
        async fn list_user_meme_packs(&self, _author_id: Uuid) -> Result<Vec<MemePack>, AppError> { todo!() }
        async fn get_pack_memes_list(&self, _pack_id: Uuid) -> Result<Vec<PackMeme>, AppError> { todo!() }
        async fn update_meme_pack(&self, _tx: &mut Transaction<'_, Postgres>, _pack_id: Uuid, _name: &str, _description: Option<&str>, _language_code: LanguageCode, _safety_level: ContentSafetyLevel, _is_public: bool) -> Result<(), AppError> { todo!() }
        async fn delete_meme_pack(&self, _tx: &mut Transaction<'_, Postgres>, _pack_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn find_pack_meme_by_id(&self, _meme_id: Uuid) -> Result<Option<PackMeme>, AppError> { todo!() }
        async fn delete_pack_meme(&self, _tx: &mut Transaction<'_, Postgres>, _meme_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn insert_situation_pack(&self, _tx: &mut Transaction<'_, Postgres>, _author_id: Uuid, _name: &str, _description: Option<&str>, _language_code: LanguageCode, _safety_level: ContentSafetyLevel, _is_public: bool) -> Result<Uuid, AppError> { todo!() }
        async fn insert_pack_situation(&self, _tx: &mut Transaction<'_, Postgres>, _pack_id: Uuid, _prompt_text: &str) -> Result<Uuid, AppError> { todo!() }
        async fn find_situation_pack(&self, _pack_id: Uuid) -> Result<Option<SituationPack>, AppError> { todo!() }
        async fn list_situation_packs(&self, _author_id: Uuid) -> Result<Vec<SituationPack>, AppError> { todo!() }
        async fn list_user_situation_packs(&self, _author_id: Uuid) -> Result<Vec<SituationPack>, AppError> { todo!() }
        async fn get_pack_situations_list(&self, _pack_id: Uuid) -> Result<Vec<PackSituation>, AppError> { todo!() }
        async fn update_situation_pack(&self, _tx: &mut Transaction<'_, Postgres>, _pack_id: Uuid, _name: &str, _description: Option<&str>, _language_code: LanguageCode, _safety_level: ContentSafetyLevel, _is_public: bool) -> Result<(), AppError> { todo!() }
        async fn delete_situation_pack(&self, _tx: &mut Transaction<'_, Postgres>, _pack_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn find_pack_situation_by_id(&self, _situation_id: Uuid) -> Result<Option<PackSituation>, AppError> { todo!() }
        async fn delete_pack_situation(&self, _tx: &mut Transaction<'_, Postgres>, _situation_id: Uuid) -> Result<(), AppError> { todo!() }

        async fn start_game(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _started_at: chrono::DateTime<chrono::Utc>) -> Result<(), AppError> { todo!() }
        async fn insert_player_reserve(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _draw_order: i32, _meme_id: Option<Uuid>, _situation_id: Option<Uuid>) -> Result<(), AppError> { todo!() }
        async fn insert_content_lock(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _meme_id: Option<Uuid>, _situation_id: Option<Uuid>) -> Result<(), AppError> { todo!() }
        async fn draw_reserve_card(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _draw_order: i32) -> Result<(), AppError> { todo!() }
        async fn update_game_settings(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _mode: GameMode, _max_rounds: i32, _hand_size: i32) -> Result<(), AppError> { todo!() }
        async fn clear_selected_situation_packs(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn clear_selected_meme_packs(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn delete_game_content_locks(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid) -> Result<(), AppError> { todo!() }
        async fn is_meme_pack_locked(&self, _pack_id: Uuid) -> Result<bool, AppError> { todo!() }
        async fn is_situation_pack_locked(&self, _pack_id: Uuid) -> Result<bool, AppError> { todo!() }
        async fn is_meme_locked(&self, _meme_id: Uuid) -> Result<bool, AppError> { todo!() }
        async fn is_situation_locked(&self, _situation_id: Uuid) -> Result<bool, AppError> { todo!() }
        async fn get_unused_hand_cards(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid) -> Result<Vec<GamePlayerHandCard>, AppError> { todo!() }
    }

    struct MockGameNotificationSender;

    #[async_trait]
    impl GameNotificationSender for MockGameNotificationSender {
        async fn notify_player_joined(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _players_count: i32, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_player_ready_changed(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _is_ready: bool, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_game_started(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _rounds_count: i32, _hand_size: i32, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_round_started(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_id: Uuid, _round_number: i32, _prompt_kind: String, _prompt_media_id: Option<i64>, _prompt_text: Option<String>, _phase_expires_at: DateTime<Utc>, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_hand_updated(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _user_id: Uuid, _round_id: Uuid, _cards: Vec<crate::features::game::GamePlayerHandCardWithMedia>, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_submission_received(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_id: Uuid, _user_id: Uuid, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_round_phase_changed(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_id: Uuid, _phase: String, _phase_expires_at: Option<DateTime<Utc>>, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_vote_received(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_id: Uuid, _voter_id: Uuid, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_round_finished(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _round_id: Uuid, _round_number: i32, _winner_user_id: Uuid, _scoreboard: Vec<(Uuid, i32)>, _round_scoreboard: Vec<(Uuid, i32)>, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_game_finished(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _winner_user_id: Uuid, _scoreboard: Vec<(Uuid, i32)>, _version: i64) -> Result<(), AppError> { todo!() }
        async fn notify_lobby_created(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _host_id: Uuid, _mode: String, _max_rounds: i32, _hand_size: i32, _players_count: i32, _created_at: DateTime<Utc>) -> Result<(), AppError> { Ok(()) }
        async fn notify_lobby_updated(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid, _players_count: i32) -> Result<(), AppError> { Ok(()) }
        async fn notify_lobby_removed(&self, _tx: &mut Transaction<'_, Postgres>, _game_id: Uuid) -> Result<(), AppError> { Ok(()) }
    }

    #[tokio::test]
    async fn test_timer_worker_graceful_shutdown() {
        let repo = Arc::new(MockGameRepository {
            claim_result: Arc::new(Mutex::new(Some(Ok(None)))),
        });
        let sender = Arc::new(MockGameNotificationSender);
        let process_timeout = Arc::new(ProcessTimeoutCommand::new(repo.clone() as Arc<dyn GameRepository>, sender));
        let worker = Arc::new(GameTimerWorker::new(repo.clone() as Arc<dyn GameRepository>, process_timeout));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = worker.start(shutdown_rx);

        shutdown_tx.send(true).unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(500), handle).await;
        assert!(result.is_ok(), "Worker failed to shut down within timeout");
    }

    #[tokio::test]
    async fn test_timer_worker_backoff_on_db_error() {
        let repo = Arc::new(MockGameRepository {
            claim_result: Arc::new(Mutex::new(Some(Err(AppError::InternalError)))),
        });
        let sender = Arc::new(MockGameNotificationSender);
        let process_timeout = Arc::new(ProcessTimeoutCommand::new(repo.clone() as Arc<dyn GameRepository>, sender));
        let worker = Arc::new(GameTimerWorker::new(repo.clone() as Arc<dyn GameRepository>, process_timeout));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = worker.start(shutdown_rx);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        shutdown_tx.send(true).unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(500), handle).await;
        assert!(result.is_ok(), "Worker failed to shut down during backoff sleep");
    }

    #[tokio::test]
    async fn test_timer_worker_panic_safety() {
        let repo = Arc::new(MockGameRepository {
            claim_result: Arc::new(Mutex::new(None)),
        });
        let sender = Arc::new(MockGameNotificationSender);
        let process_timeout = Arc::new(ProcessTimeoutCommand::new(repo.clone() as Arc<dyn GameRepository>, sender));
        let worker = Arc::new(GameTimerWorker::new(repo.clone() as Arc<dyn GameRepository>, process_timeout));

        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let handle = worker.start(shutdown_rx);

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        shutdown_tx.send(true).unwrap();

        let result = tokio::time::timeout(std::time::Duration::from_millis(500), handle).await;
        assert!(result.is_ok(), "Worker main loop did not survive panicking task");
    }
}

