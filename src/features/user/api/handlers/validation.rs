use crate::{
    common::http::error::AppError,
    features::user::{api::dto::request::UpdateMeDto, AvatarUploadFile},
};
use axum::extract::Multipart;
use validator::Validate;

const MAX_AVATAR_SIZE_BYTES: usize = 10 * 1024 * 1024;

pub fn validate_update_me(payload: &UpdateMeDto) -> Result<(), AppError> {
    payload
        .validate()
        .map_err(|err| AppError::ValidationError(format!("Invalid input: {}", err)))
}

pub async fn extract_avatar_file(multipart: &mut Multipart) -> Result<AvatarUploadFile, AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|err| AppError::ValidationError(format!("Invalid multipart body: {err}")))?
    {
        if field.name() != Some("file") {
            continue;
        }

        let filename = field
            .file_name()
            .map(ToString::to_string)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| AppError::ValidationError("File name is required".into()))?;
        let content_type = field
            .content_type()
            .map(ToString::to_string)
            .unwrap_or_else(|| "application/octet-stream".to_string());
        let bytes = field
            .bytes()
            .await
            .map_err(|err| AppError::ValidationError(format!("Invalid multipart file: {err}")))?;

        if bytes.is_empty() {
            return Err(AppError::ValidationError("File cannot be empty".into()));
        }

        if bytes.len() > MAX_AVATAR_SIZE_BYTES {
            return Err(AppError::ValidationError(format!(
                "File cannot exceed {} bytes",
                MAX_AVATAR_SIZE_BYTES
            )));
        }

        return Ok(AvatarUploadFile {
            filename,
            content_type,
            bytes: bytes.to_vec(),
        });
    }

    Err(AppError::ValidationError("Missing file field".into()))
}
