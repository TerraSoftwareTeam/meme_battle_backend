use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::game::domain::model::{
    Game, GameCard, GameMode, GameStatus, PlayerSubmissionState, RoundPhase, ContentSafetyLevel,
    MemePack, PackMemeDetails, SituationPack, PackSituation,
};

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct GameDto {
    pub id: Uuid,
    pub mode: GameMode,
    pub status: GameStatus,
    pub version: i64,
}

impl From<Game> for GameDto {
    fn from(game: Game) -> Self {
        Self {
            id: game.id,
            mode: game.mode,
            status: game.status,
            version: game.version,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct RoundDto {
    pub id: Uuid,
    pub round_number: i32,
    pub phase: RoundPhase,
    pub prompt: Option<GameCard>,
    pub phase_expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct PlayerDto {
    pub user_id: Uuid,
    pub score: i32,
    pub is_ready: bool,
    pub has_submitted: bool,
}

impl From<PlayerSubmissionState> for PlayerDto {
    fn from(p: PlayerSubmissionState) -> Self {
        Self {
            user_id: p.user_id,
            score: p.score,
            is_ready: p.is_ready,
            has_submitted: p.has_submitted,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct GameStateDto {
    pub game: GameDto,
    pub round: Option<RoundDto>,
    pub players: Vec<PlayerDto>,
    pub my_hand: Vec<GameCard>,
}

fn default_max_rounds() -> i32 {
    3
}

fn default_hand_size() -> i32 {
    5
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateGameRequest {
    pub mode: GameMode,
    #[serde(rename = "selected_situation_pack_ids", alias = "situation_pack_ids")]
    pub selected_situation_pack_ids: Vec<Uuid>,
    #[serde(rename = "selected_meme_pack_ids", alias = "meme_pack_ids")]
    pub selected_meme_pack_ids: Vec<Uuid>,
    #[serde(default = "default_max_rounds")]
    pub max_rounds: i32,
    #[serde(default = "default_hand_size")]
    pub hand_size: i32,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct UpdateGameRequest {
    pub mode: Option<GameMode>,
    #[serde(default, rename = "selected_situation_pack_ids", alias = "situation_pack_ids")]
    pub selected_situation_pack_ids: Option<Vec<Uuid>>,
    #[serde(default, rename = "selected_meme_pack_ids", alias = "meme_pack_ids")]
    pub selected_meme_pack_ids: Option<Vec<Uuid>>,
    pub max_rounds: Option<i32>,
    pub hand_size: Option<i32>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct SubmitCardRequest {
    pub card_id: Uuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct VoteRequest {
    pub submission_id: Uuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct ReadyRequest {
    pub is_ready: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateMemePackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub media_ids: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateMemePackResponse {
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct UpdateMemePackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct AddMemesToPackRequest {
    pub media_ids: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct MemePackDto {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub created_at: String,
}

impl From<MemePack> for MemePackDto {
    fn from(p: MemePack) -> Self {
        Self {
            id: p.id,
            author_id: p.author_id,
            name: p.name,
            description: p.description,
            language_code: p.language_code,
            safety_level: p.safety_level,
            is_public: p.is_public,
            created_at: p.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct PackMemeDetailsDto {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub media_id: Option<i64>,
    pub media_url: String,
}

impl From<PackMemeDetails> for PackMemeDetailsDto {
    fn from(m: PackMemeDetails) -> Self {
        Self {
            id: m.id,
            pack_id: m.pack_id,
            media_id: m.media_id,
            media_url: m.media_url,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct MemePackDetailsResponse {
    pub pack: MemePackDto,
    pub memes: Vec<PackMemeDetailsDto>,
}

// Situation packs
#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateSituationPackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub prompts: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateSituationPackResponse {
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct UpdateSituationPackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct AddSituationsToPackRequest {
    pub prompts: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct SituationPackDto {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: String,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub created_at: String,
}

impl From<SituationPack> for SituationPackDto {
    fn from(p: SituationPack) -> Self {
        Self {
            id: p.id,
            author_id: p.author_id,
            name: p.name,
            description: p.description,
            language_code: p.language_code,
            safety_level: p.safety_level,
            is_public: p.is_public,
            created_at: p.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct PackSituationDto {
    pub id: Uuid,
    pub pack_id: Uuid,
    pub prompt_text: String,
}

impl From<PackSituation> for PackSituationDto {
    fn from(s: PackSituation) -> Self {
        Self {
            id: s.id,
            pack_id: s.pack_id,
            prompt_text: s.prompt_text,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct SituationPackDetailsResponse {
    pub pack: SituationPackDto,
    pub situations: Vec<PackSituationDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct WsTokenDto {
    pub connection_token: String,
    pub game_subscription_token: String,
    pub personal_subscription_token: String,
}

impl From<crate::features::game::WsTokenResult> for WsTokenDto {
    fn from(res: crate::features::game::WsTokenResult) -> Self {
        Self {
            connection_token: res.connection_token,
            game_subscription_token: res.game_subscription_token,
            personal_subscription_token: res.personal_subscription_token,
        }
    }
}


