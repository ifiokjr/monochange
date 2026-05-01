//! monochange_app server binary.
//!
//! Starts an axum HTTP server with Leptos SSR integration,
//! PostgreSQL connection pool, and JWT session management.
//!
//! ## Secret management
//!
//! Secrets are declared in `secretspec.toml` and loaded via the
//! `secretspec run` CLI wrapper, which sets them as environment
//! variables before starting the server.
//!
//! ```bash
//! # Development (uses keyring provider with local defaults)
//! secretspec run --profile development -- cargo leptos watch
//!
//! # Production (uses 1Password or other provider)
//! secretspec run --profile production -- cargo leptos serve
//!
//! # CI (uses environment variables)
//! secretspec run --profile ci --provider env -- cargo leptos build
//! ```

use axum::Router;
use leptos::prelude::*;
use leptos_axum::{LeptosRoutes, generate_route_list};
use monochange_app::app::App;
use monochange_app_api::AppState;
use std::sync::Arc;
use tower_http::services::ServeDir;
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

    // Secrets are loaded by `secretspec run` and exposed as env vars.
    // In development, `secretspec.toml` has local defaults for DATABASE_URL and JWT_SECRET.
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgresql://postgres:postgres@localhost:5432/monochange".to_string());
    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev-secret-change-in-production".to_string());
    let github_client_id = std::env::var("GITHUB_CLIENT_ID").unwrap_or_default();
    let github_client_secret = std::env::var("GITHUB_CLIENT_SECRET").unwrap_or_default();

    // Create database connection pool
    info!("Connecting to database...");
    let pool = monochange_app_db::create_pool(&database_url)
        .await
        .map_err(|e| MonochangeError::Database(e.to_string()))?;

    // Run database migrations (skip if already applied)
    info!("Running migrations...");
    let _ = monochange_app_db::run_migrations(&pool).await;
    info!("Database ready");

    // Create application state
    let app_state = Arc::new(AppState::new(
        pool,
        jwt_secret,
        github_client_id,
        github_client_secret,
    ));

    // Leptos configuration
    let conf = get_configuration(None)
        .map_err(|e| MonochangeError::Server(e.to_string()))?;
    let leptos_options = conf.leptos_options.clone();
    let routes = generate_route_list(App);

    // Build the axum application
    let app = Router::new()
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
            App,
        )
        .fallback(leptos_axum::file_and_error_handler(
            {
                let state = app_state.clone();
                move |leptos_options: leptos::prelude::LeptosOptions| {
                    provide_context(leptos_options);
                    provide_context(state.clone());
                }
            },
        ))
        .nest_service("/pkg", ServeDir::new("pkg"))
        .nest_service("/style", ServeDir::new("style"))
        .nest_service("/branding", ServeDir::new("public/branding"));

    let addr = leptos_options.site_addr;
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
