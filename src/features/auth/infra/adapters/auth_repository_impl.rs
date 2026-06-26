use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool};

use crate::{
    common::http::{error::AppError, role::Role},
    features::auth::{
        domain::model::{RefreshTokenRecord, UserAuth},
        AuthRepository,
    },
};

#[derive(Clone)]
pub struct AuthRepositoryImpl {
    pool: PgPool,
}

impl AuthRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[derive(Debug, FromRow)]
struct UserAuthRow {
    user_id: String,
    password_hash: Option<String>,
    role: String,
}

impl From<UserAuthRow> for UserAuth {
    fn from(row: UserAuthRow) -> Self {
        Self {
            user_id: row.user_id,
            password_hash: row.password_hash,
            role: Role::from_db(&row.role),
        }
    }
}

#[derive(Debug, FromRow)]
struct RefreshTokenRow {
    user_id: String,
    family_id: uuid::Uuid,
    is_used: bool,
}

impl From<RefreshTokenRow> for RefreshTokenRecord {
    fn from(row: RefreshTokenRow) -> Self {
        Self {
            user_id: row.user_id,
            family_id: row.family_id,
            is_used: row.is_used,
        }
    }
}

#[async_trait]
impl AuthRepository for AuthRepositoryImpl {
    async fn create_user_with_auth(
        &self,
        username: String,
        handle: String,
        password_hash: Option<String>,
    ) -> Result<String, AppError> {
        let mut tx = self.pool.begin().await?;

        let existing_user_id: Option<String> =
            sqlx::query_scalar("SELECT id::text FROM users WHERE handle = $1")
                .bind(&handle)
                .fetch_optional(&mut *tx)
                .await?;

        if existing_user_id.is_some() {
            tx.rollback().await?;
            return Err(AppError::UserAlreadyExists);
        }

        let user_id = uuid::Uuid::new_v4().to_string();

        sqlx::query(
            r#"
                INSERT INTO users (id, username, handle, role)
                VALUES ($1::uuid, $2, $3, 'user')
            "#,
        )
        .bind(&user_id)
        .bind(username)
        .bind(handle)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            r#"
                INSERT INTO user_auth (user_id, password_hash)
                VALUES ($1::uuid, $2)
            "#,
        )
        .bind(&user_id)
        .bind(password_hash)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(user_id)
    }

    async fn find_by_handle(&self, handle: &str) -> Result<Option<UserAuth>, AppError> {
        let result = sqlx::query_as::<_, UserAuthRow>(
            r#"
                SELECT ua.user_id::text AS user_id, ua.password_hash, u.role::text AS role
                FROM user_auth ua
                JOIN users u ON ua.user_id = u.id
                WHERE u.handle = $1
            "#,
        )
        .bind(handle)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(Into::into))
    }

    async fn find_by_id(&self, user_id: &str) -> Result<Option<UserAuth>, AppError> {
        let result = sqlx::query_as::<_, UserAuthRow>(
            r#"
                SELECT ua.user_id::text AS user_id, ua.password_hash, u.role::text AS role
                FROM user_auth ua
                JOIN users u ON ua.user_id = u.id
                WHERE ua.user_id = $1::uuid
            "#,
        )
        .bind(user_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(Into::into))
    }

    async fn save_refresh_token(
        &self,
        user_id: String,
        token_hash: String,
        family_id: uuid::Uuid,
        expires_at: DateTime<Utc>,
    ) -> Result<(), AppError> {
        sqlx::query(
            r#"
                INSERT INTO refresh_tokens (user_id, token_hash, family_id, expires_at)
                VALUES ($1::uuid, $2, $3, $4)
            "#,
        )
        .bind(user_id)
        .bind(token_hash)
        .bind(family_id)
        .bind(expires_at)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn find_refresh_token(
        &self,
        token_hash: &str,
    ) -> Result<Option<RefreshTokenRecord>, AppError> {
        let result = sqlx::query_as::<_, RefreshTokenRow>(
            r#"
                SELECT user_id::text AS user_id, family_id, is_used
                FROM refresh_tokens
                WHERE token_hash = $1 AND expires_at > NOW()
            "#,
        )
        .bind(token_hash)
        .fetch_optional(&self.pool)
        .await?;

        Ok(result.map(Into::into))
    }

    async fn mark_token_as_used(&self, token_hash: &str) -> Result<(), AppError> {
        sqlx::query("UPDATE refresh_tokens SET is_used = TRUE WHERE token_hash = $1")
            .bind(token_hash)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    async fn revoke_token_family(&self, family_id: uuid::Uuid) -> Result<(), AppError> {
        sqlx::query("DELETE FROM refresh_tokens WHERE family_id = $1")
            .bind(family_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }
}
