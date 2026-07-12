use axum::{
    http::StatusCode,
    response::{IntoResponse, Response},
    BoxError,
};

use sqlx::Error as SqlxError;
use thiserror::Error;
use tracing::error;

use crate::common::http::dto::RestApiResponse;

use super::dto::ApiResponse;

/// AppError is an enum that represents various types of errors that can occur in the application.
/// AppError is an enum that represents various types of errors that can occur in the application.
/// It implements the `std::error::Error` trait and the `axum::response::IntoResponse` trait.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("Database error: {0}")]
    DatabaseError(#[from] SqlxError), // Used for database-related errors

    #[error("Not found: {0}")]
    NotFound(String), // Used for not found errors

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Internal server error")]
    InternalError,

    #[error("Validation error: {0}")]
    ValidationError(String),

    #[error("Forbidden Request: {0}")]
    Forbidden(String),

    #[error("Provider error: {0}")]
    ProviderError(String),

    /// Used for authentication-related errors
    #[error("Wrong credentials")]
    WrongCredentials,
    #[error("Missing credentials")]
    MissingCredentials,
    #[error("Invalid token")]
    InvalidToken,
    #[error("Token creation error")]
    TokenCreation,
    #[error("User not found")]
    UserNotFound,
    #[error("User already exists")]
    UserAlreadyExists,
}

/// Converts the AppError enum into an HTTP response.
/// It maps the error to an appropriate HTTP status code and constructs a JSON response body.
impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match self {
            AppError::ValidationError(_) => StatusCode::BAD_REQUEST,
            AppError::DatabaseError(_) => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::NotFound(_) => StatusCode::NOT_FOUND,
            AppError::Conflict(_) => StatusCode::CONFLICT,
            AppError::InternalError => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::Forbidden(_) => StatusCode::FORBIDDEN,
            AppError::ProviderError(_) => StatusCode::BAD_GATEWAY,
            AppError::WrongCredentials => StatusCode::UNAUTHORIZED,
            AppError::MissingCredentials => StatusCode::BAD_REQUEST,
            AppError::InvalidToken => StatusCode::UNAUTHORIZED,
            AppError::TokenCreation => StatusCode::INTERNAL_SERVER_ERROR,
            AppError::UserNotFound => StatusCode::NOT_FOUND,
            AppError::UserAlreadyExists => StatusCode::CONFLICT,
        };

        let message = self.to_string();

        if status.is_server_error() {
            error!(error = %message, status = status.as_u16(), "Application error");
        } else {
            tracing::warn!(error = %message, status = status.as_u16(), "Client error");
        }

        let body = axum::Json(ApiResponse::<()> {
            status: status.as_u16(),
            message,
            data: None,
            request_id: crate::common::http::dto::get_current_request_id(),
        });

        (status, body).into_response()
    }
}

/// handle_error is a function that middlewares the error handling in the application.
/// It takes a BoxError as input and returns an HTTP response.
/// It maps the error to an appropriate HTTP status code and constructs a JSON response body.
/// The function is used to handle errors that occur during the request processing.
/// It is designed to be used with the axum framework.
pub async fn handle_error(error: BoxError) -> impl IntoResponse {
    let status = if error.is::<tower::timeout::error::Elapsed>() {
        StatusCode::REQUEST_TIMEOUT
    } else {
        StatusCode::INTERNAL_SERVER_ERROR
    };

    let message = error.to_string();
    error!(?status, %message, "Request failed");

    let body = RestApiResponse::<()>::failure(status.as_u16(), message);

    (status, body)
}
