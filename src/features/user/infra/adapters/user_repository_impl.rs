use async_trait::async_trait;
use chrono::{DateTime, Utc};
use sqlx::{FromRow, PgPool, QueryBuilder};

use crate::{
    common::http::error::AppError,
    features::user::{SearchUser, UpdateUserProfile, User, UserRepository},
};

#[derive(Clone)]
pub struct UserRepositoryImpl {
    pool: PgPool,
}

impl UserRepositoryImpl {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

const FIND_USER_QUERY: &str = r#"
    SELECT
        u.id::text AS id,
        u.username,
        u.created_at,
        u.modified_at
    FROM users u
    WHERE 1=1
    "#;

const FIND_USER_INFO_QUERY: &str = r#"
    SELECT
        u.id::text AS id,
        u.username,
        u.created_at,
        u.modified_at
    FROM users u
    WHERE u.id = $1::uuid
    "#;

#[derive(Debug, FromRow)]
struct UserRow {
    id: String,
    username: Option<String>,
    created_at: Option<DateTime<Utc>>,
    modified_at: Option<DateTime<Utc>>,
}

impl From<UserRow> for User {
    fn from(row: UserRow) -> Self {
        Self {
            id: row.id,
            username: row.username,
            created_at: row.created_at,
            modified_at: row.modified_at,
        }
    }
}

fn map_user_write_error(err: sqlx::Error) -> AppError {
    match &err {
        sqlx::Error::Database(db_err) if db_err.constraint() == Some("users_username_key") => {
            AppError::ValidationError("Username already exists".into())
        }
        _ => AppError::DatabaseError(err),
    }
}

#[async_trait]
impl UserRepository for UserRepositoryImpl {
    async fn find_all(&self) -> Result<Vec<User>, AppError> {
        let users = sqlx::query_as::<_, UserRow>(FIND_USER_QUERY)
            .fetch_all(&self.pool)
            .await?;

        Ok(users.into_iter().map(Into::into).collect())
    }

    async fn find_by_id(&self, id: &str) -> Result<Option<User>, AppError> {
        let user = sqlx::query_as::<_, UserRow>(FIND_USER_INFO_QUERY)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;

        Ok(user.map(Into::into))
    }

    async fn find_list(&self, search: SearchUser) -> Result<Vec<User>, AppError> {
        let mut builder = QueryBuilder::<sqlx::Postgres>::new(FIND_USER_QUERY);

        if let Some(value) = search
            .id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder.push(" AND u.id = ");
            builder.push_bind(value);
            builder.push("::uuid");
        }

        if let Some(value) = search
            .username
            .as_deref()
            .filter(|value| !value.trim().is_empty())
        {
            builder.push(" AND u.username like ");
            builder.push_bind(format!("%{}%", value));
        }

        let query = builder.build_query_as::<UserRow>();
        let users = query.fetch_all(&self.pool).await?;

        Ok(users.into_iter().map(Into::into).collect())
    }

    async fn update_profile(
        &self,
        id: &str,
        update: UpdateUserProfile,
    ) -> Result<Option<User>, AppError> {
        let user = sqlx::query_as::<_, UserRow>(
            r#"
            UPDATE users
            SET
                username = COALESCE($2, username),
                modified_at = CURRENT_TIMESTAMP
            WHERE id = $1::uuid
            RETURNING
                id::text AS id,
                username,
                created_at,
                modified_at
            "#,
        )
        .bind(id)
        .bind(update.username)
        .fetch_optional(&self.pool)
        .await
        .map_err(map_user_write_error)?;

        Ok(user.map(Into::into))
    }

    async fn promote_to_admin(&self, id: &str) -> Result<Option<User>, AppError> {
        let user = sqlx::query_as::<_, UserRow>(
            r#"
            UPDATE users
            SET
                role = 'admin',
                modified_at = CURRENT_TIMESTAMP
            WHERE id = $1::uuid
            RETURNING
                id::text AS id,
                username,
                created_at,
                modified_at
            "#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(user.map(Into::into))
    }
}
