use axum::{
    extract::{Path, State},
    response::IntoResponse,
    Json,
};
use uuid::Uuid;

use crate::{
    common::{
        app::state::AppState,
        http::{current_user::CurrentUser, dto::RestApiResponse, error::AppError},
    },
    features::game::{
        api::dto::{
            CreateGameRequest, UpdateGameRequest, ReadyRequest, SubmitCardRequest, VoteRequest, GameDto,
            ActiveGameDto, ActiveGamesResponseDto, GameStateDto, PlayerDto, RoundDto, CreateMemePackRequest, CreateMemePackResponse,
            UpdateMemePackRequest, AddMemesToPackRequest, MemePackDto, PackMemeDetailsDto,
            MemePackDetailsResponse, CreateSituationPackRequest, CreateSituationPackResponse,
            UpdateSituationPackRequest, AddSituationsToPackRequest, SituationPackDto,
            PackSituationDto, SituationPackDetailsResponse, WsTokenDto,
        },
        GameState,
    },
};

#[utoipa::path(
    post,
    path = "/games",
    request_body = CreateGameRequest,
    responses((status = 200, description = "Create a new game session", body = GameDto)),
    tag = "Games"
)]
pub async fn create_game(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Json(payload): Json<CreateGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let game = state
        .create_game
        .execute(
            user_id,
            payload.mode,
            payload.selected_situation_pack_ids,
            payload.selected_meme_pack_ids,
            payload.max_rounds,
            payload.hand_size,
        )
        .await?;

    Ok(RestApiResponse::success(GameDto::from(game)))
}

#[utoipa::path(
    get,
    path = "/games",
    responses((status = 200, description = "List active lobby games with WS subscription tokens", body = ActiveGamesResponseDto)),
    tag = "Games"
)]
pub async fn list_active_games(
    State(state): State<GameState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let result = state.list_active_games.execute(user_id).await?;
    let games_dtos: Vec<ActiveGameDto> = result.games.into_iter().map(ActiveGameDto::from).collect();

    Ok(RestApiResponse::success(ActiveGamesResponseDto {
        games: games_dtos,
        connection_token: result.connection_token,
        lobbies_subscription_token: result.lobbies_subscription_token,
    }))
}

#[utoipa::path(
    patch,
    path = "/games/{id}",
    request_body = UpdateGameRequest,
    responses((status = 200, description = "Update game settings", body = GameDto)),
    tag = "Games"
)]
pub async fn update_game(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
    Json(payload): Json<UpdateGameRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let game_id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;

    let game = state
        .update_game
        .execute(
            user_id,
            game_id,
            payload.mode,
            payload.selected_situation_pack_ids,
            payload.selected_meme_pack_ids,
            payload.max_rounds,
            payload.hand_size,
        )
        .await?;

    Ok(RestApiResponse::success(GameDto::from(game)))
}

#[utoipa::path(
    get,
    path = "/games/{id}/state",
    responses((status = 200, description = "Get current game state snapshot", body = GameStateDto)),
    tag = "Games"
)]
pub async fn get_game_state(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;

    let res = state.get_game_state.execute(user_id, id).await?;

    let round_dto = res.round.map(|round| RoundDto {
        id: round.id,
        round_number: round.round_number,
        phase: round.phase,
        prompt: res.prompt,
        phase_expires_at: round.phase_expires_at,
    });

    let state_dto = GameStateDto {
        game: GameDto::from(res.game),
        round: round_dto,
        players: res.players.into_iter().map(PlayerDto::from).collect(),
        my_hand: res.my_hand,
    };

    Ok(RestApiResponse::success(state_dto))
}

#[utoipa::path(
    get,
    path = "/games/{id}/ws-token",
    responses((status = 200, description = "Get Centrifugo WebSocket connection and subscription tokens", body = WsTokenDto)),
    tag = "Games"
)]
pub async fn get_ws_token(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;

    let res = state.get_ws_token.execute(user_id, id).await?;
    Ok(RestApiResponse::success(WsTokenDto::from(res)))
}

