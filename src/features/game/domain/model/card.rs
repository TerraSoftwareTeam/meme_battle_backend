use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Serialize, Deserialize, Clone, Debug, utoipa::ToSchema, PartialEq, Eq)]
#[serde(tag = "type", content = "data")]
pub enum GameCard {
    Meme { id: Uuid, media_url: String },
    Situation { id: Uuid, prompt_text: String },
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

#[derive(Debug, Clone)]
pub struct GamePlayerHandCardWithMedia {
    pub id: Uuid,
    pub kind: String,
    pub media_id: Option<i64>,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub enum RawGameCard {
    Meme { id: Uuid, media_id: Option<i64> },
    Situation { id: Uuid, prompt_text: String },
}
