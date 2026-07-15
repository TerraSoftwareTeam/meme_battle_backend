use std::sync::Arc;
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::game::{
        domain::{
            model::{Game, GameCard, RawGameCard, GameRound, PlayerSubmissionState},
            ports::GameRepository,
        },
        application::ports::game_media_manager::GameMediaManager,
    },
};

pub struct GameStateSubmission {
    pub id: Uuid,
    pub card: GameCard,
}

pub struct GameStateResult {
    pub game: Game,
    pub round: Option<GameRound>,
    pub prompt: Option<GameCard>,
    pub players: Vec<PlayerSubmissionState>,
    pub my_hand: Vec<GameCard>,
    pub submissions: Option<Vec<GameStateSubmission>>,
    pub my_submission: Option<GameCard>,
    pub has_voted: bool,
}

pub struct GetGameStateQuery {
    repo: Arc<dyn GameRepository>,
    media_manager: Arc<dyn GameMediaManager>,
}

impl GetGameStateQuery {
    pub fn new(repo: Arc<dyn GameRepository>, media_manager: Arc<dyn GameMediaManager>) -> Self {
        Self { repo, media_manager }
    }

    pub async fn execute(&self, user_id: Uuid, game_id: Uuid) -> Result<GameStateResult, AppError> {
        let mut tx = self.repo.begin().await?;

        let game = self.repo
            .find_game(game_id)
            .await?
            .ok_or_else(|| AppError::NotFound(format!("Game not found: {}", game_id)))?;

        let current_round = self.repo.get_current_round(game_id).await?;
        let round_id = current_round.as_ref().map(|r| r.id);

        let players = self.repo.get_players_with_submissions(game_id, round_id).await?;
        let my_hand = self.repo.get_player_hand(game_id, user_id).await?;

        let mut resolved_my_hand = Vec::new();
        for card in my_hand {
            resolved_my_hand.push(self.resolve_card(card).await?);
        }

        let prompt = match &current_round {
            Some(round) => {
                if let Some(card) = self.repo
                    .get_prompt_card(round.prompt_situation_id, round.prompt_meme_id)
                    .await?
                {
                    Some(self.resolve_card(card).await?)
                } else {
                    None
                }
            }
            None => None,
        };

        // Populate new snapshot fields
        let mut submissions = None;
        let mut my_submission = None;
        let mut has_voted = false;

        if let Some(round) = &current_round {
            has_voted = self.repo.check_player_voted(&mut tx, round.id, user_id).await?;

            let round_subs = self.repo.get_round_submissions(&mut tx, round.id).await?;

            // Find current user's submission to display under my_submission
            if let Some(my_sub) = round_subs.iter().find(|s| s.user_id == user_id) {
                if let Some(meme_id) = my_sub.submission_meme_id {
                    let media_id = self.repo.find_pack_meme_by_id(meme_id).await?.map(|m| m.media_id).flatten();
                    my_submission = Some(self.resolve_card(RawGameCard::Meme { id: meme_id, media_id }).await?);
                } else if let Some(sit_id) = my_sub.submission_situation_id {
                    let prompt_text = self.repo.find_pack_situation_by_id(sit_id).await?.map(|s| s.prompt_text).unwrap_or_default();
                    my_submission = Some(self.resolve_card(RawGameCard::Situation { id: sit_id, prompt_text }).await?);
                }
            }

            // Populate all anonymized submissions for voting/results if in Voting/Finished phase
            use crate::features::game::domain::model::RoundPhase;
            if matches!(round.phase, RoundPhase::Voting | RoundPhase::Finished) {
                let mut resolved_subs = Vec::new();
                for sub in round_subs {
                    let card = if let Some(meme_id) = sub.submission_meme_id {
                        let media_id = self.repo.find_pack_meme_by_id(meme_id).await?.map(|m| m.media_id).flatten();
                        self.resolve_card(RawGameCard::Meme { id: meme_id, media_id }).await?
                    } else if let Some(sit_id) = sub.submission_situation_id {
                        let prompt_text = self.repo.find_pack_situation_by_id(sit_id).await?.map(|s| s.prompt_text).unwrap_or_default();
                        self.resolve_card(RawGameCard::Situation { id: sit_id, prompt_text }).await?
                    } else {
                        continue;
                    };
                    resolved_subs.push(GameStateSubmission {
                        id: sub.id,
                        card,
                    });
                }
                submissions = Some(resolved_subs);
            }
        }

        tx.commit().await?;

        Ok(GameStateResult {
            game,
            round: current_round,
            prompt,
            players,
            my_hand: resolved_my_hand,
            submissions,
            my_submission,
            has_voted,
        })
    }

    async fn resolve_card(&self, card: RawGameCard) -> Result<GameCard, AppError> {
        match card {
            RawGameCard::Meme { id, media_id } => {
                let media_url = if let Some(mid) = media_id {
                    self.media_manager.resolve_url(mid).await?.unwrap_or_default()
                } else {
                    "".to_string()
                };
                Ok(GameCard::Meme { id, media_url })
            }
            RawGameCard::Situation { id, prompt_text } => {
                Ok(GameCard::Situation { id, prompt_text })
            }
        }
    }
}
