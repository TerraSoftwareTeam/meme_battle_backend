use axum::{
    routing::{get, post, delete, patch},
    Router,
};
use utoipa::{
    openapi::security::{HttpAuthScheme, HttpBuilder, SecurityScheme},
    OpenApi,
};

use super::handlers;
use crate::{
    common::app::state::AppState,
    features::game::{
        api::dto::{
            CreateGameRequest, UpdateGameRequest, GameDto, GameStateDto, PlayerDto, ReadyRequest, RoundDto,
            SubmitCardRequest, VoteRequest, CreateMemePackRequest, CreateMemePackResponse,
            UpdateMemePackRequest, AddMemesToPackRequest, MemePackDto, PackMemeDetailsDto,
            MemePackDetailsResponse, CreateSituationPackRequest, CreateSituationPackResponse,
            UpdateSituationPackRequest, AddSituationsToPackRequest, SituationPackDto,
            PackSituationDto, SituationPackDetailsResponse, WsTokenDto,
        },
        domain::model::{GameCard, GameMode, GameStatus, RoundPhase},
    },
};

pub fn game_routes() -> Router<AppState> {
    Router::new()
        .route("/", post(handlers::create_game))
        .route("/{id}", patch(handlers::update_game))
        .route("/packs/memes", post(handlers::create_meme_pack).get(handlers::list_meme_packs))
        .route("/packs/memes/me", get(handlers::list_user_meme_packs))
        .route("/packs/memes/{id}", get(handlers::get_meme_pack).patch(handlers::update_meme_pack).delete(handlers::delete_meme_pack))
        .route("/packs/memes/{id}/memes", post(handlers::add_memes_to_pack))
        .route("/packs/memes/{id}/memes/{meme_id}", delete(handlers::delete_pack_meme))
        .route("/packs/situations", post(handlers::create_situation_pack).get(handlers::list_situation_packs))
        .route("/packs/situations/me", get(handlers::list_user_situation_packs))
        .route("/packs/situations/{id}", get(handlers::get_situation_pack).patch(handlers::update_situation_pack).delete(handlers::delete_situation_pack))
        .route("/packs/situations/{id}/situations", post(handlers::add_situations_to_pack))
        .route("/packs/situations/{id}/situations/{situation_id}", delete(handlers::delete_pack_situation))
        .route("/{id}/state", get(handlers::get_game_state))
        .route("/{id}/ws-token", get(handlers::get_ws_token))
        .route("/{id}/join", post(handlers::join_game))
        .route("/{id}/ready", post(handlers::set_ready))
        .route("/{id}/start", post(handlers::start_game_session))
        .route("/{id}/rounds/{round_id}/submit", post(handlers::submit_card))
        .route("/{id}/rounds/{round_id}/vote", post(handlers::vote_card))
}

#[derive(OpenApi)]
#[openapi(
    paths(
        handlers::create_game,
        handlers::update_game,
        handlers::get_game_state,
        handlers::get_ws_token,
        handlers::join_game,
        handlers::set_ready,
        handlers::start_game_session,
        handlers::submit_card,
        handlers::vote_card,
        handlers::create_meme_pack,
        handlers::list_meme_packs,
        handlers::list_user_meme_packs,
        handlers::get_meme_pack,
        handlers::update_meme_pack,
        handlers::delete_meme_pack,
        handlers::add_memes_to_pack,
        handlers::delete_pack_meme,
        handlers::create_situation_pack,
        handlers::list_situation_packs,
        handlers::list_user_situation_packs,
        handlers::get_situation_pack,
        handlers::update_situation_pack,
        handlers::delete_situation_pack,
        handlers::add_situations_to_pack,
        handlers::delete_pack_situation
    ),
    components(schemas(
        GameDto,
        RoundDto,
        PlayerDto,
        GameStateDto,
        GameCard,
        CreateGameRequest,
        UpdateGameRequest,
        SubmitCardRequest,
        VoteRequest,
        ReadyRequest,
        CreateMemePackRequest,
        CreateMemePackResponse,
        UpdateMemePackRequest,
        AddMemesToPackRequest,
        MemePackDto,
        PackMemeDetailsDto,
        MemePackDetailsResponse,
        CreateSituationPackRequest,
        CreateSituationPackResponse,
        UpdateSituationPackRequest,
        AddSituationsToPackRequest,
        SituationPackDto,
        PackSituationDto,
        SituationPackDetailsResponse,
        WsTokenDto,
        GameMode,
        GameStatus,
        RoundPhase
    )),
    tags(
        (name = "Games", description = "Meme Battle game endpoints"),
        (name = "Meme Packs", description = "Meme Pack management endpoints"),
        (name = "Situation Packs", description = "Situation Pack management endpoints")
    ),
    security(
        ("bearer_auth" = [])
    ),
    modifiers(&GameApiDoc)
)]
pub struct GameApiDoc;

impl utoipa::Modify for GameApiDoc {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.as_mut().unwrap();
        components.add_security_scheme(
            "bearer_auth",
            SecurityScheme::Http(
                HttpBuilder::new()
                    .scheme(HttpAuthScheme::Bearer)
                    .bearer_format("JWT")
                    .description(Some("Input your `<your-jwt>`"))
                    .build(),
            ),
        )
    }
}

