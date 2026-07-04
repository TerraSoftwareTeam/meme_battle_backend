use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Response},
};

use chrono::{Duration, Utc};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use rand::{distr::Alphanumeric, rng, Rng};
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use std::{env, fmt::Display};
use utoipa::ToSchema;

use crate::common::http::{current_user::CurrentUser, error::AppError, role::Role};

/// JWT_SECRET_KEY is the environment variable that holds the secret key for JWT encoding and decoding.
/// It is loaded from the environment variables using the dotenv crate.
/// The secret key is used to sign the JWT tokens and should be kept secret.
pub static KEYS: LazyLock<Keys> = LazyLock::new(|| {
    dotenvy::dotenv().ok();

    let jwt_secret = env::var("JWT_SECRET_KEY").expect("JWT_SECRET_KEY must be set");
    let argon_secret = env::var("ARGON2_SECRET_KEY").expect("ARGON2_SECRET_KEY must be set");
    let hmac_secret = env::var("ARGON2_SECRET_KEY").expect("ARGON2_SECRET_KEY must be set");

    Keys::new(
        jwt_secret.as_bytes(),
        argon_secret.as_bytes(),
        hmac_secret.as_bytes(),
    )
});

/// Keys is a struct that holds the encoding and decoding keys for JWT.
pub struct Keys {
    pub encoding: EncodingKey,
    pub decoding: DecodingKey,
    pub argon_secret: Vec<u8>,
    pub hmac_secret: Vec<u8>,
}

/// The Keys struct is used to create the encoding and decoding keys for JWT.
impl Keys {
    fn new(secret: &[u8], argon_secret: &[u8], hmac_secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
            argon_secret: argon_secret.to_vec(),
            hmac_secret: hmac_secret.to_vec(),
        }
    }
}

/// Claims is a struct that represents the claims in the JWT token.
/// It contains the subject (user ID), expiration time, and issued at time.
/// The `sub` field is the user ID, `exp` is the expiration time, and `iat` is the issued at time.
/// The `Claims` struct is used to encode and decode the JWT tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    pub sub: String,
    pub role: Role,
    pub exp: usize,
    pub iat: usize,
}

/// The Claims struct implements the `Display` trait for easy printing.
/// It formats the claims as a string, showing the user ID.
impl Display for Claims {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "user_id: {} role: {}", self.sub, self.role)
    }
}

/// The Default trait is implemented for the Claims struct.
/// It sets the default values for the claims.
impl Default for Claims {
    fn default() -> Self {
        let now = Utc::now();
        let expire: Duration = Duration::days(365);
        let exp: usize = (now + expire).timestamp() as usize;
        let iat: usize = now.timestamp() as usize;
        Claims {
            sub: String::new(),
            role: Role::User,
            exp,
            iat,
        }
    }
}

/// AuthBody is a struct that represents the authentication body.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct AuthBody {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
}

/// The AuthBody struct is used to create a new instance of the authentication body.
/// It takes an access & refresh token as a parameter and sets the token type to "Bearer".
impl AuthBody {
    pub fn new(access_token: String, refresh_token: String) -> Self {
        Self {
            access_token,
            refresh_token,
            token_type: "Bearer".to_string(),
        }
    }
}

/// make_jwt_token is a function that creates a JWT token.
/// It takes a user ID as a parameter and returns a Result with the JWT token or an error.
pub fn make_jwt_token(user_id: &str, role: &Role) -> Result<String, AppError> {
    let claims = Claims {
        sub: user_id.to_string(),
        role: role.clone(),
        ..Default::default()
    };
    encode(&Header::default(), &claims, &KEYS.encoding).map_err(|_| AppError::TokenCreation)
}

/// It used to generate a random string for refresh tokens (opaque token).
pub fn generate_random_string() -> String {
    rng()
        .sample_iter(&Alphanumeric)
        .take(64)
        .map(char::from)
        .collect()
}

/// Middleware to validate JWT tokens.
/// If the token is valid, the request proceeds; otherwise, a 401 Unauthorized is returned.
pub async fn jwt_auth<B>(mut req: Request<B>, next: Next) -> Result<Response, Response>
where
    B: Send + Into<axum::body::Body>,
{
    // Try to extract and trim the token in one go.
    let token = req
        .headers()
        .get("Authorization")
        .and_then(|v| v.to_str().ok())
        .and_then(|header| header.strip_prefix("Bearer "))
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .ok_or_else(|| AppError::InvalidToken.into_response())?;

    // Validate and decode the token.
    let token_data =
        decode::<Claims>(token, &KEYS.decoding, &Validation::default()).map_err(|err| {
            tracing::error!("Error decoding token: {:?}", err);
            AppError::InvalidToken.into_response()
        })?;

    req.extensions_mut().insert(CurrentUser {
        user_id: token_data.claims.sub,
        role: token_data.claims.role,
    });
    Ok(next.run(req.map(Into::into)).await)
}

#[derive(Debug, Serialize)]
pub struct CentrifugoConnectClaims {
    pub sub: String,
    pub exp: i64,
}

#[derive(Debug, Serialize)]
pub struct CentrifugoSubscribeClaims {
    pub sub: String,
    pub channel: String,
    pub exp: i64,
}

pub fn make_centrifugo_connect_token(user_id: &str) -> Result<String, AppError> {
    let exp = (Utc::now() + Duration::days(1)).timestamp();
    let claims = CentrifugoConnectClaims {
        sub: user_id.to_string(),
        exp,
    };
    encode(&Header::default(), &claims, &KEYS.encoding).map_err(|_| AppError::TokenCreation)
}

pub fn make_centrifugo_subscribe_token(user_id: &str, channel: &str) -> Result<String, AppError> {
    let exp = (Utc::now() + Duration::hours(2)).timestamp();
    let claims = CentrifugoSubscribeClaims {
        sub: user_id.to_string(),
        channel: channel.to_string(),
        exp,
    };
    encode(&Header::default(), &claims, &KEYS.encoding).map_err(|_| AppError::TokenCreation)
}
