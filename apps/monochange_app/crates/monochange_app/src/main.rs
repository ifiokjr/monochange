//! monochange_app server binary.
//!
//! Starts an axum HTTP server with Leptos SSR integration,
//! SQLite connection pool, and JWT session management.
//!
//! ## Secret management
//!
//! Secrets are declared in `secretspec.toml` and loaded in-process via
//! the SecretSpec Rust SDK. Production uses the OnePassword provider;
//! Docker injects only the 1Password service account token as a Docker
//! secret.
//!
//! ```bash
//! # Development (uses keyring provider with local defaults)
//! secretspec run --profile development -- cargo leptos watch
//!
//! # Production (uses the SecretSpec SDK and 1Password provider)
//! SECRETSPEC_PROFILE=production SECRETSPEC_PROVIDER=onepassword://monochange ./monochange_app
//!
//! # CI (uses environment variables)
//! secretspec run --profile ci --provider env -- cargo leptos build
//! ```

use std::sync::Arc;

use axum::Router;
use axum::response::Json;
use leptos::prelude::*;
use leptos_axum::LeptosRoutes;
use leptos_axum::generate_route_list;
use monochange_app::app::App;
use monochange_app::app::shell;
use monochange_app_api::AppState;
use serde::Serialize;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Application-level error type.
#[derive(Debug)]
pub enum MonochangeError {
	/// Database connection or migration failure.
	Database(String),
	/// Server initialization failure.
	Server(String),
}

impl std::fmt::Display for MonochangeError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Database(msg) => write!(f, "database error: {msg}"),
			Self::Server(msg) => write!(f, "server error: {msg}"),
		}
	}
}

impl std::error::Error for MonochangeError {}

#[tokio::main]
async fn main() -> Result<(), MonochangeError> {
	// Initialize tracing
	tracing_subscriber::fmt()
		.with_env_filter(
			EnvFilter::try_from_default_env()
				.unwrap_or_else(|_| EnvFilter::new("monochange_app=debug,info")),
		)
		.init();

	// Load typed application secrets through the SecretSpec SDK.
	// In production, SECRETSPEC_PROVIDER points at OnePassword and the
	// Docker entrypoint exposes OP_SERVICE_ACCOUNT_TOKEN from a Docker secret.
	let resolved_secrets = monochange_app_api::load_app_secrets()
		.map_err(|error| MonochangeError::Server(error.to_string()))?;
	let secrets = resolved_secrets.secrets;
	let database_url = secrets.database_url.clone().unwrap_or_else(|| {
		std::env::var("DATABASE_URL")
			.unwrap_or_else(|_| "sqlite://./monochange_app.sqlite3".to_string())
	});

	// Create database connection pool
	info!("Connecting to database...");
	let pool = monochange_app_db::create_pool(&database_url)
		.await
		.map_err(|e| MonochangeError::Database(e.to_string()))?;

	// Run database migrations (skip if already applied)
	info!("Running migrations...");
	monochange_app_db::run_migrations(&pool)
		.await
		.map_err(|e| MonochangeError::Database(e.to_string()))?;
	info!("Database ready");

	let automation_config = monochange_app_automation::AutomationRuntimeConfig::from_env()
		.map_err(|error| MonochangeError::Server(error.to_string()))?;
	let _automation_worker: Option<tokio::task::JoinHandle<()>> =
		monochange_app_automation::spawn_sqlite_automation_worker(pool.clone(), automation_config);

	// Create application state
	let app_state = Arc::new(AppState::new(pool, secrets));

	// Leptos configuration
	let conf = get_configuration(None).map_err(|e| MonochangeError::Server(e.to_string()))?;
	let leptos_options = conf.leptos_options.clone();
	let routes = generate_route_list(App);

	// Build the axum application
	#[derive(Serialize)]
	struct HealthResponse {
		status: &'static str,
		http: &'static str,
	}

	let app = Router::new()
		.route(
			"/health",
			axum::routing::get(|| {
				async move {
					Json(HealthResponse {
						status: "ok",
						http: "up",
					})
				}
			}),
		)
		.leptos_routes_with_context(
			&leptos_options,
			routes,
			{
				let leptos_options = leptos_options.clone();
				let state = app_state.clone();
				move || {
					provide_context(leptos_options.clone());
					provide_context(state.clone());
				}
			},
			{
				let leptos_options = leptos_options.clone();
				move || shell(leptos_options.clone())
			},
		)
		.fallback(leptos_axum::file_and_error_handler({
			let state = app_state.clone();
			move |leptos_options: leptos::prelude::LeptosOptions| {
				provide_context(leptos_options.clone());
				provide_context(state.clone());
				shell(leptos_options)
			}
		}));

	let addr = std::env::var("PORT")
		.ok()
		.and_then(|p| format!("0.0.0.0:{p}").parse().ok())
		.unwrap_or(leptos_options.site_addr);
	info!("monochange_app starting on http://{addr}");

	let listener = tokio::net::TcpListener::bind(&addr)
		.await
		.map_err(|e| MonochangeError::Server(e.to_string()))?;
	let app: axum::Router<()> = app.with_state(leptos_options.clone());
	axum::serve(listener, app)
		.await
		.map_err(|e| MonochangeError::Server(e.to_string()))?;

	Ok(())
}
