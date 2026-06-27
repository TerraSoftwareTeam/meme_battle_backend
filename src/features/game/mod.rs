pub mod api;
mod application;
mod domain;
mod infra;

// Re-export routes and Swagger spec
pub use api::routes::{game_routes, GameApiDoc};

// Re-export commands
pub use application::commands::{
    create_game::CreateGameCommand, join_game::JoinGameCommand, set_ready::SetReadyCommand,
    start_game::StartGameCommand, submit_card::SubmitCardCommand, vote_card::VoteCardCommand,
    create_meme_pack::CreateMemePackCommand,
    meme_pack_commands::{UpdateMemePackCommand, DeleteMemePackCommand, AddMemesToPackCommand, DeletePackMemeCommand},
    situation_pack_commands::{CreateSituationPackCommand, UpdateSituationPackCommand, DeleteSituationPackCommand, AddSituationsToPackCommand, DeletePackSituationCommand},
};

// Re-export queries
pub use application::queries::{
    get_game_state::{GetGameStateQuery, GameStateResult},
    meme_pack_queries::{ListMemePacksQuery, GetMemePackQuery, MemePackQueryResult},
    situation_pack_queries::{ListSituationPacksQuery, GetSituationPackQuery, SituationPackQueryResult},
};

// Re-export domain models & repo port
pub use domain::{
    model::{
        ContentSafetyLevel, Game, GameCard, GameMode, GamePlayer, GamePlayerHandCard, GameRound,
        GameStatus, PlayerSubmissionState, RoundPhase, RoundSubmission, RoundVote,
        MemePack, PackMeme, PackMemeDetails, SituationPack, PackSituation,
    },
    ports::game_repository::GameRepository,
};

// Re-export infrastructure adapter
pub use infra::adapters::game_repository_impl::GameRepositoryImpl;

use std::sync::Arc;

#[derive(Clone)]
pub struct GameState {
    pub create_game: Arc<CreateGameCommand>,
    pub join_game: Arc<JoinGameCommand>,
    pub set_ready: Arc<SetReadyCommand>,
    pub start_game: Arc<StartGameCommand>,
    pub submit_card: Arc<SubmitCardCommand>,
    pub vote_card: Arc<VoteCardCommand>,
    pub get_game_state: Arc<GetGameStateQuery>,
    pub create_meme_pack: Arc<CreateMemePackCommand>,
    pub update_meme_pack: Arc<UpdateMemePackCommand>,
    pub delete_meme_pack: Arc<DeleteMemePackCommand>,
    pub add_memes_to_pack: Arc<AddMemesToPackCommand>,
    pub delete_pack_meme: Arc<DeletePackMemeCommand>,
    pub create_situation_pack: Arc<CreateSituationPackCommand>,
    pub update_situation_pack: Arc<UpdateSituationPackCommand>,
    pub delete_situation_pack: Arc<DeleteSituationPackCommand>,
    pub add_situations_to_pack: Arc<AddSituationsToPackCommand>,
    pub delete_pack_situation: Arc<DeletePackSituationCommand>,
    pub list_meme_packs: Arc<ListMemePacksQuery>,
    pub get_meme_pack: Arc<GetMemePackQuery>,
    pub list_situation_packs: Arc<ListSituationPacksQuery>,
    pub get_situation_pack: Arc<GetSituationPackQuery>,
}

impl GameState {
    pub fn new(
        create_game: Arc<CreateGameCommand>,
        join_game: Arc<JoinGameCommand>,
        set_ready: Arc<SetReadyCommand>,
        start_game: Arc<StartGameCommand>,
        submit_card: Arc<SubmitCardCommand>,
        vote_card: Arc<VoteCardCommand>,
        get_game_state: Arc<GetGameStateQuery>,
        create_meme_pack: Arc<CreateMemePackCommand>,
        update_meme_pack: Arc<UpdateMemePackCommand>,
        delete_meme_pack: Arc<DeleteMemePackCommand>,
        add_memes_to_pack: Arc<AddMemesToPackCommand>,
        delete_pack_meme: Arc<DeletePackMemeCommand>,
        create_situation_pack: Arc<CreateSituationPackCommand>,
        update_situation_pack: Arc<UpdateSituationPackCommand>,
        delete_situation_pack: Arc<DeleteSituationPackCommand>,
        add_situations_to_pack: Arc<AddSituationsToPackCommand>,
        delete_pack_situation: Arc<DeletePackSituationCommand>,
        list_meme_packs: Arc<ListMemePacksQuery>,
        get_meme_pack: Arc<GetMemePackQuery>,
        list_situation_packs: Arc<ListSituationPacksQuery>,
        get_situation_pack: Arc<GetSituationPackQuery>,
    ) -> Self {
        Self {
            create_game,
            join_game,
            set_ready,
            start_game,
            submit_card,
            vote_card,
            get_game_state,
            create_meme_pack,
            update_meme_pack,
            delete_meme_pack,
            add_memes_to_pack,
            delete_pack_meme,
            create_situation_pack,
            update_situation_pack,
            delete_situation_pack,
            add_situations_to_pack,
            delete_pack_situation,
            list_meme_packs,
            get_meme_pack,
            list_situation_packs,
            get_situation_pack,
        }
    }
}
