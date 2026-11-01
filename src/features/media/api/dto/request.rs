use serde::Deserialize;
use utoipa::ToSchema;

#[derive(Debug, ToSchema)]
#[allow(dead_code)]
pub struct UploadMediaRequestDto {
    #[schema(value_type = String, format = Binary)]
    pub file: Vec<u8>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct ImportTelegramStickersRequestDto {
    /// Telegram sticker pack URL (e.g. "https://t.me/addstickers/animals", "tg://addstickers?set=animals") or plain pack name
    #[schema(example = "https://t.me/addstickers/animals")]
    pub url: String,
}
