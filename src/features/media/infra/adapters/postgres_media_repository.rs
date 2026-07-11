use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

use crate::{
    common::http::error::AppError,
    features::media::{
        CreateMediaAsset, MediaAsset, MediaProvider, MediaRepository, MediaStatus, MediaVisibility, StoredFile,
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
            visibility: MediaVisibility::try_from(row.visibility).map_err(AppError::ProviderError)?,
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
}
