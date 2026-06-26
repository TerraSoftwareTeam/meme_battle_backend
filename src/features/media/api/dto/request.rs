use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use validator::Validate;

#[derive(Debug, ToSchema)]
#[allow(dead_code)]
pub struct UploadMediaRequestDto {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, ToSchema, Validate)]
pub struct UploadMediaFromUrlDto {
    #[validate(url(message = "Source URL must be valid"))]
    pub url: String,
}
