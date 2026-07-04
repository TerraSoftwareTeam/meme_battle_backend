use std::sync::Arc;
use async_trait::async_trait;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::{
        game::{
            GameNotificationSender,
            GamePlayerHandCardWithMedia,
        },
        realtime::{
            PublishNotificationCommand,
            model::{
                GameFinishedPayload, GameStartedPayload, HandCardDto, HandUpdatedPayload,
                PlayerJoinedPayload, PlayerReadyChangedPayload,
                RoundFinishedPayload, RoundPhaseChangedPayload, RoundStartedPayload, ScoreItem,
                SubmissionReceivedPayload, VoteReceivedPayload, RealtimePayload,
            },
        },
    },
};

pub struct GameNotificationSenderAdapter {
    publish_usecase: Arc<PublishNotificationCommand>,
}

impl GameNotificationSenderAdapter {
    pub fn new(publish_usecase: Arc<PublishNotificationCommand>) -> Self {
        Self { publish_usecase }
    }
}

#[async_trait]
impl GameNotificationSender for GameNotificationSenderAdapter {
    async fn notify_player_joined(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        players_count: i32,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::PlayerJoined(PlayerJoinedPayload { user_id, players_count });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_player_ready_changed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        is_ready: bool,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::PlayerReadyChanged(PlayerReadyChangedPayload { user_id, is_ready });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_game_started(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        rounds_count: i32,
        hand_size: i32,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::GameStarted(GameStartedPayload {
            rounds_count,
            hand_size,
            current_round_number: 1,
            phase: "submitting".to_string(),
        });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_round_started(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        round_number: i32,
        prompt_kind: String,
        prompt_id: Uuid,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::RoundStarted(RoundStartedPayload {
            round_id,
            round_number,
            phase: "submitting".to_string(),
            prompt_kind,
            prompt_id,
            submission_deadline_at: chrono::Utc::now() + chrono::Duration::seconds(45),
        });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_hand_updated(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        user_id: Uuid,
        round_id: Uuid,
        cards: Vec<GamePlayerHandCardWithMedia>,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("personal:#{}", user_id);
        let cards_dto = cards
            .into_iter()
            .map(|card| HandCardDto {
                kind: card.kind,
                id: card.id,
                media_id: card.media_id,
            })
            .collect::<Vec<_>>();
        let payload = RealtimePayload::HandUpdated(HandUpdatedPayload { round_id, cards: cards_dto });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, Some(user_id)).await
    }

    async fn notify_submission_received(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        user_id: Uuid,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::SubmissionReceived(SubmissionReceivedPayload { round_id, user_id });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_round_phase_changed(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        phase: String,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::RoundPhaseChanged(RoundPhaseChangedPayload { round_id, phase });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_vote_received(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        voter_id: Uuid,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::VoteReceived(VoteReceivedPayload { round_id, voter_id });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_round_finished(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        round_id: Uuid,
        round_number: i32,
        winner_user_id: Uuid,
        scoreboard: Vec<(Uuid, i32)>,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let scoreboard_dto = scoreboard
            .into_iter()
            .map(|(uid, s)| ScoreItem {
                user_id: uid,
                score: s,
            })
            .collect::<Vec<_>>();
        let payload = RealtimePayload::RoundFinished(RoundFinishedPayload {
            round_id,
            round_number,
            winner_user_id,
            scoreboard: scoreboard_dto,
        });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }

    async fn notify_game_finished(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        game_id: Uuid,
        winner_user_id: Uuid,
        final_scoreboard: Vec<(Uuid, i32)>,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let scoreboard_dto = final_scoreboard
            .into_iter()
            .map(|(uid, s)| ScoreItem {
                user_id: uid,
                score: s,
            })
            .collect::<Vec<_>>();
        let payload = RealtimePayload::GameFinished(GameFinishedPayload {
            winner_user_id,
            final_scoreboard: scoreboard_dto,
        });
        self.publish_usecase.execute(tx, game_id, &channel, version, payload, None).await
    }
}
