//! Database connection pool and migration runner for monochange_app.
//!
//! Uses sqlx for PostgreSQL connection pooling under Welds ORM.

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;

use sqlx::postgres::PgPoolOptions;
use thiserror::Error;

pub mod models;

/// Database error type.
#[derive(Debug, Error)]
pub enum DbError {
    #[error("Connection failed: {0}")]
    Connection(String),

    #[error("Migration failed: {0}")]
    Migration(String),
}

/// Create a PostgreSQL connection pool.
pub async fn create_pool(database_url: &str) -> Result<sqlx::PgPool, DbError> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect(database_url)
        .await
        .map_err(|e| DbError::Connection(e.to_string()))
}

/// Get a Welds client from the pool for ORM operations.
pub async fn get_client(
    pool: &sqlx::PgPool,
) -> Result<welds_connections::postgres::PostgresClient, DbError> {
    Ok(welds_connections::postgres::PostgresClient::from(pool.clone()))
}

/// Run database migrations.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), DbError> {
    let client = get_client(pool).await?;

    let migrations: &[welds::migrations::MigrationFn] = &[
        create_users_tables,
    ];

    welds::migrations::up(&client, migrations)
        .await
        .map_err(|e| DbError::Migration(e.to_string()))?;

    Ok(())
}

// ── Migration functions ──

fn create_users_tables(
    _state: &welds::migrations::TableState,
) -> Result<welds::migrations::MigrationStep, welds::WeldsError> {
    Ok(welds::migrations::MigrationStep::new(
        "001_create_users",
        CreateUsersTable,
    ))
}

struct CreateUsersTable;

impl welds::migrations::MigrationWriter for CreateUsersTable {
    fn up_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
        vec![r#"
            CREATE TABLE IF NOT EXISTS users (
                id SERIAL PRIMARY KEY,
                github_id BIGINT NOT NULL UNIQUE,
                github_login VARCHAR(255) NOT NULL,
                github_avatar_url TEXT,
                github_access_token TEXT NOT NULL,
                email VARCHAR(255),
                plan_tier VARCHAR(50) NOT NULL DEFAULT 'free',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS organizations (
                id SERIAL PRIMARY KEY,
                github_id BIGINT NOT NULL UNIQUE,
                github_login VARCHAR(255) NOT NULL,
                github_avatar_url TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS organization_members (
                id SERIAL PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                org_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                role VARCHAR(50) NOT NULL DEFAULT 'member',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(user_id, org_id)
            );

            CREATE TABLE IF NOT EXISTS installations (
                id SERIAL PRIMARY KEY,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                github_installation_id BIGINT NOT NULL UNIQUE,
                github_account_login VARCHAR(255) NOT NULL,
                github_account_type VARCHAR(50) NOT NULL,
                target_type VARCHAR(50) NOT NULL DEFAULT 'selected',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS repositories (
                id SERIAL PRIMARY KEY,
                installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
                github_repo_id BIGINT NOT NULL UNIQUE,
                github_full_name VARCHAR(512) NOT NULL,
                github_private BOOLEAN NOT NULL DEFAULT false,
                monochange_config_hash TEXT,
                settings_json JSONB,
                plan_tier VARCHAR(50) NOT NULL DEFAULT 'free',
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_repos_installation ON repositories(installation_id);
            CREATE INDEX IF NOT EXISTS idx_installations_user ON installations(user_id);
            CREATE INDEX IF NOT EXISTS idx_org_members_user ON organization_members(user_id);
        "#.to_string()]
    }

    fn down_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
        vec![r#"
            DROP TABLE IF EXISTS repositories CASCADE;
            DROP TABLE IF EXISTS installations CASCADE;
            DROP TABLE IF EXISTS organization_members CASCADE;
            DROP TABLE IF EXISTS organizations CASCADE;
            DROP TABLE IF EXISTS users CASCADE;
        "#.to_string()]
    }
}
