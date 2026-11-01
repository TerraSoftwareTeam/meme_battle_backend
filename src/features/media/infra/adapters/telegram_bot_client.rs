use async_trait::async_trait;
use reqwest::Client;
use serde::Deserialize;
use std::time::Duration;

use crate::{
    common::http::error::AppError,
    features::media::domain::{
        ports::telegram_client::TelegramStickerClient,
        telegram::{DownloadedSticker, TelegramSticker, TelegramStickerSet},
    },
};

#[derive(Clone, Debug)]
pub struct TelegramBotClient {
    bot_token: String,
    base_url: String,
    client: Client,
}

impl TelegramBotClient {
    pub fn new(bot_token: String, base_url: String) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        Self {
            bot_token,
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    pub fn with_client(bot_token: String, base_url: String, client: Client) -> Self {
        Self {
            bot_token,
            base_url: base_url.trim_end_matches('/').to_string(),
            client,
        }
    }

    fn api_url(&self, method: &str) -> String {
        format!("{}/bot{}/{}", self.base_url, self.bot_token, method)
    }

    fn file_download_url(&self, file_path: &str) -> String {
        format!("{}/file/bot{}/{}", self.base_url, self.bot_token, file_path)
    }
}

#[derive(Debug, Deserialize)]
struct TgApiResponse<T> {
    ok: bool,
    result: Option<T>,
    description: Option<String>,
    error_code: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct TgStickerSet {
    name: String,
    title: String,
    stickers: Vec<TgSticker>,
}

#[derive(Debug, Deserialize)]
struct TgSticker {
    file_id: String,
    file_unique_id: String,
    width: i32,
    height: i32,
    is_animated: Option<bool>,
    is_video: Option<bool>,
    emoji: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TgFile {
    file_path: Option<String>,
}

#[async_trait]
impl TelegramStickerClient for TelegramBotClient {
    async fn get_sticker_set(&self, set_name: &str) -> Result<TelegramStickerSet, AppError> {
        if self.bot_token.is_empty() {
            return Err(AppError::ProviderError(
                "Telegram bot token is not configured on this server".to_string(),
            ));
        }

        let url = format!(
            "{}?name={}",
            self.api_url("getStickerSet"),
            urlencoding::encode(set_name)
        );
        let response = self.client.get(&url).send().await.map_err(|err| {
            AppError::ProviderError(format!("Failed to connect to Telegram API: {err}"))
        })?;

        let status = response.status();
        let payload = response
            .json::<TgApiResponse<TgStickerSet>>()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Invalid Telegram API JSON response: {err}"))
            })?;

        if !payload.ok || payload.result.is_none() {
            let desc = payload
                .description
                .unwrap_or_else(|| "Unknown Telegram error".to_string());
            let code = payload.error_code.unwrap_or(status.as_u16() as i32);

            if code == 400 || code == 404 || desc.contains("STICKERSET_INVALID") {
                return Err(AppError::NotFound(format!(
                    "Telegram sticker pack '{set_name}' not found: {desc}"
                )));
            }
            if code == 401 {
                return Err(AppError::ProviderError(format!(
                    "Telegram bot token invalid or unauthorized: {desc}"
                )));
            }
            if code == 429 {
                return Err(AppError::ProviderError(format!(
                    "Telegram rate limit exceeded: {desc}"
                )));
            }

            return Err(AppError::ProviderError(format!(
                "Telegram API error {code}: {desc}"
            )));
        }

        let set = payload.result.unwrap();
        let stickers = set
            .stickers
            .into_iter()
            .map(|s| TelegramSticker {
                file_id: s.file_id,
                file_unique_id: s.file_unique_id,
                emoji: s.emoji,
                is_animated: s.is_animated.unwrap_or(false),
                is_video: s.is_video.unwrap_or(false),
                width: s.width,
                height: s.height,
            })
            .collect();

        Ok(TelegramStickerSet {
            name: set.name,
            title: set.title,
            stickers,
        })
    }

    async fn download_sticker_file(&self, file_id: &str) -> Result<DownloadedSticker, AppError> {
        if self.bot_token.is_empty() {
            return Err(AppError::ProviderError(
                "Telegram bot token is not configured on this server".to_string(),
            ));
        }

        // 1. Get file path from Telegram
        let url = format!(
            "{}?file_id={}",
            self.api_url("getFile"),
            urlencoding::encode(file_id)
        );
        let response = self.client.get(&url).send().await.map_err(|err| {
            AppError::ProviderError(format!("Failed to query Telegram getFile: {err}"))
        })?;

        let status = response.status();
        let payload = response
            .json::<TgApiResponse<TgFile>>()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Invalid Telegram getFile JSON response: {err}"))
            })?;

        if !payload.ok || payload.result.is_none() {
            let desc = payload
                .description
                .unwrap_or_else(|| "Unknown Telegram getFile error".to_string());
            let code = payload.error_code.unwrap_or(status.as_u16() as i32);

            return Err(AppError::NotFound(format!(
                "File for file_id '{file_id}' not found in Telegram (code {code}): {desc}"
            )));
        }

        let tg_file = payload.result.unwrap();
        let file_path = tg_file.file_path.ok_or_else(|| {
            AppError::NotFound(format!(
                "File path for file_id '{file_id}' not found in Telegram"
            ))
        })?;

        // Sanitize path against directory traversal or null bytes
        let clean_path = file_path.trim_start_matches('/');
        if clean_path.contains("..") || clean_path.contains('\\') || clean_path.contains('\0') {
            return Err(AppError::ProviderError(
                "Invalid sticker file path received from Telegram".to_string(),
            ));
        }

        // 2. Download the binary bytes
        let download_url = self.file_download_url(clean_path);
        let file_response = self.client.get(&download_url).send().await.map_err(|err| {
            AppError::ProviderError(format!("Failed to download Telegram sticker file: {err}"))
        })?;

        if !file_response.status().is_success() {
            return Err(AppError::ProviderError(format!(
                "Telegram file download returned HTTP {}",
                file_response.status()
            )));
        }

        let bytes = file_response
            .bytes()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Failed to read Telegram sticker bytes: {err}"))
            })?
            .to_vec();

        let raw_filename = clean_path.rsplit('/').next().unwrap_or("sticker.webp");
        let safe_stem: String = raw_filename
            .chars()
            .filter(|c| c.is_ascii_alphanumeric() || *c == '.' || *c == '_' || *c == '-')
            .collect();
        let safe_raw = if safe_stem.is_empty() {
            "sticker.webp".to_string()
        } else {
            safe_stem
        };

        let lower = safe_raw.to_ascii_lowercase();
        let (filename, content_type) = if lower.ends_with(".webp") {
            (safe_raw, "image/webp".to_string())
        } else if lower.ends_with(".png") {
            (safe_raw, "image/png".to_string())
        } else {
            (format!("{safe_raw}.webp"), "image/webp".to_string())
        };

        Ok(DownloadedSticker {
            bytes,
            filename,
            content_type,
        })
    }
}
