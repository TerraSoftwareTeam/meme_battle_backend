use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, utoipa::ToSchema)]
#[sqlx(type_name = "content_safety_level", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ContentSafetyLevel {
    FamilyFriendly,
    Spicy,
    Explicit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, utoipa::ToSchema)]
#[sqlx(type_name = "game_status", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum GameStatus {
    Lobby,
    Playing,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, utoipa::ToSchema)]
#[sqlx(type_name = "round_phase", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum RoundPhase {
    Submitting,
    Voting,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize, utoipa::ToSchema)]
#[sqlx(type_name = "game_mode", rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum GameMode {
    SituationToMeme,
    MemeToSituation,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum GameCard {
    Meme { id: Uuid, media_url: String },
    Situation { id: Uuid, prompt_text: String },
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PlayerSubmissionState {
    pub user_id: Uuid,
    pub score: i32,
    pub is_ready: bool,
    pub has_submitted: bool,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct Game {
    pub id: Uuid,
    pub host_id: Uuid,
    pub mode: GameMode,
    pub status: GameStatus,
    /// Maximum number of rounds before the game ends.
    pub max_rounds: i32,
    /// Index of the current round (0 = none started yet).
    pub current_round: i32,
    pub version: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GamePlayer {
    pub game_id: Uuid,
    pub user_id: Uuid,
    pub score: i32,
    pub is_ready: bool,
    pub joined_at: DateTime<Utc>,
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
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct GamePlayerHandCard {
    pub id: Uuid,
    pub game_id: Uuid,
    pub user_id: Uuid,
    pub meme_id: Option<Uuid>,
    pub situation_id: Option<Uuid>,
    pub is_used: bool,
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

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct MemePack {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PackMeme {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub media_id: Option<i64>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PackMemeDetails {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub media_id: Option<i64>,
    pub media_url: String,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct SituationPack {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, sqlx::FromRow, Serialize, Deserialize, utoipa::ToSchema)]
pub struct PackSituation {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub prompt_text: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Event Sourcing types
// ─────────────────────────────────────────────────────────────────────────────

/// All domain events produced during a game lifecycle.
/// Each variant carries the data needed to reconstruct in-memory state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "PascalCase")]
pub enum GameEvent {
    VoteRegistered {
        round_id: Uuid,
        voter_id: Uuid,
    },
    RoundFinished {
        round_id: Uuid,
        winner_user_id: Option<Uuid>,
        /// Per-player (user_id, new_score) snapshots after the round
        scores: Vec<(Uuid, i32)>,
    },
    GameFinished {
        final_scores: Vec<(Uuid, i32)>,
    },
}

impl GameEvent {
    pub fn event_type(&self) -> &'static str {
        match self {
            GameEvent::VoteRegistered { .. } => "VoteRegistered",
            GameEvent::RoundFinished { .. } => "RoundFinished",
            GameEvent::GameFinished { .. } => "GameFinished",
        }
    }
}

/// In-memory projection of the `games` aggregate.
///
/// Load from the DB read-model with [`GameAggregate::from_game`], then call
/// [`apply_events`] to advance the projection without additional queries.
#[derive(Debug, Clone)]
pub struct GameAggregate {
    pub id: Uuid,
    pub status: GameStatus,
    pub max_rounds: i32,
    pub current_round: i32,
    /// The **committed** version from the DB — used for Optimistic Concurrency Check.
    pub version: i64,
}

impl GameAggregate {
    pub fn from_game(game: &Game) -> Self {
        Self {
            id: game.id,
            status: game.status,
            max_rounds: game.max_rounds,
            current_round: game.current_round,
            version: game.version,
        }
    }

    /// Fold a slice of freshly-produced domain events onto the aggregate.
    ///
    /// Rules:
    /// - Every event increments `version` by 1 (mirrors `increment_game_version`).
    /// - `RoundFinished` also increments `current_round`.
    /// - `GameFinished` marks `status` as `Finished`.
    pub fn apply_events(&mut self, events: &[GameEvent]) {
        for event in events {
            self.version += 1;
            match event {
                GameEvent::RoundFinished { .. } => {
                    self.current_round += 1;
                }
                GameEvent::GameFinished { .. } => {
                    self.status = GameStatus::Finished;
                }
                GameEvent::VoteRegistered { .. } => {}
            }
        }
    }

    /// Returns `true` when `current_round` has reached `max_rounds`,
    /// meaning the *next* `RoundFinished` event should trigger `GameFinished`.
    pub fn is_last_round(&self) -> bool {
        self.current_round >= self.max_rounds
    }
}
