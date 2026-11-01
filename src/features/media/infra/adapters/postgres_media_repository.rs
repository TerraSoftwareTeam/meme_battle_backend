use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::{
    common::http::error::AppError,
    features::media::{
        CreateMediaAsset, MediaAsset, MediaProvider, MediaPurgeTask, MediaRepository, MediaStatus,
        MediaVisibility, StoredFile,
    },
};

#[derive(Clone)]
pub struct PostgresMediaRepository {
    pool: PgPool,
}

impl PostgresMediaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct MediaAssetRow {
    id: i64,
    owner_user_id: String,
    provider: String,
    provider_file_id: String,
    url: String,
    filename: String,
    content_type: String,
    size_bytes: i64,
    status: String,
    visibility: String,
    created_at: DateTime<Utc>,
}

impl TryFrom<MediaAssetRow> for MediaAsset {
    type Error = AppError;

    fn try_from(row: MediaAssetRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            owner_user_id: row.owner_user_id,
            provider: MediaProvider::try_from(row.provider).map_err(AppError::ProviderError)?,
            provider_file_id: row.provider_file_id,
            url: row.url,
            filename: row.filename,
            content_type: row.content_type,
            size_bytes: row.size_bytes,
            status: MediaStatus::try_from(row.status).map_err(AppError::ProviderError)?,
            visibility: MediaVisibility::try_from(row.visibility)
                .map_err(AppError::ProviderError)?,
            created_at: row.created_at,
        })
    }
}

#[derive(Debug, FromRow)]
struct MediaPurgeTaskRow {
    id: Uuid,
    media_asset_id: i64,
    provider: String,
    provider_file_id: String,
    retry_count: i32,
    next_retry_at: DateTime<Utc>,
    last_error: Option<String>,
    created_at: DateTime<Utc>,
}

impl TryFrom<MediaPurgeTaskRow> for MediaPurgeTask {
    type Error = AppError;

    fn try_from(row: MediaPurgeTaskRow) -> Result<Self, Self::Error> {
        Ok(Self {
            id: row.id,
            media_asset_id: row.media_asset_id,
            provider: MediaProvider::try_from(row.provider).map_err(AppError::ProviderError)?,
            provider_file_id: row.provider_file_id,
            retry_count: row.retry_count,
            next_retry_at: row.next_retry_at,
            last_error: row.last_error,
            created_at: row.created_at,
        })
    }
}

#[async_trait]
impl MediaRepository for PostgresMediaRepository {
    async fn create(&self, asset: CreateMediaAsset) -> Result<MediaAsset, AppError> {
        let StoredFile {
            provider,
            provider_file_id,
            url,
            filename,
            content_type,
            size_bytes,
        } = asset.stored_file;

        let row = sqlx::query_as::<_, MediaAssetRow>(
            r#"
            INSERT INTO media_assets (
                owner_user_id,
                provider,
                provider_file_id,
                url,
                filename,
                content_type,
                size_bytes
            )
            VALUES ($1::uuid, $2, $3, $4, $5, $6, $7)
            RETURNING
                id,
                owner_user_id::text AS owner_user_id,
                provider,
                provider_file_id,
                url,
                filename,
                content_type,
                size_bytes,
                status::text AS status,
                visibility::text AS visibility,
                created_at
            "#,
        )
        .bind(asset.owner_user_id)
        .bind(provider.as_str())
        .bind(provider_file_id)
        .bind(url)
        .bind(filename)
        .bind(content_type)
        .bind(size_bytes)
        .fetch_one(&self.pool)
        .await?;

        row.try_into()
    }

