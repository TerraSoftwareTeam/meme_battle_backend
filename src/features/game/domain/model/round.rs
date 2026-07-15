use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, utoipa::ToSchema,
)]
#[sqlx(type_name = "round_phase", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RoundPhase {
    Waiting,
    Submitting,
    Voting,
    Finished,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlayerSubmissionState {
    pub user_id: Uuid,
    pub score: i32,
    pub is_ready: bool,
    pub handle: String,
    pub has_submitted: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GameRound {
    pub id: Uuid,
    pub game_id: Uuid,
    pub round_number: i32,
    pub prompt_situation_id: Option<Uuid>,
    pub prompt_meme_id: Option<Uuid>,
    pub phase: RoundPhase,
    pub winner_user_id: Option<Uuid>,
    pub phase_expires_at: Option<DateTime<Utc>>,
    pub claimed_at: Option<DateTime<Utc>>,
    pub claimed_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoundSubmission {
    pub id: Uuid,
    pub round_id: Uuid,
    pub user_id: Uuid,
    pub submission_meme_id: Option<Uuid>,
    pub submission_situation_id: Option<Uuid>,
    pub submitted_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RoundVote {
    pub round_id: Uuid,
    pub voter_id: Uuid,
    pub submission_id: Uuid,
    pub created_at: DateTime<Utc>,
}
