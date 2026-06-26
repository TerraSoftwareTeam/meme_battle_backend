use crate::common::http::error::AppError;
use crate::features::auth::api::dto::request::{AuthUserDto, RefreshSessionDto, RegisterAuthUserDto};
use validator::Validate;

pub fn validate_register_auth_user(payload: &RegisterAuthUserDto) -> Result<(), AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))
}

pub fn validate_auth_user(payload: &AuthUserDto) -> Result<(), AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))
}

pub fn validate_refresh_session(payload: &RefreshSessionDto) -> Result<(), AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))
}