    async fn find_by_id(&self, id: i64) -> Result<Option<MediaAsset>, AppError> {
        let row = sqlx::query_as::<_, MediaAssetRow>(
            r#"
            SELECT
                id,
                owner_user_id::text AS owner_user_id,
                provider,
                provider_file_id,
                url,
                filename,
                content_type,
                size_bytes,
                status::text AS status,
                visibility::text AS visibility,
                created_at
            FROM media_assets
            WHERE id = $1
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    async fn find_by_id_for_owner(
        &self,
        id: i64,
        owner_user_id: &str,
    ) -> Result<Option<MediaAsset>, AppError> {
        let row = sqlx::query_as::<_, MediaAssetRow>(
            r#"
            SELECT
                id,
                owner_user_id::text AS owner_user_id,
                provider,
                provider_file_id,
                url,
                filename,
                content_type,
                size_bytes,
                status::text AS status,
                visibility::text AS visibility,
                created_at
            FROM media_assets
            WHERE id = $1 AND owner_user_id = $2::uuid
            "#,
        )
        .bind(id)
        .bind(owner_user_id)
        .fetch_optional(&self.pool)
        .await?;

        row.map(TryInto::try_into).transpose()
    }

    async fn list_by_owner(&self, owner_user_id: &str) -> Result<Vec<MediaAsset>, AppError> {
        let rows = sqlx::query_as::<_, MediaAssetRow>(
            r#"
            SELECT
                id,
                owner_user_id::text AS owner_user_id,
                provider,
                provider_file_id,
                url,
                filename,
                content_type,
                size_bytes,
                status::text AS status,
                visibility::text AS visibility,
                created_at
            FROM media_assets
            WHERE owner_user_id = $1::uuid
            ORDER BY created_at DESC, id DESC
            "#,
        )
        .bind(owner_user_id)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn delete_by_id_for_owner(&self, id: i64, owner_user_id: &str) -> Result<bool, AppError> {
        let result = sqlx::query(
            r#"
            DELETE FROM media_assets
            WHERE id = $1 AND owner_user_id = $2::uuid
            "#,
        )
        .bind(id)
        .bind(owner_user_id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn provider_exists(
        &self,
        provider: MediaProvider,
        provider_file_id: &str,
    ) -> Result<bool, AppError> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM media_assets
                WHERE provider = $1 AND provider_file_id = $2
            )
            "#,
        )
        .bind(provider.as_str())
        .bind(provider_file_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    async fn mark_attached(&self, ids: &[i64]) -> Result<(), AppError> {
        if ids.is_empty() {
            return Ok(());
        }

        sqlx::query(
            r#"
            UPDATE media_assets
            SET status = 'attached', visibility = 'public'
            WHERE id = ANY($1)
            "#,
        )
        .bind(ids)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn validate_exists(&self, ids: &[i64]) -> Result<(), AppError> {
        if ids.is_empty() {
            return Ok(());
        }

        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM media_assets
            WHERE id = ANY($1)
            "#,
        )
        .bind(ids)
        .fetch_one(&self.pool)
        .await?;

        if count != ids.len() as i64 {
            return Err(AppError::NotFound("Media assets not found".to_string()));
        }

        Ok(())
    }

    async fn validate_ownership(&self, ids: &[i64], owner_user_id: &str) -> Result<(), AppError> {
        if ids.is_empty() {
            return Ok(());
        }

        let count = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM media_assets
            WHERE id = ANY($1) AND owner_user_id = $2::uuid
            "#,
        )
        .bind(ids)
        .bind(owner_user_id)
        .fetch_one(&self.pool)
        .await?;

        if count != ids.len() as i64 {
            return Err(AppError::NotFound("Media assets not found".to_string()));
        }

        Ok(())
    }

    async fn delete_by_id(&self, id: i64) -> Result<bool, AppError> {
        let result = sqlx::query(
            r#"
            DELETE FROM media_assets
            WHERE id = $1
            "#,
        )
        .bind(id)
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected() > 0)
    }

    async fn enqueue_purge_tasks_for_media_ids(&self, media_ids: &[i64]) -> Result<(), AppError> {
        if media_ids.is_empty() {
            return Ok(());
        }

        sqlx::query(
            r#"
            INSERT INTO media_purge_tasks (media_asset_id, provider, provider_file_id)
            SELECT id, provider, provider_file_id
            FROM media_assets
            WHERE id = ANY($1)
            "#,
        )
        .bind(media_ids)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn fetch_stale_pending_media(
        &self,
        older_than: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<MediaAsset>, AppError> {
        let rows = sqlx::query_as::<_, MediaAssetRow>(
            r#"
            SELECT
                id,
                owner_user_id::text AS owner_user_id,
                provider,
                provider_file_id,
                url,
                filename,
                content_type,
                size_bytes,
                status::text AS status,
                visibility::text AS visibility,
                created_at
            FROM media_assets
            WHERE status = 'pending'
              AND created_at < $1
            ORDER BY created_at ASC
            LIMIT $2
            "#,
        )
        .bind(older_than)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn fetch_pending_purge_tasks(&self, limit: i64) -> Result<Vec<MediaPurgeTask>, AppError> {
        let rows = sqlx::query_as::<_, MediaPurgeTaskRow>(
            r#"
            SELECT id, media_asset_id, provider, provider_file_id, retry_count, next_retry_at, last_error, created_at
            FROM media_purge_tasks
            WHERE next_retry_at <= now() AND retry_count <= 5
            ORDER BY next_retry_at ASC
            LIMIT $1
            "#,
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        rows.into_iter().map(TryInto::try_into).collect()
    }

    async fn complete_purge_task(&self, task_id: Uuid) -> Result<(), AppError> {
        sqlx::query(
            r#"
            DELETE FROM media_purge_tasks
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn fail_purge_task(
        &self,
        task_id: Uuid,
        error: &str,
        next_retry_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
            UPDATE media_purge_tasks
            SET retry_count = retry_count + 1,
                last_error = $2,
                next_retry_at = $3
            WHERE id = $1
            "#,
        )
        .bind(task_id)
        .bind(error)
        .bind(next_retry_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn is_media_used_in_active_packs(&self, media_asset_id: i64) -> Result<bool, AppError> {
        let exists = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM pack_memes pm
                JOIN meme_packs mp ON mp.id = pm.pack_id
                WHERE pm.media_id = $1
                  AND pm.deleted_at IS NULL
                  AND mp.deleted_at IS NULL
            )
            "#,
        )
        .bind(media_asset_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(exists)
    }

    async fn is_media_locked_in_games(&self, media_asset_id: i64) -> Result<bool, AppError> {
        let locked = sqlx::query_scalar::<_, bool>(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM game_content_locks gcl
                JOIN pack_memes pm ON pm.id = gcl.meme_id
                WHERE pm.media_id = $1
            )
            "#,
        )
        .bind(media_asset_id)
        .fetch_one(&self.pool)
        .await?;

        Ok(locked)
    }
}
