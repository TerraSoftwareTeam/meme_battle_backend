use std::sync::Arc;
use std::time::Instant;

use sqlx::{migrate::MigrateError, PgPool};

use crate::common::app::adapters::MediaAssetToUserAvatarAdapter;
use crate::common::{
    app::{
        config::Config,
        state::{AppState, AuthState, MediaState, UserState},
    },
};
use crate::features::auth::{
    AuthAsGuestCommand, AuthRepository, AuthRepositoryImpl, LoginUserCommand,
    RefreshSessionCommand, RegisterUserCommand,
};
use crate::features::media::{
    DeleteMediaCommand, FileStorage, GetMediaAssetUrlQuery, GetMediaByIdQuery, GetUserMediaQuery,
    HackClubCdnStorage, MarkMediaAttachedCommand, MediaRepository, PostgresMediaRepository,
    UploadMediaCommand, UploadMediaFromUrlCommand,
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
        media_storage.clone(),
        media_repository.clone(),
    ));
    let upload_media_from_url = Arc::new(UploadMediaFromUrlCommand::new(
        media_storage.clone(),
        media_repository.clone(),
    ));
    let delete_media = Arc::new(DeleteMediaCommand::new(
        media_storage,
        media_repository.clone(),
    ));
    let get_media_by_id = Arc::new(GetMediaByIdQuery::new(media_repository.clone()));
    let get_media_asset_url = Arc::new(GetMediaAssetUrlQuery::new(media_repository.clone()));
    let get_user_media = Arc::new(GetUserMediaQuery::new(media_repository.clone()));
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
        upload_media_from_url,
        delete_media,
        get_media_by_id,
        get_user_media,
        get_media_asset_url.clone(),
        mark_media_attached,
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

    let state = AppState::new(
        config,
        auth_state,
        media_state,
        user_state,
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
