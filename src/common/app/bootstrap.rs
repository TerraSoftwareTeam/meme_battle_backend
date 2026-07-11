use std::sync::Arc;
use std::time::Instant;

use sqlx::{migrate::MigrateError, PgPool};

use crate::common::app::adapters::MediaAssetToUserAvatarAdapter;
use crate::common::{
    app::{
        config::Config,
        state::{AppState, AuthState, MediaState, UserState, RealtimeState},
    },
};
use crate::features::auth::{
    AuthAsGuestCommand, AuthRepository, AuthRepositoryImpl, LoginUserCommand,
    RefreshSessionCommand, RegisterUserCommand,
};
use crate::features::media::{
    FileStorage, GetMediaAssetUrlQuery, HackClubCdnStorage, MarkMediaAttachedCommand,
    MediaRepository, PostgresMediaRepository, UploadMediaCommand,
};
use crate::features::user::{
    AvatarMediaUploader, GetMeQuery, GetUserByIdQuery, GetUserListQuery, GetUsersQuery,
    MediaAssetResolver, PromoteToAdminCommand, UpdateMeCommand, UpdateMyAvatarCommand,
    UserRepository, UserRepositoryImpl,
};

use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

/// Runs database migrations before the application starts handling requests.
pub async fn run_database_migrations(pool: &PgPool) -> Result<(), MigrateError> {
    let started_at = Instant::now();

    tracing::info!("Database migrations started");

    match sqlx::migrate!("./migrations").run(pool).await {
        Ok(()) => {
            tracing::info!(
                elapsed_ms = started_at.elapsed().as_millis(),
                "Database migrations completed"
            );
            Ok(())
        }
        Err(err) => {
            tracing::error!(
                elapsed_ms = started_at.elapsed().as_millis(),
                error = %err,
                "Database migrations failed"
            );
            Err(err)
        }
    }
}

