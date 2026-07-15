use crate::{
    common::http::error::AppError,
    features::user::api::dto::request::UpdateMeDto,
};
use validator::Validate;

pub fn validate_update_me(payload: &UpdateMeDto) -> Result<(), AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))
}
