use common::app::{
    bootstrap::{build_app_state, run_database_migrations, shutdown_signal},
    config::{setup_database, Config},
};
use std::time::Instant;
use meme_battle_backend::{app::create_router, common};
use tracing::{error, info};

#[cfg(not(feature = "opentelemetry"))]
use common::app::bootstrap::setup_tracing;

#[cfg(feature = "opentelemetry")]
use common::observability::opentelemetry::{setup_tracing_opentelemetry, shutdown_opentelemetry};

/// Main entry point for the application.
/// It sets up the database connection, initializes the server, and starts listening for requests.
/// It also sets up the Swagger UI for API documentation.
///
/// # Errors
/// Returns an error if the database connection fails or if the server fails to start.
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let startup_started_at = Instant::now();

    #[cfg(not(feature = "opentelemetry"))]
    setup_tracing();

    #[cfg(feature = "opentelemetry")]
    let otel_providers = {
        let providers = setup_tracing_opentelemetry();
        // Startup span to ensure at least one span is generated and exported
        let span = tracing::info_span!("startup");
        let _enter = span.enter();
        providers
    };

    info!("Loading application configuration");
    let config = match Config::from_env() {
        Ok(config) => config,
        Err(err) => {
            error!(error = %err, "Application configuration failed");
            return Err(err.into());
        }
    };

    let pool = match setup_database(&config).await {
        Ok(pool) => pool,
        Err(err) => {
            error!(error = %err, "Application startup stopped because database is unavailable");
            return Err(err.into());
        }
    };

    if let Err(err) = run_database_migrations(&pool).await {
        error!(error = %err, "Application startup stopped because database migrations failed");
        return Err(err.into());
    }

    let state = build_app_state(pool.clone(), config.clone());

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    // Start unified realtime outbox processor worker
    state.realtime.processor.clone().start(pool.clone(), shutdown_rx.clone());

    // Start game timer background worker
    state.game.timer_worker.clone().start(shutdown_rx);

    let app = create_router(state);

    let addr = format!("{}:{}", config.service_host, config.service_port);

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(listener) => {
            info!(
                addr = %addr,
                local_addr = %listener.local_addr()?,
                startup_elapsed_ms = startup_started_at.elapsed().as_millis(),
            );
            listener
        }
        Err(err) => {
            error!(addr = %addr, error = %err, "Failed to bind HTTP listener");
            return Err(err.into());
        }
    };

    let shutdown_signal_fut = async move {
        shutdown_signal().await;
        info!("Graceful shutdown initiated, signaling background workers");
        let _ = shutdown_tx.send(true);
    };

    if let Err(err) = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal_fut)
        .await
    {
        error!(error = %err, "HTTP server stopped with error");
        return Err(err.into());
    }

    info!("HTTP server stopped");

    #[cfg(feature = "opentelemetry")]
    if let Err(err) = shutdown_opentelemetry(otel_providers) {
        error!(error = %err, "OpenTelemetry shutdown failed");
        return Err(err);
    }

    info!(
        uptime_ms = startup_started_at.elapsed().as_millis(),
        "Application shutdown completed"
    );

    Ok(())
}
