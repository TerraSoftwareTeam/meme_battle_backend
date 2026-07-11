pub mod api;
mod application;
mod domain;
mod infra;

// Re-export routes and Swagger spec
pub use api::routes::{game_routes, GameApiDoc};

// Re-export commands
pub use application::commands::{
    AddMemesToPackCommand, AddSituationsToPackCommand, CreateGameCommand, CreateMemePackCommand,
    CreateSituationPackCommand, DeleteMemePackCommand, DeletePackMemeCommand,
    DeletePackSituationCommand, DeleteSituationPackCommand, JoinGameCommand, ProcessTimeoutCommand,
    SetReadyCommand, StartGameCommand, SubmitCardCommand, UpdateGameCommand, UpdateMemePackCommand,
    UpdateSituationPackCommand, VoteCardCommand,
};

// Re-export queries
pub use application::queries::{
    get_game_state::{GameStateResult, GetGameStateQuery},
    get_ws_token::{GetWsTokenQuery, WsTokenResult},
    meme_pack_queries::{
        GetMemePackQuery, ListMemePacksQuery, ListUserMemePacksQuery, MemePackQueryResult,
    },
    situation_pack_queries::{
        GetSituationPackQuery, ListSituationPacksQuery, ListUserSituationPacksQuery,
        SituationPackQueryResult,
    },
    list_active_games::{ListActiveGamesQuery, ListActiveGamesResult},
};

// Re-export domain models & repo port
pub use domain::{
    model::{
        ContentSafetyLevel, Game, ActiveGame, GameCard, GameMode, GamePlayer, GamePlayerHandCard,
        GamePlayerHandCardWithMedia, GameRound, GameStatus, LanguageCode, MemePack, PackMeme,
        PackMemeDetails, PackSituation, PlayerSubmissionState, RawGameCard, RoundPhase,
        RoundSubmission, RoundVote, SituationPack,
    },
    ports::game_repository::GameRepository,
};

// Re-export application ports
pub use application::ports::game_media_manager::GameMediaManager;
pub use application::ports::game_notification_sender::GameNotificationSender;
pub use application::ports::game_token_generator::GameTokenGenerator;

// Re-export infrastructure adapter
pub use infra::adapters::GameRepositoryImpl;
pub use infra::timer_worker::GameTimerWorker;

use std::sync::Arc;

#[derive(Clone)]
pub struct GameState {
    pub create_game: Arc<CreateGameCommand>,
    pub join_game: Arc<JoinGameCommand>,
    pub set_ready: Arc<SetReadyCommand>,
    pub start_game: Arc<StartGameCommand>,
    pub update_game: Arc<UpdateGameCommand>,
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
    pub list_user_meme_packs: Arc<ListUserMemePacksQuery>,
    pub get_meme_pack: Arc<GetMemePackQuery>,
    pub list_situation_packs: Arc<ListSituationPacksQuery>,
    pub list_user_situation_packs: Arc<ListUserSituationPacksQuery>,
    pub get_situation_pack: Arc<GetSituationPackQuery>,
    pub get_ws_token: Arc<GetWsTokenQuery>,
    pub list_active_games: Arc<ListActiveGamesQuery>,
    pub process_timeout: Arc<ProcessTimeoutCommand>,
    pub timer_worker: Arc<GameTimerWorker>,
    pub media_manager: Arc<dyn GameMediaManager>,
}

impl GameState {
    pub fn new(
        create_game: Arc<CreateGameCommand>,
        join_game: Arc<JoinGameCommand>,
        set_ready: Arc<SetReadyCommand>,
        start_game: Arc<StartGameCommand>,
        update_game: Arc<UpdateGameCommand>,
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
        list_user_meme_packs: Arc<ListUserMemePacksQuery>,
        get_meme_pack: Arc<GetMemePackQuery>,
        list_situation_packs: Arc<ListSituationPacksQuery>,
        list_user_situation_packs: Arc<ListUserSituationPacksQuery>,
        get_situation_pack: Arc<GetSituationPackQuery>,
        get_ws_token: Arc<GetWsTokenQuery>,
        list_active_games: Arc<ListActiveGamesQuery>,
        process_timeout: Arc<ProcessTimeoutCommand>,
        timer_worker: Arc<GameTimerWorker>,
        media_manager: Arc<dyn GameMediaManager>,
    ) -> Self {
        Self {
            create_game,
            join_game,
            set_ready,
            start_game,
            update_game,
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
            list_user_meme_packs,
            get_meme_pack,
            list_situation_packs,
            list_user_situation_packs,
            get_situation_pack,
            get_ws_token,
            list_active_games,
            process_timeout,
            timer_worker,
            media_manager,
        }
    }
}
