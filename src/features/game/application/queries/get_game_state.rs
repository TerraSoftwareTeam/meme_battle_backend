use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::domain::{
        model::{Game, GameCard, GameRound, PlayerSubmissionState},
        ports::GameRepository,
    },
};

pub struct GameStateResult {
    pub game: Game,
    pub round: Option<GameRound>,
    pub prompt: Option<GameCard>,
    pub players: Vec<PlayerSubmissionState>,
    pub my_hand: Vec<GameCard>,
}

pub struct GetGameStateQuery {
    repo: Arc<dyn GameRepository>,
}

impl GetGameStateQuery {
    pub fn new(repo: Arc<dyn GameRepository>) -> Self {
        Self { repo }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid) -> Result<GameStateResult, AppError> {
        let game = self.repo
            .find_game(game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        let current_round = self.repo.get_current_round(game_id).await?;
        let round_id = current_round.as_ref().map(|r| r.id);

        let players = self.repo.get_players_with_submissions(game_id, round_id).await?;
        let my_hand = self.repo.get_player_hand(game_id, user_id).await?;

        let prompt = match &current_round {
            Some(round) => {
                self.repo
                    .get_prompt_card(round.prompt_situation_id, round.prompt_meme_id)
                    .await?
            }
            None => None,
        };

        Ok(GameStateResult {
            game,
            round: current_round,
            prompt,
            players,
            my_hand,
        })
    }
}
