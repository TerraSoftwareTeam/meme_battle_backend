use sqlx::{postgres::{PgPoolOptions, PgConnectOptions}, PgPool, ConnectOptions};
use std::str::FromStr;
use std::env;
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio::time::sleep;
use tracing::{error, info, warn};

/// Config is a struct that holds the configuration for the application.
#[derive(Clone, Debug)]
pub struct Config {
    pub database_url: String,
    pub database_max_connections: u32,
    pub database_min_connections: u32,

    pub service_host: String,
    pub service_port: String,

    pub hackclub_cdn_base_url: String,
    pub hackclub_cdn_api_key: Option<String>,

    pub admin_user_ids: Vec<String>,
    pub secret_seed_key: String,

    pub centrifugo_api_url: String,
    pub centrifugo_api_key: String,
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Required environment variable {name} is not set")]
    MissingEnv { name: &'static str },
}

/// from_env reads the environment variables and returns a Config struct.
/// It uses the dotenv crate to load environment variables from a .env file if it exists.
/// It returns a Result with the Config struct or an error if any of the environment variables are missing.
impl Config {
    pub fn from_env() -> Result<Self, ConfigError> {
        dotenvy::dotenv().ok();

        let config = Self {
            database_url: database_url_from_env()?,

            database_max_connections: optional_u32_env("DATABASE_MAX_CONNECTIONS", 5),
            database_min_connections: optional_u32_env("DATABASE_MIN_CONNECTIONS", 1),

            service_host: required_env("SERVICE_HOST")?,
            service_port: required_env("SERVICE_PORT")?,

            hackclub_cdn_base_url: env::var("HACKCLUB_CDN_BASE_URL")
                .unwrap_or_else(|_| "https://cdn.hackclub.com".to_string()),
            hackclub_cdn_api_key: optional_non_empty_env("HACKCLUB_CDN_API_KEY"),

            admin_user_ids: env::var("ADMIN_USER_IDS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            secret_seed_key: required_env("SECRET_SEED_KEY")?,

            centrifugo_api_url: env::var("CENTRIFUGO_API_URL")
                .unwrap_or_else(|_| "http://localhost:8000/api".to_string()),
            centrifugo_api_key: env::var("CENTRIFUGO_API_KEY")
                .unwrap_or_else(|_| "CHANGE_ME_API_KEY".to_string()),
        };

        validate_sensitive_env("JWT_SECRET_KEY")?;
        validate_sensitive_env("ARGON2_SECRET_KEY")?;
        validate_sensitive_env("SECRET_SEED_KEY")?;

        info!(
            service_host = %config.service_host,
            service_port = %config.service_port,
            database = %config.database_log_label(),
            database_max_connections = config.database_max_connections,
            database_min_connections = config.database_min_connections,
            hackclub_cdn_base_url = %config.hackclub_cdn_base_url,
            admin_user_ids = ?config.admin_user_ids,
            "Application configuration loaded"
        );

        Ok(config)
    }

    pub fn database_log_label(&self) -> String {
        redact_database_url(&self.database_url)
    }
}

/// setup_database initializes the database connection pool.
pub async fn setup_database(config: &Config) -> Result<PgPool, sqlx::Error> {
    const MAX_DATABASE_CONNECT_ATTEMPTS: u32 = 3;
    const DATABASE_RETRY_DELAY: Duration = Duration::from_secs(1);

    let mut attempts = 0;
    let started_at = Instant::now();

    info!(
        database = %config.database_log_label(),
        max_connections = config.database_max_connections,
        min_connections = config.database_min_connections,
        max_attempts = MAX_DATABASE_CONNECT_ATTEMPTS,
        "Connecting to database"
    );

    let pool = loop {
        attempts += 1;
        let attempt_started_at = Instant::now();

        info!(
            attempt = attempts,
            max_attempts = MAX_DATABASE_CONNECT_ATTEMPTS,
            "Database connection attempt started"
        );

        let mut connect_options = PgConnectOptions::from_str(&config.database_url)?;
        connect_options = connect_options.log_statements(log::LevelFilter::Trace);

        match PgPoolOptions::new()
            .max_connections(config.database_max_connections)
            .min_connections(config.database_min_connections)
            .connect_with(connect_options)
            .await
        {
            Ok(pool) => {
                info!(
                    attempt = attempts,
                    attempt_elapsed_ms = attempt_started_at.elapsed().as_millis(),
                    total_elapsed_ms = started_at.elapsed().as_millis(),
                    "Database connection established"
                );
                break pool;
            }
            Err(err) => {
                if attempts >= MAX_DATABASE_CONNECT_ATTEMPTS {
                    error!(
                        attempt = attempts,
                        max_attempts = MAX_DATABASE_CONNECT_ATTEMPTS,
                        elapsed_ms = started_at.elapsed().as_millis(),
                        error = %err,
                        "Database connection failed"
                    );
                    return Err(err);
                }

                warn!(
                    attempt = attempts,
                    max_attempts = MAX_DATABASE_CONNECT_ATTEMPTS,
                    retry_in_ms = DATABASE_RETRY_DELAY.as_millis(),
                    error = %err,
                    "Database connection failed; retrying"
                );
                sleep(DATABASE_RETRY_DELAY).await;
            }
        }
    };

    Ok(pool)
}

fn required_env(name: &'static str) -> Result<String, ConfigError> {
    let value = env::var(name).map_err(|_| {
        error!(env_var = name, "Required environment variable is missing");
        ConfigError::MissingEnv { name }
    })?;

    if value.trim().is_empty() {
        error!(env_var = name, "Required environment variable is empty");
        return Err(ConfigError::MissingEnv { name });
    }

    Ok(value)
}

fn validate_sensitive_env(name: &'static str) -> Result<(), ConfigError> {
    required_env(name).map(|_| ())
}

fn database_url_from_env() -> Result<String, ConfigError> {
    let user = required_env("POSTGRES_USER")?;
    let password = required_env("POSTGRES_PASSWORD")?;
    let host = env::var("POSTGRES_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = required_env("POSTGRES_PORT")?;
    let database = required_env("POSTGRES_DB")?;

    Ok(format!(
        "postgres://{}:{}@{}:{}/{}",
        urlencoding::encode(&user),
        urlencoding::encode(&password),
        host,
        port,
        urlencoding::encode(&database)
    ))
}

fn optional_u32_env(name: &'static str, default: u32) -> u32 {
    match env::var(name) {
        Ok(value) => match value.parse::<u32>() {
            Ok(parsed) => parsed,
            Err(err) => {
                warn!(
                    env_var = name,
                    value = %value,
                    default = default,
                    error = %err,
                    "Invalid numeric environment variable; using default"
                );
                default
            }
        },
        Err(_) => default,
    }
}

fn optional_non_empty_env(name: &'static str) -> Option<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn redact_database_url(database_url: &str) -> String {
    if let Some((_, after_credentials)) = database_url.rsplit_once('@') {
        return after_credentials
            .split('?')
            .next()
            .unwrap_or(after_credentials)
            .to_string();
    }

    "<redacted>".to_string()
}
