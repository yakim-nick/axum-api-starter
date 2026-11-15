//! Database access. All SQL lives here. Note queries always filter by `user_id`,
//! so ownership is enforced by the query itself: another user's note simply
//! "does not exist" (404), which also avoids leaking that the id is valid.

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::AppError;

#[derive(Debug, Clone, FromRow)]
pub struct UserRow {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct Note {
    pub id: Uuid,
    pub title: String,
    pub body: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Insert a user. `Ok(None)` means the email is already taken.
pub async fn create_user(
    pool: &PgPool,
    email: &str,
    password_hash: &str,
) -> Result<Option<UserRow>, AppError> {
    let result = sqlx::query_as::<_, UserRow>(
        "INSERT INTO users (email, password_hash) VALUES ($1, $2) RETURNING id, email, password_hash",
    )
    .bind(email)
    .bind(password_hash)
    .fetch_one(pool)
    .await;
    match result {
        Ok(user) => Ok(Some(user)),
        Err(sqlx::Error::Database(db)) if db.is_unique_violation() => Ok(None),
        Err(other) => Err(other.into()),
    }
}

pub async fn find_user_by_email(pool: &PgPool, email: &str) -> Result<Option<UserRow>, AppError> {
    Ok(
        sqlx::query_as("SELECT id, email, password_hash FROM users WHERE lower(email) = $1")
            .bind(email)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn find_user_by_id(pool: &PgPool, id: Uuid) -> Result<Option<UserRow>, AppError> {
    Ok(
        sqlx::query_as("SELECT id, email, password_hash FROM users WHERE id = $1")
            .bind(id)
            .fetch_optional(pool)
            .await?,
    )
}

pub async fn create_note(
    pool: &PgPool,
    user_id: Uuid,
    title: &str,
    body: &str,
) -> Result<Note, AppError> {
    Ok(sqlx::query_as(
        "INSERT INTO notes (user_id, title, body) VALUES ($1, $2, $3) \
         RETURNING id, title, body, created_at, updated_at",
    )
    .bind(user_id)
    .bind(title)
    .bind(body)
    .fetch_one(pool)
    .await?)
}

pub async fn list_notes(
    pool: &PgPool,
    user_id: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<Note>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, title, body, created_at, updated_at FROM notes WHERE user_id = $1 \
         ORDER BY created_at DESC, id LIMIT $2 OFFSET $3",
    )
    .bind(user_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?)
}

pub async fn get_note(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<Option<Note>, AppError> {
    Ok(sqlx::query_as(
        "SELECT id, title, body, created_at, updated_at FROM notes WHERE id = $1 AND user_id = $2",
    )
    .bind(id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?)
}

/// Partial update: `None` leaves a column unchanged.
pub async fn update_note(
    pool: &PgPool,
    user_id: Uuid,
    id: Uuid,
    title: Option<&str>,
    body: Option<&str>,
) -> Result<Option<Note>, AppError> {
    Ok(sqlx::query_as(
        "UPDATE notes SET title = COALESCE($3, title), body = COALESCE($4, body), updated_at = now() \
         WHERE id = $1 AND user_id = $2 RETURNING id, title, body, created_at, updated_at",
    )
    .bind(id)
    .bind(user_id)
    .bind(title)
    .bind(body)
    .fetch_optional(pool)
    .await?)
}

/// Returns true when a row was deleted.
pub async fn delete_note(pool: &PgPool, user_id: Uuid, id: Uuid) -> Result<bool, AppError> {
    let result = sqlx::query("DELETE FROM notes WHERE id = $1 AND user_id = $2")
        .bind(id)
        .bind(user_id)
        .execute(pool)
        .await?;
    Ok(result.rows_affected() > 0)
}
