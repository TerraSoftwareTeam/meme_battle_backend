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
        media::GetMediaAssetUrlQuery,
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
    get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
}

impl GameNotificationSenderAdapter {
    pub fn new(
        publish_usecase: Arc<PublishNotificationCommand>,
        get_media_asset_url: Arc<GetMediaAssetUrlQuery>,
    ) -> Self {
        Self {
            publish_usecase,
            get_media_asset_url,
        }
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
        prompt_media_id: Option<i64>,
        prompt_text: Option<String>,
        phase_expires_at: chrono::DateTime<chrono::Utc>,
        version: i64,
    ) -> Result<(), AppError> {
        let prompt_content = if let Some(media_id) = prompt_media_id {
            self.get_media_asset_url.execute(media_id).await?.unwrap_or_default()
        } else {
            prompt_text.unwrap_or_default()
        };

        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::RoundStarted(RoundStartedPayload {
            round_id,
            round_number,
            phase: "submitting".to_string(),
            prompt_kind,
            prompt_content,
            phase_expires_at,
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
        let mut cards_dto = Vec::new();
        for card in cards {
            let image_url = if let Some(media_id) = card.media_id {
                self.get_media_asset_url.execute(media_id).await?
            } else {
                None
            };
            cards_dto.push(HandCardDto {
                id: card.id,
                kind: card.kind,
                image_url,
                text: card.text,
            });
        }
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
        phase_expires_at: Option<chrono::DateTime<chrono::Utc>>,
        version: i64,
    ) -> Result<(), AppError> {
        let channel = format!("game:{}", game_id);
        let payload = RealtimePayload::RoundPhaseChanged(RoundPhaseChangedPayload {
            round_id,
            phase,
            phase_expires_at,
        });
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
        round_scoreboard: Vec<(Uuid, i32)>,
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
        let round_scoreboard_dto = round_scoreboard
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
            round_scoreboard: round_scoreboard_dto,
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
