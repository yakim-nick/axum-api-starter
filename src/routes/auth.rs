use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::auth::{AuthUser, create_token, dummy_hash, hash_password, verify_password};
use crate::error::AppError;
use crate::extract::ApiJson;
use crate::state::AppState;
use crate::{repo, validation};

#[derive(Debug, Deserialize)]
pub struct Credentials {
    email: String,
    password: String,
}

#[derive(Debug, Serialize)]
pub struct UserView {
    id: Uuid,
    email: String,
}

#[derive(Debug, Serialize)]
pub struct AuthResponse {
    token: String,
    user: UserView,
}

pub async fn register(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<Credentials>,
) -> Result<(StatusCode, Json<AuthResponse>), AppError> {
    let email = validation::email(&body.email)?;
    let password = validation::password(&body.password)?;

    // Hashing is CPU-heavy: keep it off the async runtime threads.
    let password = password.to_owned();
    let hash = tokio::task::spawn_blocking(move || hash_password(&password))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))??;

    let user = repo::create_user(&state.pool, &email, &hash)
        .await?
        .ok_or(AppError::Conflict(
            "An account with this email already exists",
        ))?;
    let token = create_token(user.id, &state.config.jwt_secret, state.config.jwt_ttl_secs)?;
    Ok((
        StatusCode::CREATED,
        Json(AuthResponse {
            token,
            user: UserView {
                id: user.id,
                email: user.email,
            },
        }),
    ))
}

pub async fn login(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<Credentials>,
) -> Result<Json<AuthResponse>, AppError> {
    const BAD_CREDENTIALS: AppError = AppError::Unauthorized("Invalid email or password");

    let email = body.email.trim().to_lowercase();
    let user = repo::find_user_by_email(&state.pool, &email).await?;

    // Verify against a dummy hash when the user does not exist, so both failure
    // paths cost the same and look the same.
    let stored = user.as_ref().map(|u| u.password_hash.clone());
    let password = body.password;
    let ok = tokio::task::spawn_blocking(move || match stored {
        Some(hash) => verify_password(&password, &hash),
        None => {
            verify_password(&password, dummy_hash());
            false
        }
    })
    .await
    .map_err(|e| AppError::Internal(e.to_string()))?;

    let user = match user {
        Some(user) if ok => user,
        _ => return Err(BAD_CREDENTIALS),
    };
    let token = create_token(user.id, &state.config.jwt_secret, state.config.jwt_ttl_secs)?;
    Ok(Json(AuthResponse {
        token,
        user: UserView {
            id: user.id,
            email: user.email,
        },
    }))
}

pub async fn me(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<Json<UserView>, AppError> {
    // The token may outlive the account, so check the user still exists.
    let user = repo::find_user_by_id(&state.pool, user_id)
        .await?
        .ok_or(AppError::Unauthorized("Account no longer exists"))?;
    Ok(Json(UserView {
        id: user.id,
        email: user.email,
    }))
}
