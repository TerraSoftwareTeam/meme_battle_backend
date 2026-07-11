use crate::{
    common::http::error::AppError,
    features::media::UploadFile,
};
use axum::extract::Multipart;

pub async fn extract_file(
    multipart: &mut Multipart,
    max_file_size_bytes: usize,
) -> Result<UploadFile, AppError> {
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

        if bytes.len() > max_file_size_bytes {
            return Err(AppError::ValidationError(format!(
                "File cannot exceed {} bytes",
                max_file_size_bytes
            )));
        }

        return Ok(UploadFile {
            filename,
            content_type,
            bytes: bytes.to_vec(),
        });
    }

    Err(AppError::ValidationError("Missing file field".into()))
}