/// Constructs and wires all application services and returns a configured AppState.
pub fn build_app_state(pool: PgPool, config: Config) -> AppState {
    tracing::info!("Building application state");

    // Auth
    let auth_repository: Arc<dyn AuthRepository> = Arc::new(AuthRepositoryImpl::new(pool.clone()));
    let register_user = Arc::new(RegisterUserCommand::new(auth_repository.clone()));
    let login_user = Arc::new(LoginUserCommand::new(
        auth_repository.clone(),
        config.admin_user_ids.clone(),
    ));
    let auth_as_guest = Arc::new(AuthAsGuestCommand::new(auth_repository.clone()));
    let refresh_session = Arc::new(RefreshSessionCommand::new(
        auth_repository,
        config.admin_user_ids.clone(),
    ));

    // Media
    let media_repository: Arc<dyn MediaRepository> =
        Arc::new(PostgresMediaRepository::new(pool.clone()));
    let media_storage: Arc<dyn FileStorage> = Arc::new(HackClubCdnStorage::new(
        config.hackclub_cdn_base_url.clone(),
        config.hackclub_cdn_api_key.clone(),
    ));
    let upload_media = Arc::new(UploadMediaCommand::new(
        media_storage,
        media_repository.clone(),
    ));
    let get_media_asset_url = Arc::new(GetMediaAssetUrlQuery::new(media_repository.clone()));
    let mark_media_attached = Arc::new(MarkMediaAttachedCommand::new(media_repository.clone()));
    let user_media_adapter = Arc::new(MediaAssetToUserAvatarAdapter::new(
        get_media_asset_url.clone(),
        upload_media.clone(),
    ));
    let media_asset_resolver: Arc<dyn MediaAssetResolver> = user_media_adapter.clone();
    let avatar_media_uploader: Arc<dyn AvatarMediaUploader> = user_media_adapter;

    // User
    let user_repository: Arc<dyn UserRepository> = Arc::new(UserRepositoryImpl::new(pool.clone()));
    let update_me = Arc::new(UpdateMeCommand::new(
        user_repository.clone(),
        media_asset_resolver.clone(),
    ));
    let update_my_avatar = Arc::new(UpdateMyAvatarCommand::new(
        user_repository.clone(),
        avatar_media_uploader,
    ));
    let get_me = Arc::new(GetMeQuery::new(
        user_repository.clone(),
        media_asset_resolver.clone(),
    ));
    let get_user_by_id = Arc::new(GetUserByIdQuery::new(
        user_repository.clone(),
        media_asset_resolver.clone(),
    ));
    let get_user_list = Arc::new(GetUserListQuery::new(
        user_repository.clone(),
        media_asset_resolver.clone(),
    ));
    let get_users = Arc::new(GetUsersQuery::new(
        user_repository.clone(),
        media_asset_resolver,
    ));
    let promote_to_admin = Arc::new(PromoteToAdminCommand::new(user_repository));

    // States
    let auth_state = AuthState::new(register_user, login_user, auth_as_guest, refresh_session);
    let media_state = MediaState::new(
        upload_media,
        get_media_asset_url.clone(),
        mark_media_attached.clone(),
    );
    let user_state = UserState::new(
        update_me,
        update_my_avatar,
        get_me,
        get_user_by_id,
        get_user_list,
        get_users,
        promote_to_admin,
    );

    // Realtime Feature Bootstrapping
    let outbox_repository = Arc::new(crate::features::realtime::PostgresOutboxRepository::new(pool.clone()));
    let realtime_publisher = Arc::new(crate::features::realtime::HttpCentrifugoClient::new(config.clone()));

    let publish_usecase = Arc::new(crate::features::realtime::PublishNotificationCommand::new(outbox_repository.clone()));
    let token_usecase = Arc::new(crate::features::realtime::GenerateTokenCommand::new());

    let outbox_processor = Arc::new(crate::features::realtime::OutboxProcessor::new(outbox_repository, realtime_publisher));

    // Global Adapters wiring Game feature to Realtime feature
    let notification_sender: Arc<dyn crate::features::game::GameNotificationSender> =
        Arc::new(crate::common::app::adapters::GameNotificationSenderAdapter::new(
            publish_usecase,
            get_media_asset_url.clone(),
        ));
    let token_generator: Arc<dyn crate::features::game::GameTokenGenerator> =
        Arc::new(crate::common::app::adapters::GameTokenGeneratorAdapter::new(token_usecase));

    let realtime_state = RealtimeState::new(
        outbox_processor,
    );

    // Game
    let game_repository: Arc<dyn crate::features::game::GameRepository> =
        Arc::new(crate::features::game::GameRepositoryImpl::new(pool.clone()));
    let game_media_manager = Arc::new(crate::common::app::adapters::game_media_manager_adapter::GameMediaManagerAdapter::new(
        get_media_asset_url.clone(),
        media_repository.clone(),
    ));

    let create_game = Arc::new(crate::features::game::CreateGameCommand::new(game_repository.clone()));
    let join_game = Arc::new(crate::features::game::JoinGameCommand::new(game_repository.clone(), notification_sender.clone()));
    let set_ready = Arc::new(crate::features::game::SetReadyCommand::new(game_repository.clone(), notification_sender.clone()));
    let start_game = Arc::new(crate::features::game::StartGameCommand::new(game_repository.clone(), notification_sender.clone()));
    let update_game = Arc::new(crate::features::game::UpdateGameCommand::new(game_repository.clone()));
    let submit_card = Arc::new(crate::features::game::SubmitCardCommand::new(game_repository.clone(), notification_sender.clone()));
    let vote_card = Arc::new(crate::features::game::VoteCardCommand::new(game_repository.clone(), notification_sender.clone()));
    let get_game_state = Arc::new(crate::features::game::GetGameStateQuery::new(game_repository.clone(), game_media_manager.clone()));
    let create_meme_pack = Arc::new(crate::features::game::CreateMemePackCommand::new(
        game_repository.clone(),
        mark_media_attached.clone(),
        game_media_manager.clone(),
    ));
    let update_meme_pack = Arc::new(crate::features::game::UpdateMemePackCommand::new(game_repository.clone()));
    let delete_meme_pack = Arc::new(crate::features::game::DeleteMemePackCommand::new(game_repository.clone()));
    let add_memes_to_pack = Arc::new(crate::features::game::AddMemesToPackCommand::new(
        game_repository.clone(),
        mark_media_attached.clone(),
        game_media_manager.clone(),
    ));
    let delete_pack_meme = Arc::new(crate::features::game::DeletePackMemeCommand::new(game_repository.clone()));
    let create_situation_pack = Arc::new(crate::features::game::CreateSituationPackCommand::new(game_repository.clone()));
    let update_situation_pack = Arc::new(crate::features::game::UpdateSituationPackCommand::new(game_repository.clone()));
    let delete_situation_pack = Arc::new(crate::features::game::DeleteSituationPackCommand::new(game_repository.clone()));
    let add_situations_to_pack = Arc::new(crate::features::game::AddSituationsToPackCommand::new(game_repository.clone()));
    let delete_pack_situation = Arc::new(crate::features::game::DeletePackSituationCommand::new(game_repository.clone()));

    let list_meme_packs = Arc::new(crate::features::game::ListMemePacksQuery::new(game_repository.clone()));
    let list_user_meme_packs = Arc::new(crate::features::game::ListUserMemePacksQuery::new(game_repository.clone(), game_media_manager.clone()));
    let get_meme_pack = Arc::new(crate::features::game::GetMemePackQuery::new(game_repository.clone(), game_media_manager.clone()));
    let list_situation_packs = Arc::new(crate::features::game::ListSituationPacksQuery::new(game_repository.clone()));
    let list_user_situation_packs = Arc::new(crate::features::game::ListUserSituationPacksQuery::new(game_repository.clone()));
    let get_situation_pack = Arc::new(crate::features::game::GetSituationPackQuery::new(game_repository.clone()));
    let get_ws_token = Arc::new(crate::features::game::GetWsTokenQuery::new(game_repository.clone(), token_generator));

    let process_timeout = Arc::new(crate::features::game::ProcessTimeoutCommand::new(
        game_repository.clone(),
        notification_sender,
    ));
    let timer_worker = Arc::new(crate::features::game::GameTimerWorker::new(
        game_repository,
        process_timeout.clone(),
    ));

    let game_state = crate::features::game::GameState::new(
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
        process_timeout,
        timer_worker,
        game_media_manager,
    );

    let state = AppState::new(
        config,
        auth_state,
        media_state,
        user_state,
        game_state,
        realtime_state,
    );

    tracing::info!("Application state built");

    state
}

/// Setup tracing for the application.
pub fn setup_tracing() {
    dotenvy::dotenv().ok();

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=info,tower_http=info,axum::rejection=trace".into()),
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_file(true)
                .with_line_number(true)
                .with_timer(tracing_subscriber::fmt::time::ChronoLocal::rfc_3339())
                .with_thread_ids(true)
                .with_thread_names(true)
                .with_target(true)
                .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE),
        )
        .init();
}

/// Shutdown signal handler
pub async fn shutdown_signal() {
    match tokio::signal::ctrl_c().await {
        Ok(()) => tracing::info!("Shutdown signal received"),
        Err(err) => tracing::error!(
            error = %err,
            "Failed to install CTRL+C signal handler"
        ),
    }
}
