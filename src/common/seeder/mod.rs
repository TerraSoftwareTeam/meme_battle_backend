use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    common::{app::config::Config, http::error::AppError},
    features::{
        game::{ContentSafetyLevel, GameRepository, GameRepositoryImpl, LanguageCode},
        media::{
            CreateMediaAsset, FileStorage, HackClubCdnStorage, MediaAsset, MediaProvider,
            MediaRepository, PostgresMediaRepository, StoredFile, UploadFile,
        },
    },
};

pub const DEFAULT_ADMIN_USER_ID: Uuid = Uuid::from_u128(1); // 00000000-0000-0000-0000-000000000001

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SituationSeedItem {
    Simple(String),
    Detailed { prompt_text: String },
}

impl SituationSeedItem {
    pub fn prompt_text(&self) -> &str {
        match self {
            Self::Simple(s) => s.as_str(),
            Self::Detailed { prompt_text } => prompt_text.as_str(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct SituationPackSeedConfig {
    pub pack_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
    #[serde(default = "default_safety_level")]
    pub safety_level: ContentSafetyLevel,
    #[serde(default = "default_is_public")]
    pub is_public: bool,
    pub items: Vec<SituationSeedItem>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum MemeSeedItem {
    Simple(String),
    Detailed { file: String },
}

impl MemeSeedItem {
    pub fn file_path(&self) -> &str {
        match self {
            Self::Simple(s) => s.as_str(),
            Self::Detailed { file } => file.as_str(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct MemePackSeedConfig {
    pub pack_id: Option<Uuid>,
    pub name: String,
    pub description: Option<String>,
    pub language_code: LanguageCode,
    #[serde(default = "default_safety_level")]
    pub safety_level: ContentSafetyLevel,
    #[serde(default = "default_is_public")]
    pub is_public: bool,
    pub items: Vec<MemeSeedItem>,
}

fn default_safety_level() -> ContentSafetyLevel {
    ContentSafetyLevel::FamilyFriendly
}

fn default_is_public() -> bool {
    true
}

pub struct Seeder {
    config: Config,
    storage: Arc<dyn FileStorage>,
    media_repository: Arc<dyn MediaRepository>,
    game_repository: Arc<dyn GameRepository>,
}

impl Seeder {
    pub fn new(pool: PgPool, config: Config) -> Self {
        let storage = Arc::new(HackClubCdnStorage::new(
            &config.hackclub_cdn_base_url,
            config.hackclub_cdn_api_key.clone(),
        ));
        let media_repository = Arc::new(PostgresMediaRepository::new(pool.clone()));
        let game_repository = Arc::new(GameRepositoryImpl::new(pool));

        Self {
            config,
            storage,
            media_repository,
            game_repository,
        }
    }

    pub fn with_dependencies(
        config: Config,
        storage: Arc<dyn FileStorage>,
        media_repository: Arc<dyn MediaRepository>,
        game_repository: Arc<dyn GameRepository>,
    ) -> Self {
        Self {
            config,
            storage,
            media_repository,
            game_repository,
        }
    }

    /// Ensure default admin user exists
    pub async fn ensure_admin_user(&self) -> Result<(), AppError> {
        let admin_id = self.config.default_admin_user_id;
        let username = if admin_id == Uuid::from_u128(1) {
            "admin".to_string()
        } else {
            format!("admin_{}", &admin_id.to_string()[..8])
        };

        self.game_repository
            .ensure_admin_user(admin_id, &username)
            .await
    }

    /// Run full sync for all seeds found in seeds_dir
    pub async fn sync_all(&self, seeds_dir: &Path) -> Result<(), AppError> {
        info!("Starting seeds synchronization from: {}", seeds_dir.display());
        self.ensure_admin_user().await?;

        if !seeds_dir.exists() {
            warn!("Seeds directory '{}' does not exist, skipping", seeds_dir.display());
            return Ok(());
        }

        // 1. Sync Situation Packs
        let situation_configs = self.find_config_files(seeds_dir, "situations");
        for config_path in situation_configs {
            if let Err(err) = self.sync_situation_pack(&config_path).await {
                error!(error = %err, path = %config_path.display(), "Failed to sync situation pack");
                return Err(err);
            }
        }

        // 2. Sync Meme Packs
        let meme_configs = self.find_config_files(seeds_dir, "memes");
        for config_path in meme_configs {
            if let Err(err) = self.sync_meme_pack(&config_path).await {
                error!(error = %err, path = %config_path.display(), "Failed to sync meme pack");
                return Err(err);
            }
        }

        info!("Seeds synchronization completed successfully");
        Ok(())
    }

    fn find_config_files(&self, base_dir: &Path, category: &str) -> Vec<PathBuf> {
        let mut results = Vec::new();

        // Check <base_dir>/official/<category>
        let official_dir = base_dir.join("official").join(category);
        if official_dir.is_dir() {
            self.collect_json_files(&official_dir, &mut results);
        }

        // Check <base_dir>/<category>
        let direct_dir = base_dir.join(category);
        if direct_dir.is_dir() {
            self.collect_json_files(&direct_dir, &mut results);
        }

        // Check <base_dir>/<category>.json
        let single_file = base_dir.join(format!("{}.json", category));
        if single_file.is_file() {
            results.push(single_file);
        }

        results.sort();
        results.dedup();
        results
    }

    fn collect_json_files(&self, dir: &Path, results: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
                    results.push(path);
                } else if path.is_dir() && path.file_name().map_or(true, |n| n != "assets") {
                    self.collect_json_files(&path, results);
                }
            }
        }
    }

    /// Sync a single situation pack config
    pub async fn sync_situation_pack(&self, config_path: &Path) -> Result<(), AppError> {
        info!("Syncing situation pack config: {}", config_path.display());
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| AppError::ProviderError(format!("Failed to read {}: {}", config_path.display(), e)))?;

        let config: SituationPackSeedConfig = serde_json::from_str(&content)
            .map_err(|e| AppError::ValidationError(format!("Invalid JSON in {}: {}", config_path.display(), e)))?;

        let pack_id = config.pack_id.unwrap_or_else(|| {
            // Deterministic UUID based on pack name if not specified
            Uuid::new_v5(&Uuid::NAMESPACE_DNS, config.name.as_bytes())
        });

        // 1. Upsert pack header via GameRepository
        self.game_repository
            .upsert_seed_situation_pack(
                pack_id,
                self.config.default_admin_user_id,
                &config.name,
                config.description.as_deref(),
                config.language_code,
                config.safety_level,
                config.is_public,
            )
            .await?;

        // 2. Prepare items with hashes
        let mut items = Vec::new();
        for item in &config.items {
            let prompt_text = item.prompt_text().trim();
            if !prompt_text.is_empty() {
                let hash = compute_text_hash(prompt_text);
                items.push((prompt_text.to_string(), hash));
            }
        }

        // 3. Sync items via GameRepository
        let stats = self
            .game_repository
            .sync_seed_situations(pack_id, &items)
            .await?;

        info!(
            pack_name = %config.name,
            inserted = stats.inserted,
            reactivated = stats.reactivated,
            unchanged = stats.unchanged,
            deactivated = stats.deactivated,
            "Situation pack synced successfully"
        );

        Ok(())
    }

    /// Sync a single meme pack config
    pub async fn sync_meme_pack(&self, config_path: &Path) -> Result<(), AppError> {
        info!("Syncing meme pack config: {}", config_path.display());
        let content = std::fs::read_to_string(config_path)
            .map_err(|e| AppError::ProviderError(format!("Failed to read {}: {}", config_path.display(), e)))?;

        let config: MemePackSeedConfig = serde_json::from_str(&content)
            .map_err(|e| AppError::ValidationError(format!("Invalid JSON in {}: {}", config_path.display(), e)))?;

        let pack_id = config.pack_id.unwrap_or_else(|| {
            Uuid::new_v5(&Uuid::NAMESPACE_DNS, config.name.as_bytes())
        });

        // 1. Upsert pack header via GameRepository
        self.game_repository
            .upsert_seed_meme_pack(
                pack_id,
                self.config.default_admin_user_id,
                &config.name,
                config.description.as_deref(),
                config.language_code,
                config.safety_level,
                config.is_public,
            )
            .await?;

        // 2. Fetch existing DB items for reconcile via GameRepository
        let existing_rows = self
            .game_repository
            .get_pack_memes_reconcile_state(pack_id)
            .await?;

        let mut existing_by_hash = std::collections::HashMap::new();
        for row in &existing_rows {
            if let Some(ref hash) = row.content_hash {
                existing_by_hash.insert(hash.clone(), row);
            }
        }

        let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
        let mut desired_hashes = HashSet::new();
        let mut inserted_count = 0;
        let mut reactivated_count = 0;
        let mut unchanged_count = 0;

        // 3. Process meme items from config
        for item in &config.items {
            let relative_file_path = item.file_path().trim();
            if relative_file_path.is_empty() {
                continue;
            }

            let file_path = config_dir.join(relative_file_path);
            if !file_path.is_file() {
                error!(
                    file = %file_path.display(),
                    config = %config_path.display(),
                    "Meme asset file not found"
                );
                return Err(AppError::NotFound(format!(
                    "Meme asset file '{}' referenced in '{}' was not found",
                    file_path.display(),
                    config_path.display()
                )));
            }

            let file_bytes = std::fs::read(&file_path)
                .map_err(|e| AppError::ProviderError(format!("Failed to read asset {}: {}", file_path.display(), e)))?;

            let hash = compute_file_hash(&file_bytes);
            if !desired_hashes.insert(hash.clone()) {
                continue;
            }

            if let Some(existing) = existing_by_hash.get(&hash) {
                if !existing.is_active {
                    self.game_repository.reactivate_pack_meme(existing.id).await?;
                    reactivated_count += 1;
                } else {
                    unchanged_count += 1;
                }
            } else {
                // Upload new asset and insert into DB
                let file_name = file_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("meme.jpg")
                    .to_string();

                let content_type = detect_content_type(&file_name).to_string();

                let media_asset = self
                    .upload_or_reuse_media(&file_name, &content_type, file_bytes, &hash)
                    .await?;

                self.game_repository
                    .insert_seed_pack_meme(pack_id, media_asset.id, &hash)
                    .await?;

                inserted_count += 1;
            }
        }

        // 4. Deactivate removed items via GameRepository
        let desired_hashes_vec: Vec<String> = desired_hashes.into_iter().collect();
        let deactivated_count = self
            .game_repository
            .deactivate_removed_pack_memes(pack_id, &desired_hashes_vec)
            .await?;

        info!(
            pack_name = %config.name,
            inserted = inserted_count,
            reactivated = reactivated_count,
            unchanged = unchanged_count,
            deactivated = deactivated_count,
            "Meme pack synced successfully"
        );

        Ok(())
    }

    async fn upload_or_reuse_media(
        &self,
        filename: &str,
        content_type: &str,
        bytes: Vec<u8>,
        hash: &str,
    ) -> Result<MediaAsset, AppError> {
        let upload_file = UploadFile {
            filename: filename.to_string(),
            content_type: content_type.to_string(),
            bytes,
        };

        let stored_file = if self.config.hackclub_cdn_api_key.is_some() {
            self.storage.upload(upload_file).await?
        } else {
            // Local / Offline fallback when CDN is not configured
            warn!("HACKCLUB_CDN_API_KEY is not set, generating local placeholder for seed asset");
            StoredFile {
                provider: MediaProvider::HackClubCdn,
                provider_file_id: format!("seed_{}", hash),
                url: format!("{}/local-seeds/{}/{}", self.config.hackclub_cdn_base_url, hash, filename),
                filename: filename.to_string(),
                content_type: content_type.to_string(),
                size_bytes: upload_file.bytes.len() as i64,
            }
        };

        let media_asset = self
            .media_repository
            .create(CreateMediaAsset {
                owner_user_id: self.config.default_admin_user_id.to_string(),
                stored_file,
            })
            .await?;

        // Mark as attached via MediaRepository
        self.media_repository
            .mark_attached(&[media_asset.id])
            .await?;

        Ok(media_asset)
    }
}

pub fn compute_text_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.trim().as_bytes());
    hex::encode(hasher.finalize())
}

pub fn compute_file_hash(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

pub fn detect_content_type(file_name: &str) -> &'static str {
    let lower = file_name.to_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else {
        "application/octet-stream"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_text_hash_deterministic_and_trims() {
        let hash1 = compute_text_hash("When code works on first try");
        let hash2 = compute_text_hash("   When code works on first try   ");
        let hash3 = compute_text_hash("When code fails");

        assert_eq!(hash1, hash2, "Whitespace trimming should produce identical hashes");
        assert_ne!(hash1, hash3, "Different texts must produce different hashes");
        assert_eq!(hash1.len(), 64, "SHA-256 hex string must be 64 characters");
    }

    #[test]
    fn test_compute_file_hash() {
        let bytes = b"dummy image file content";
        let hash1 = compute_file_hash(bytes);
        let hash2 = compute_file_hash(bytes);

        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[test]
    fn test_detect_content_type() {
        assert_eq!(detect_content_type("meme.PNG"), "image/png");
        assert_eq!(detect_content_type("photo.jpg"), "image/jpeg");
        assert_eq!(detect_content_type("photo.JPEG"), "image/jpeg");
        assert_eq!(detect_content_type("dog.webp"), "image/webp");
        assert_eq!(detect_content_type("cat.gif"), "image/gif");
        assert_eq!(detect_content_type("vector.svg"), "image/svg+xml");
        assert_eq!(detect_content_type("data.bin"), "application/octet-stream");
    }

    #[test]
    fn test_deserialize_situation_configs_formats() {
        let json_detailed = r#"{
            "name": "Test Pack",
            "language_code": "en",
            "items": [
                { "prompt_text": "Detailed item 1" },
                { "prompt_text": "Detailed item 2" }
            ]
        }"#;

        let parsed_detailed: SituationPackSeedConfig = serde_json::from_str(json_detailed).unwrap();
        assert_eq!(parsed_detailed.items.len(), 2);
        assert_eq!(parsed_detailed.items[0].prompt_text(), "Detailed item 1");
        assert_eq!(parsed_detailed.safety_level, ContentSafetyLevel::FamilyFriendly);
        assert!(parsed_detailed.is_public);

        let json_simple = r#"{
            "name": "Test Pack Simple",
            "language_code": "ru",
            "safety_level": "spicy",
            "is_public": false,
            "items": [
                "Simple item 1",
                "Simple item 2"
            ]
        }"#;

        let parsed_simple: SituationPackSeedConfig = serde_json::from_str(json_simple).unwrap();
        assert_eq!(parsed_simple.items.len(), 2);
        assert_eq!(parsed_simple.items[0].prompt_text(), "Simple item 1");
        assert_eq!(parsed_simple.safety_level, ContentSafetyLevel::Spicy);
        assert!(!parsed_simple.is_public);
    }

    #[test]
    fn test_deserialize_meme_configs_formats() {
        let json_detailed = r#"{
            "name": "Test Meme Pack",
            "language_code": "en",
            "items": [
                { "file": "assets/meme1.png" },
                { "file": "assets/meme2.jpg" }
            ]
        }"#;

        let parsed_detailed: MemePackSeedConfig = serde_json::from_str(json_detailed).unwrap();
        assert_eq!(parsed_detailed.items.len(), 2);
        assert_eq!(parsed_detailed.items[0].file_path(), "assets/meme1.png");

        let json_simple = r#"{
            "name": "Test Meme Pack Simple",
            "language_code": "ru",
            "items": [
                "assets/meme1.png"
            ]
        }"#;

        let parsed_simple: MemePackSeedConfig = serde_json::from_str(json_simple).unwrap();
        assert_eq!(parsed_simple.items.len(), 1);
        assert_eq!(parsed_simple.items[0].file_path(), "assets/meme1.png");
    }
}
