use async_trait::async_trait;
use serde_json::json;
use std::time::Duration;
use crate::common::{app::config::Config, http::error::AppError};
use crate::features::realtime::domain::ports::realtime_publisher::RealtimePublisher;

pub struct HttpCentrifugoClient {
    config: Config,
    client: reqwest::Client,
}

impl HttpCentrifugoClient {
    pub fn new(config: Config) -> Self {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(3))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            config,
            client,
        }
    }
}

#[async_trait]
impl RealtimePublisher for HttpCentrifugoClient {
    async fn publish(&self, channel: &str, payload: serde_json::Value) -> Result<(), AppError> {
        let body = json!({
            "method": "publish",
            "params": {
                "channel": channel,
                "data": payload
            }
        });

        let response = self.client
            .post(&self.config.centrifugo_api_url)
            .header("Authorization", format!("apikey {}", self.config.centrifugo_api_key))
            .json(&body)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if resp.status().is_success() {
                    Ok(())
                } else {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    tracing::error!("Centrifugo returned error status {}: {}", status, text);
                    Err(AppError::InternalError)
                }
            }
            Err(err) => {
                tracing::error!("Failed to reach Centrifugo: {:?}", err);
                Err(AppError::InternalError)
            }
        }
    }
}
