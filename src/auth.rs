//! Password hashing (Argon2id), JWT creation/verification and the `AuthUser` extractor.

use std::sync::OnceLock;

use argon2::password_hash::phc::PasswordHash;
use argon2::{Argon2, PasswordHasher, PasswordVerifier};
use axum::extract::FromRequestParts;
use axum::http::header::AUTHORIZATION;
use axum::http::request::Parts;
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::AppError;
use crate::state::AppState;

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: the user id.
    pub sub: Uuid,
    pub iat: u64,
    pub exp: u64,
}

pub fn hash_password(password: &str) -> Result<String, AppError> {
    Argon2::default()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|e| AppError::Internal(format!("password hashing failed: {e}")))
}

/// Check a password against a stored PHC hash string. A malformed hash counts as a mismatch.
pub fn verify_password(password: &str, stored_hash: &str) -> bool {
    match PasswordHash::new(stored_hash) {
        Ok(parsed) => Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok(),
        Err(_) => false,
    }
}

/// A valid hash of a random password, used to spend the same time verifying when the
/// email is unknown, so response timing does not reveal which emails are registered.
pub fn dummy_hash() -> &'static str {
    static HASH: OnceLock<String> = OnceLock::new();
    HASH.get_or_init(|| hash_password("dummy-password-for-timing").expect("hashing works"))
}

pub fn create_token(user_id: Uuid, secret: &str, ttl_secs: u64) -> Result<String, AppError> {
    let now = unix_now();
    let claims = Claims {
        sub: user_id,
        iat: now,
        exp: now + ttl_secs,
    };
    let key = EncodingKey::from_secret(secret.as_bytes())
        .map_err(|e| AppError::Internal(format!("invalid signing key: {e}")))?;
    encode(&Header::new(Algorithm::HS256), &claims, &key)
        .map_err(|e| AppError::Internal(format!("token signing failed: {e}")))
}

/// Verify signature, algorithm (pinned to HS256) and expiry.
pub fn verify_token(token: &str, secret: &str) -> Result<Claims, AppError> {
    let mut validation = Validation::new(Algorithm::HS256);
    validation.leeway = 0;
    validation.set_required_spec_claims(&["exp", "sub"]);
    const INVALID: AppError = AppError::Unauthorized("Invalid or expired token");
    let key = DecodingKey::from_secret(secret.as_bytes()).map_err(|_| INVALID)?;
    decode::<Claims>(token, &key, &validation)
        .map(|data| data.claims)
        .map_err(|_| INVALID)
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Extractor: put `AuthUser` in a handler's arguments to require a valid Bearer token.
#[derive(Debug, Clone, Copy)]
pub struct AuthUser(pub Uuid);

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized("Missing Authorization header"))?;
        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized(
                "Expected 'Authorization: Bearer <token>'",
            ))?;
        let claims = verify_token(token, &state.config.jwt_secret)?;
        Ok(AuthUser(claims.sub))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SECRET: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn hash_then_verify_round_trip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(hash.starts_with("$argon2id$"), "{hash}");
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong", &hash));
    }

    #[test]
    fn two_hashes_of_the_same_password_differ_because_of_the_salt() {
        assert_ne!(
            hash_password("same").unwrap(),
            hash_password("same").unwrap()
        );
    }

    #[test]
    fn malformed_hash_is_a_mismatch_not_a_panic() {
        assert!(!verify_password("x", "not-a-hash"));
        assert!(!verify_password("x", ""));
    }

    #[test]
    fn token_round_trip_keeps_the_user_id() {
        let id = Uuid::new_v4();
        let token = create_token(id, SECRET, 60).unwrap();
        assert_eq!(verify_token(&token, SECRET).unwrap().sub, id);
    }

    #[test]
    fn token_signed_with_another_secret_is_rejected() {
        let token = create_token(Uuid::new_v4(), "another-secret-another-secret-123", 60).unwrap();
        assert!(verify_token(&token, SECRET).is_err());
    }

    #[test]
    fn expired_token_is_rejected() {
        let claims = Claims {
            sub: Uuid::new_v4(),
            iat: 1,
            exp: 2,
        };
        let token = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()).unwrap(),
        )
        .unwrap();
        assert!(verify_token(&token, SECRET).is_err());
    }

    #[test]
    fn garbage_and_other_algorithms_are_rejected() {
        assert!(verify_token("garbage", SECRET).is_err());
        let claims = Claims {
            sub: Uuid::new_v4(),
            iat: 0,
            exp: unix_now() + 60,
        };
        let hs512 = encode(
            &Header::new(Algorithm::HS512),
            &claims,
            &EncodingKey::from_secret(SECRET.as_bytes()).unwrap(),
        )
        .unwrap();
        assert!(
            verify_token(&hs512, SECRET).is_err(),
            "algorithm must be pinned to HS256"
        );
    }
}
