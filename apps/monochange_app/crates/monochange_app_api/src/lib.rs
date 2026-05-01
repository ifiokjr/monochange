//! API layer — session management, JWT auth, middleware.
//!
//! Provides:
//! - JWT token creation and verification
//! - Session cookie management
//! - Auth extraction middleware

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;

use axum::http::StatusCode;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT claims stored in the session token.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject = user's DB id
    pub sub: i32,
    /// GitHub user ID
    pub github_id: i64,
    /// GitHub login
    pub github_login: String,
    /// Expiration timestamp
    pub exp: usize,
    /// Issued at timestamp
    pub iat: usize,
}

/// Create a JWT for a user session.
pub fn create_token(
    secret: &str,
    user_id: i32,
    github_id: i64,
    github_login: &str,
) -> Result<String, jsonwebtoken::errors::Error> {
    let now = Utc::now();
    let exp = now + Duration::days(7);

    let claims = Claims {
        sub: user_id,
        github_id,
        github_login: github_login.to_string(),
        exp: exp.timestamp() as usize,
        iat: now.timestamp() as usize,
    };

    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
}

/// Verify a JWT and extract claims.
pub fn verify_token(secret: &str, token: &str) -> Result<Claims, StatusCode> {
    decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map(|data| data.claims)
    .map_err(|_| StatusCode::UNAUTHORIZED)
}

/// Application state shared across requests.
#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub jwt_secret: String,
    pub github_client_id: String,
    pub github_client_secret: String,
}

impl AppState {
    pub fn new(
        db: sqlx::PgPool,
        jwt_secret: String,
        github_client_id: String,
        github_client_secret: String,
    ) -> Self {
        Self {
            db,
            jwt_secret,
            github_client_id,
            github_client_secret,
        }
    }
}
