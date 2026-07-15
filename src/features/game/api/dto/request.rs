use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::features::game::domain::model::{
    GameMode, ContentSafetyLevel, LanguageCode,
};

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
    #[serde(default)]
    #[schema(nullable, example = json!(null))]
    pub handle: Option<String>,
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
    pub language_code: LanguageCode,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub media_ids: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct UpdateMemePackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct AddMemesToPackRequest {
    pub media_ids: Vec<i64>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct CreateSituationPackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
    pub prompts: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct UpdateSituationPackRequest {
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
    pub safety_level: ContentSafetyLevel,
    pub is_public: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct AddSituationsToPackRequest {
    pub prompts: Vec<String>,
}

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema)]
pub struct JoinGameRequest {
    #[serde(default)]
    #[schema(nullable, example = json!(null))]
    pub handle: Option<String>,
}
