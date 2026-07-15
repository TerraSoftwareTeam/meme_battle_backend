use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::game::domain::model::{
    Game, ActiveGame, GameCard, GameMode, GameStatus, PlayerSubmissionState, RoundPhase, ContentSafetyLevel, LanguageCode,
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
pub struct ActiveGameDto {
    pub id: Uuid,
    pub host_id: Uuid,
    pub mode: GameMode,
    pub max_rounds: i32,
    pub hand_size: i32,
    pub players_count: i32,
    pub created_at: String,
}

impl From<ActiveGame> for ActiveGameDto {
    fn from(game: ActiveGame) -> Self {
        Self {
            id: game.id,
            host_id: game.host_id,
            mode: game.mode,
            max_rounds: game.max_rounds,
            hand_size: game.hand_size,
            players_count: game.players_count,
            created_at: game.created_at.to_rfc3339(),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct ActiveGamesResponseDto {
    pub games: Vec<ActiveGameDto>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct LobbiesWsTokenDto {
    pub connection_token: String,
    pub lobbies_subscription_token: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct RoundSubmissionDto {
    pub id: Uuid,
    pub card: GameCard,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct RoundDto {
    pub id: Uuid,
    pub round_number: i32,
    pub phase: RoundPhase,
    pub prompt: Option<GameCard>,
    pub phase_expires_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub submissions: Option<Vec<RoundSubmissionDto>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub my_submission: Option<GameCard>,
    pub has_voted: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct PlayerDto {
    pub user_id: Uuid,
    pub score: i32,
    pub is_ready: bool,
    pub handle: String,
    pub has_submitted: bool,
}

impl From<PlayerSubmissionState> for PlayerDto {
    fn from(p: PlayerSubmissionState) -> Self {
        Self {
            user_id: p.user_id,
            score: p.score,
            is_ready: p.is_ready,
            handle: p.handle,
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

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateMemePackResponse {
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct MemePackDto {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
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

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateSituationPackResponse {
    pub id: Uuid,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct SituationPackDto {
    pub id: Uuid,
    pub author_id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
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
