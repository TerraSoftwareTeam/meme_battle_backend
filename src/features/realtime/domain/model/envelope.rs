use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RealtimeEventType {
    PlayerJoined,
    PlayerLeft,
    PlayerReadyChanged,
    GameStarted,
    RoundStarted,
    SubmissionReceived,
    RoundPhaseChanged,
    VoteReceived,
    RoundFinished,
    ScoreUpdated,
    GameFinished,
    GameCancelled,

    LobbyCreated,
    LobbyUpdated,
    LobbyRemoved,
    GameInvited,
    MatchmakingFound,
    HandUpdated,
    SubmissionAccepted,
    SubmissionRejected,
    VoteAccepted,
    VoteRejected,
    ResultPrivate,
    TurnReminder,
    SyncRequired,
    ErrorDomain,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RealtimeEnvelope {
    pub event_id: Uuid,
    pub event_type: RealtimeEventType,
    pub game_id: Uuid,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<Uuid>,
    pub occurred_at: DateTime<Utc>,
    pub version: i64,
    pub payload: RealtimePayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScoreItem {
    pub user_id: Uuid,
    pub score: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandCardDto {
    pub id: Uuid,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image_url: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerJoinedPayload {
    pub user_id: Uuid,
    pub players_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayerReadyChangedPayload {
    pub user_id: Uuid,
    pub is_ready: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundPhaseChangedPayload {
    pub round_id: Uuid,
    pub phase: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_expires_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteReceivedPayload {
    pub round_id: Uuid,
    pub voter_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameStartedPayload {
    pub rounds_count: i32,
    pub hand_size: i32,
    pub current_round_number: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundStartedPayload {
    pub round_id: Uuid,
    pub round_number: i32,
    pub phase: String,
    pub prompt_kind: String,
    pub prompt_content: String,
    pub phase_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionReceivedPayload {
    pub round_id: Uuid,
    pub user_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoundFinishedPayload {
    pub round_id: Uuid,
    pub round_number: i32,
    pub winner_user_id: Uuid,
    pub scoreboard: Vec<ScoreItem>,
    pub round_scoreboard: Vec<ScoreItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameFinishedPayload {
    pub winner_user_id: Uuid,
    pub final_scoreboard: Vec<ScoreItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandUpdatedPayload {
    pub round_id: Uuid,
    pub cards: Vec<HandCardDto>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionAcceptedPayload {
    pub round_id: Uuid,
    pub submission_id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubmissionRejectedPayload {
    pub round_id: Uuid,
    pub reason_code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncRequiredPayload {
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyCreatedPayload {
    pub id: Uuid,
    pub host_id: Uuid,
    pub mode: String,
    pub max_rounds: i32,
    pub hand_size: i32,
    pub players_count: i32,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyUpdatedPayload {
    pub id: Uuid,
    pub players_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LobbyRemovedPayload {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RealtimePayload {
    PlayerJoined(PlayerJoinedPayload),
    PlayerReadyChanged(PlayerReadyChangedPayload),
    RoundPhaseChanged(RoundPhaseChangedPayload),
    VoteReceived(VoteReceivedPayload),
    GameStarted(GameStartedPayload),
    RoundStarted(RoundStartedPayload),
    SubmissionReceived(SubmissionReceivedPayload),
    RoundFinished(RoundFinishedPayload),
    GameFinished(GameFinishedPayload),
    HandUpdated(HandUpdatedPayload),
    SubmissionAccepted(SubmissionAcceptedPayload),
    SubmissionRejected(SubmissionRejectedPayload),
    SyncRequired(SyncRequiredPayload),
    LobbyCreated(LobbyCreatedPayload),
    LobbyUpdated(LobbyUpdatedPayload),
    LobbyRemoved(LobbyRemovedPayload),
}

impl RealtimePayload {
    pub fn event_type(&self) -> RealtimeEventType {
        match self {
            RealtimePayload::PlayerJoined(_) => RealtimeEventType::PlayerJoined,
            RealtimePayload::PlayerReadyChanged(_) => RealtimeEventType::PlayerReadyChanged,
            RealtimePayload::RoundPhaseChanged(_) => RealtimeEventType::RoundPhaseChanged,
            RealtimePayload::VoteReceived(_) => RealtimeEventType::VoteReceived,
            RealtimePayload::GameStarted(_) => RealtimeEventType::GameStarted,
            RealtimePayload::RoundStarted(_) => RealtimeEventType::RoundStarted,
            RealtimePayload::SubmissionReceived(_) => RealtimeEventType::SubmissionReceived,
            RealtimePayload::RoundFinished(_) => RealtimeEventType::RoundFinished,
            RealtimePayload::GameFinished(_) => RealtimeEventType::GameFinished,
            RealtimePayload::HandUpdated(_) => RealtimeEventType::HandUpdated,
            RealtimePayload::SubmissionAccepted(_) => RealtimeEventType::SubmissionAccepted,
            RealtimePayload::SubmissionRejected(_) => RealtimeEventType::SubmissionRejected,
            RealtimePayload::SyncRequired(_) => RealtimeEventType::SyncRequired,
            RealtimePayload::LobbyCreated(_) => RealtimeEventType::LobbyCreated,
            RealtimePayload::LobbyUpdated(_) => RealtimeEventType::LobbyUpdated,
            RealtimePayload::LobbyRemoved(_) => RealtimeEventType::LobbyRemoved,
        }
    }
}

impl RealtimeEnvelope {
    pub fn all(
        game_id: Uuid,
        version: i64,
        payload: RealtimePayload,
    ) -> Self {
        let event_type = payload.event_type();
        Self {
            event_id: Uuid::new_v4(),
            event_type,
            game_id,
            user_id: None,
            occurred_at: Utc::now(),
            version,
            payload,
        }
    }

    pub fn personal(
        game_id: Uuid,
        user_id: Uuid,
        version: i64,
        payload: RealtimePayload,
    ) -> Self {
        let event_type = payload.event_type();
        Self {
            event_id: Uuid::new_v4(),
            event_type,
            game_id,
            user_id: Some(user_id),
            occurred_at: Utc::now(),
            version,
            payload,
        }
    }
}