#[utoipa::path(
    post,
    path = "/games/{id}/join",
    responses((status = 200, description = "Join the game lobby")),
    tag = "Games"
)]
pub async fn join_game(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;

    state.game.join_game.execute(user_id, id).await?;

    Ok(RestApiResponse::success_with_message("Joined successfully".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/{id}/ready",
    request_body = ReadyRequest,
    responses((status = 200, description = "Update readiness status in lobby")),
    tag = "Games"
)]
pub async fn set_ready(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
    Json(payload): Json<ReadyRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;

    state.game.set_ready.execute(user_id, id, payload.is_ready).await?;

    Ok(RestApiResponse::success_with_message("Readiness updated".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/{id}/start",
    responses((status = 200, description = "Start the game")),
    tag = "Games"
)]
pub async fn start_game_session(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<RestApiResponse<()>, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;

    state.game.start_game.execute(user_id, id).await?;

    Ok(RestApiResponse::success_with_message("Game started successfully".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/{id}/rounds/{round_id}/submit",
    request_body = SubmitCardRequest,
    responses((status = 200, description = "Submit a card for the round")),
    tag = "Games"
)]
pub async fn submit_card(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path((id_str, round_id_str)): Path<(String, String)>,
    Json(payload): Json<SubmitCardRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;
    let round_id = Uuid::parse_str(&round_id_str)
        .map_err(|_| AppError::ValidationError("Invalid round ID".to_string()))?;

    state
        .game
        .submit_card
        .execute(user_id, id, round_id, payload.card_id)
        .await?;

    Ok(RestApiResponse::success_with_message("Card submitted successfully".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/{id}/rounds/{round_id}/vote",
    request_body = VoteRequest,
    responses((status = 200, description = "Vote for a submission")),
    tag = "Games"
)]
pub async fn vote_card(
    State(state): State<AppState>,
    current_user: CurrentUser,
    Path((id_str, round_id_str)): Path<(String, String)>,
    Json(payload): Json<VoteRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid game ID".to_string()))?;
    let round_id = Uuid::parse_str(&round_id_str)
        .map_err(|_| AppError::ValidationError("Invalid round ID".to_string()))?;

    state
        .game
        .vote_card
        .execute(user_id, id, round_id, payload.submission_id)
        .await?;

    Ok(RestApiResponse::success_with_message("Vote registered successfully".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/packs/memes",
    request_body = CreateMemePackRequest,
    responses((status = 200, description = "Create a new meme pack and attach images", body = CreateMemePackResponse)),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn create_meme_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Json(payload): Json<CreateMemePackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let pack_id = state
        .create_meme_pack
        .execute(
            user_id,
            payload.name,
            payload.description,
            payload.language_code,
            payload.safety_level,
            payload.is_public,
            payload.media_ids,
        )
        .await?;

    Ok(RestApiResponse::success(CreateMemePackResponse { id: pack_id }))
}

#[utoipa::path(
    get,
    path = "/games/packs/memes",
    responses((status = 200, description = "List all visible meme packs", body = [MemePackDto])),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn list_meme_packs(
    State(state): State<GameState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let packs = state.list_meme_packs.execute(user_id).await?;
    let dtos: Vec<MemePackDto> = packs.into_iter().map(MemePackDto::from).collect();

    Ok(RestApiResponse::success(dtos))
}

#[utoipa::path(
    get,
    path = "/games/packs/memes/{id}",
    responses((status = 200, description = "Get meme pack details", body = MemePackDetailsResponse)),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_meme_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    let res = state.get_meme_pack.execute(id, user_id).await?;

    let details = MemePackDetailsResponse {
        pack: MemePackDto::from(res.pack),
        memes: res.memes.into_iter().map(PackMemeDetailsDto::from).collect(),
    };

    Ok(RestApiResponse::success(details))
}

#[utoipa::path(
    patch,
    path = "/games/packs/memes/{id}",
    request_body = UpdateMemePackRequest,
    responses((status = 200, description = "Update meme pack metadata")),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_meme_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
    Json(payload): Json<UpdateMemePackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    state
        .update_meme_pack
        .execute(
            user_id,
            id,
            payload.name,
            payload.description,
            payload.language_code,
            payload.safety_level,
            payload.is_public,
        )
        .await?;

    Ok(RestApiResponse::success_with_message("Meme pack updated successfully".to_string(), ()))
}

#[utoipa::path(
    delete,
    path = "/games/packs/memes/{id}",
    responses((status = 200, description = "Delete a meme pack")),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_meme_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    state.delete_meme_pack.execute(user_id, id).await?;

    Ok(RestApiResponse::success_with_message("Meme pack deleted successfully".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/packs/memes/{id}/memes",
    request_body = AddMemesToPackRequest,
    responses((status = 200, description = "Add memes to an existing pack")),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn add_memes_to_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
    Json(payload): Json<AddMemesToPackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    state.add_memes_to_pack.execute(user_id, id, payload.media_ids).await?;

    Ok(RestApiResponse::success_with_message("Memes added to pack successfully".to_string(), ()))
}

#[utoipa::path(
    delete,
    path = "/games/packs/memes/{id}/memes/{meme_id}",
    responses((status = 200, description = "Remove a meme from a pack")),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_pack_meme(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path((_id_str, meme_id_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let meme_id = Uuid::parse_str(&meme_id_str)
        .map_err(|_| AppError::ValidationError("Invalid meme ID".to_string()))?;

    state.delete_pack_meme.execute(user_id, meme_id).await?;

    Ok(RestApiResponse::success_with_message("Meme removed from pack successfully".to_string(), ()))
}

// Situation Packs
#[utoipa::path(
    post,
    path = "/games/packs/situations",
    request_body = CreateSituationPackRequest,
    responses((status = 200, description = "Create a new situation pack", body = CreateSituationPackResponse)),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn create_situation_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Json(payload): Json<CreateSituationPackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let pack_id = state
        .create_situation_pack
        .execute(
            user_id,
            payload.name,
            payload.description,
            payload.language_code,
            payload.safety_level,
            payload.is_public,
            payload.prompts,
        )
        .await?;

    Ok(RestApiResponse::success(CreateSituationPackResponse { id: pack_id }))
}

#[utoipa::path(
    get,
    path = "/games/packs/situations",
    responses((status = 200, description = "List all visible situation packs", body = [SituationPackDto])),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn list_situation_packs(
    State(state): State<GameState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let packs = state.list_situation_packs.execute(user_id).await?;
    let dtos: Vec<SituationPackDto> = packs.into_iter().map(SituationPackDto::from).collect();

    Ok(RestApiResponse::success(dtos))
}

#[utoipa::path(
    get,
    path = "/games/packs/situations/{id}",
    responses((status = 200, description = "Get situation pack details", body = SituationPackDetailsResponse)),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn get_situation_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    let res = state.get_situation_pack.execute(id, user_id).await?;

    let details = SituationPackDetailsResponse {
        pack: SituationPackDto::from(res.pack),
        situations: res.situations.into_iter().map(PackSituationDto::from).collect(),
    };

    Ok(RestApiResponse::success(details))
}

#[utoipa::path(
    patch,
    path = "/games/packs/situations/{id}",
    request_body = UpdateSituationPackRequest,
    responses((status = 200, description = "Update situation pack metadata")),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn update_situation_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
    Json(payload): Json<UpdateSituationPackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    state
        .update_situation_pack
        .execute(
            user_id,
            id,
            payload.name,
            payload.description,
            payload.language_code,
            payload.safety_level,
            payload.is_public,
        )
        .await?;

    Ok(RestApiResponse::success_with_message("Situation pack updated successfully".to_string(), ()))
}

#[utoipa::path(
    delete,
    path = "/games/packs/situations/{id}",
    responses((status = 200, description = "Delete a situation pack")),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_situation_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    state.delete_situation_pack.execute(user_id, id).await?;

    Ok(RestApiResponse::success_with_message("Situation pack deleted successfully".to_string(), ()))
}

#[utoipa::path(
    post,
    path = "/games/packs/situations/{id}/situations",
    request_body = AddSituationsToPackRequest,
    responses((status = 200, description = "Add situations to an existing pack")),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn add_situations_to_pack(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path(id_str): Path<String>,
    Json(payload): Json<AddSituationsToPackRequest>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let id = Uuid::parse_str(&id_str)
        .map_err(|_| AppError::ValidationError("Invalid pack ID".to_string()))?;

    state.add_situations_to_pack.execute(user_id, id, payload.prompts).await?;

    Ok(RestApiResponse::success_with_message("Situations added to pack successfully".to_string(), ()))
}

#[utoipa::path(
    delete,
    path = "/games/packs/situations/{id}/situations/{situation_id}",
    responses((status = 200, description = "Remove a situation from a pack")),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn delete_pack_situation(
    State(state): State<GameState>,
    current_user: CurrentUser,
    Path((_id_str, situation_id_str)): Path<(String, String)>,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;
    let situation_id = Uuid::parse_str(&situation_id_str)
        .map_err(|_| AppError::ValidationError("Invalid situation ID".to_string()))?;

    state.delete_pack_situation.execute(user_id, situation_id).await?;

    Ok(RestApiResponse::success_with_message("Situation removed from pack successfully".to_string(), ()))
}

#[utoipa::path(
    get,
    path = "/games/packs/memes/me",
    responses((status = 200, description = "List meme packs created by current user", body = [MemePackDetailsResponse])),
    tag = "Meme Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn list_user_meme_packs(
    State(state): State<GameState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let results = state.list_user_meme_packs.execute(user_id).await?;
    let dtos: Vec<MemePackDetailsResponse> = results
        .into_iter()
        .map(|res| MemePackDetailsResponse {
            pack: MemePackDto::from(res.pack),
            memes: res.memes.into_iter().map(PackMemeDetailsDto::from).collect(),
        })
        .collect();

    Ok(RestApiResponse::success(dtos))
}

#[utoipa::path(
    get,
    path = "/games/packs/situations/me",
    responses((status = 200, description = "List situation packs created by current user", body = [SituationPackDetailsResponse])),
    tag = "Situation Packs",
    security(
        ("bearer_auth" = [])
    )
)]
pub async fn list_user_situation_packs(
    State(state): State<GameState>,
    current_user: CurrentUser,
) -> Result<impl IntoResponse, AppError> {
    let user_id = Uuid::parse_str(&current_user.user_id)
        .map_err(|_| AppError::ValidationError("Invalid current user ID".to_string()))?;

    let results = state.list_user_situation_packs.execute(user_id).await?;
    let dtos: Vec<SituationPackDetailsResponse> = results
        .into_iter()
        .map(|res| SituationPackDetailsResponse {
            pack: SituationPackDto::from(res.pack),
            situations: res.situations.into_iter().map(PackSituationDto::from).collect(),
        })
        .collect();

    Ok(RestApiResponse::success(dtos))
}

