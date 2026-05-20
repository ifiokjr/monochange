//! API layer — session management, JWT auth, middleware.
//!
//! Provides:
//! - JWT token creation and verification
//! - Session cookie management
//! - Auth extraction middleware

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;

use std::sync::Arc;

use axum::http::StatusCode;
use chrono::Duration;
use chrono::Utc;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use jsonwebtoken::decode;
use jsonwebtoken::encode;
use serde::Deserialize;
use serde::Serialize;

pub mod secrets {
	secretspec_derive::declare_secrets!("../../secretspec.toml");
}

pub use secrets::SecretSpec as AppSecrets;

/// Load application secrets through the SecretSpec SDK.
pub fn load_app_secrets() -> Result<secretspec::Resolved<AppSecrets>, secretspec::SecretSpecError> {
	secrets::SecretSpec::builder().load()
}

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
	pub db: monochange_app_db::DbPool,
	pub secrets: Arc<AppSecrets>,
	pub jwt_secret: String,
	pub github_client_id: String,
	pub github_client_secret: String,
}

impl AppState {
	pub fn new(db: monochange_app_db::DbPool, secrets: AppSecrets) -> Self {
		let jwt_secret = secrets.jwt_secret.clone().unwrap_or_default();
		let github_client_id = secrets.github_client_id.clone().unwrap_or_default();
		let github_client_secret = secrets.github_client_secret.clone().unwrap_or_default();

		Self {
			db,
			secrets: Arc::new(secrets),
			jwt_secret,
			github_client_id,
			github_client_secret,
		}
	}
}
