use async_trait::async_trait;
use reqwest::multipart;
use serde::{Deserialize, Serialize};

use crate::{
    common::http::error::AppError,
    features::media::{FileStorage, MediaProvider, StoredFile, UploadFile},
};

#[derive(Debug, Clone)]
pub struct HackClubCdnStorage {
    client: reqwest::Client,
    base_url: String,
    api_key: Option<String>,
}

impl HackClubCdnStorage {
    pub fn new(base_url: impl Into<String>, api_key: Option<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
            api_key,
        }
    }

    fn api_key(&self) -> Result<&str, AppError> {
        self.api_key.as_deref().ok_or_else(|| {
            AppError::ProviderError("Hack Club CDN API key is not configured".into())
        })
    }

    fn upload_url(&self) -> String {
        format!("{}/api/v4/upload", self.base_url)
    }

    fn upload_from_url_url(&self) -> String {
        format!("{}/api/v4/upload_from_url", self.base_url)
    }

    fn delete_url(&self, provider_file_id: &str) -> String {
        format!("{}/api/v4/upload/{}", self.base_url, provider_file_id)
    }

    fn map_upload_response(response: HackClubUploadResponse) -> StoredFile {
        StoredFile {
            provider: MediaProvider::HackClubCdn,
            provider_file_id: response.id,
            url: response.url,
            filename: response.filename,
            content_type: response.content_type,
            size_bytes: response.size,
        }
    }
}

#[derive(Debug, Deserialize)]
struct HackClubUploadResponse {
    id: String,
    filename: String,
    size: i64,
    content_type: String,
    url: String,
}

#[derive(Debug, Serialize)]
struct UploadFromUrlRequest<'a> {
    url: &'a str,
}

#[derive(Debug, Deserialize)]
struct HackClubDeleteResponse {
    deleted: bool,
}

#[derive(Debug, Deserialize)]
struct HackClubErrorResponse {
    error: String,
}

#[async_trait]
impl FileStorage for HackClubCdnStorage {
    async fn upload(&self, file: UploadFile) -> Result<StoredFile, AppError> {
        let api_key = self.api_key()?;
        let part = multipart::Part::bytes(file.bytes)
            .file_name(file.filename)
            .mime_str(&file.content_type)
            .map_err(|err| AppError::ValidationError(format!("Invalid content type: {err}")))?;
        let form = multipart::Form::new().part("file", part);

        let response = self
            .client
            .post(self.upload_url())
            .bearer_auth(api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Hack Club CDN upload failed: {err}"))
            })?;

        if !response.status().is_success() {
            return Err(map_error_response(response).await);
        }

        let payload = response
            .json::<HackClubUploadResponse>()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Invalid Hack Club CDN response: {err}"))
            })?;

        Ok(Self::map_upload_response(payload))
    }

    async fn upload_from_url(&self, url: &str) -> Result<StoredFile, AppError> {
        let api_key = self.api_key()?;
        let response = self
            .client
            .post(self.upload_from_url_url())
            .bearer_auth(api_key)
            .json(&UploadFromUrlRequest { url })
            .send()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Hack Club CDN upload_from_url failed: {err}"))
            })?;

        if !response.status().is_success() {
            return Err(map_error_response(response).await);
        }

        let payload = response
            .json::<HackClubUploadResponse>()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Invalid Hack Club CDN response: {err}"))
            })?;

        Ok(Self::map_upload_response(payload))
    }

    async fn delete(&self, provider_file_id: &str) -> Result<(), AppError> {
        let api_key = self.api_key()?;
        let response = self
            .client
            .delete(self.delete_url(provider_file_id))
            .bearer_auth(api_key)
            .send()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Hack Club CDN delete failed: {err}"))
            })?;

        if !response.status().is_success() {
            return Err(map_error_response(response).await);
        }

        let payload = response
            .json::<HackClubDeleteResponse>()
            .await
            .map_err(|err| {
                AppError::ProviderError(format!("Invalid Hack Club CDN response: {err}"))
            })?;

        if payload.deleted {
            Ok(())
        } else {
            Err(AppError::ProviderError(
                "Hack Club CDN did not delete the upload".into(),
            ))
        }
    }
}

async fn map_error_response(response: reqwest::Response) -> AppError {
    let status = response.status();
    let error = response
        .json::<HackClubErrorResponse>()
        .await
        .map(|payload| payload.error)
        .unwrap_or_else(|_| "Unknown Hack Club CDN error".to_string());

    match status.as_u16() {
        400 | 422 => AppError::ValidationError(error),
        401 => AppError::Forbidden(error),
        404 => AppError::NotFound(error),
        402 => AppError::ProviderError(format!("Hack Club CDN quota exceeded: {error}")),
        _ => AppError::ProviderError(format!("Hack Club CDN returned {status}: {error}")),
    }
}
