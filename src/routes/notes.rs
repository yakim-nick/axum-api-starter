use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::Deserialize;
use uuid::Uuid;

use crate::auth::AuthUser;
use crate::error::AppError;
use crate::extract::{ApiJson, ApiPath, ApiQuery};
use crate::repo::{self, Note};
use crate::state::AppState;
use crate::validation;

const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 100;

#[derive(Debug, Deserialize)]
pub struct CreateNote {
    title: String,
    #[serde(default)]
    body: String,
}

#[derive(Debug, Deserialize)]
pub struct UpdateNote {
    title: Option<String>,
    body: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct Page {
    limit: Option<i64>,
    offset: Option<i64>,
}

pub async fn create(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ApiJson(input): ApiJson<CreateNote>,
) -> Result<(StatusCode, Json<Note>), AppError> {
    let title = validation::title(&input.title)?;
    let body = validation::body(&input.body)?;
    let note = repo::create_note(&state.pool, user_id, &title, &body).await?;
    Ok((StatusCode::CREATED, Json(note)))
}

pub async fn list(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ApiQuery(page): ApiQuery<Page>,
) -> Result<Json<Vec<Note>>, AppError> {
    let limit = page.limit.unwrap_or(DEFAULT_LIMIT);
    let offset = page.offset.unwrap_or(0);
    if !(1..=MAX_LIMIT).contains(&limit) || offset < 0 {
        return Err(AppError::Validation(format!(
            "limit must be 1-{MAX_LIMIT} and offset >= 0"
        )));
    }
    Ok(Json(
        repo::list_notes(&state.pool, user_id, limit, offset).await?,
    ))
}

pub async fn get_one(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<Note>, AppError> {
    repo::get_note(&state.pool, user_id, id)
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

pub async fn update(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ApiPath(id): ApiPath<Uuid>,
    ApiJson(input): ApiJson<UpdateNote>,
) -> Result<Json<Note>, AppError> {
    if input.title.is_none() && input.body.is_none() {
        return Err(AppError::Validation("provide title and/or body".into()));
    }
    let title = input.title.as_deref().map(validation::title).transpose()?;
    let body = input.body.as_deref().map(validation::body).transpose()?;
    repo::update_note(&state.pool, user_id, id, title.as_deref(), body.as_deref())
        .await?
        .map(Json)
        .ok_or(AppError::NotFound)
}

pub async fn delete(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    if repo::delete_note(&state.pool, user_id, id).await? {
        Ok(StatusCode::NO_CONTENT)
    } else {
        Err(AppError::NotFound)
    }
}
