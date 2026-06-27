use utoipa::ToSchema;

#[derive(Debug, ToSchema)]
#[allow(dead_code)]
pub struct UploadMediaRequestDto {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}
