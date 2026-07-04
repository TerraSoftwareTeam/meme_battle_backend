use async_trait::async_trait;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
use crate::common::http::error::AppError;

#[async_trait]
pub trait GameNotificationSender: Send + Sync {
    async fn notify_player_joined(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        players_count: i32,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_player_ready_changed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        is_ready: bool,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_game_started(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        rounds_count: i32,
        hand_size: i32,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_round_started(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        round_number: i32,
        prompt_kind: String,
        prompt_id: Uuid,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_hand_updated(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        round_id: Uuid,
        cards: Vec<crate::features::game::GamePlayerHandCardWithMedia>,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_submission_received(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        user_id: Uuid,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_round_phase_changed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        phase: String,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_vote_received(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        voter_id: Uuid,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_round_finished(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        round_number: i32,
        winner_user_id: Uuid,
        scoreboard: Vec<(Uuid, i32)>,
        version: i64,
    ) -> Result<(), AppError>;

    async fn notify_game_finished(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        winner_user_id: Uuid,
        final_scoreboard: Vec<(Uuid, i32)>,
        version: i64,
    ) -> Result<(), AppError>;
}
