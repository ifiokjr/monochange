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
	Ok(welds_connections::postgres::PostgresClient::from(
		pool.clone(),
	))
}

/// Run database migrations.
pub async fn run_migrations(pool: &sqlx::PgPool) -> Result<(), DbError> {
	let client = get_client(pool).await?;

	let migrations: &[welds::migrations::MigrationFn] =
		&[create_users_tables, create_release_automation_tables];

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

fn create_release_automation_tables(
	_state: &welds::migrations::TableState,
) -> Result<welds::migrations::MigrationStep, welds::WeldsError> {
	Ok(welds::migrations::MigrationStep::new(
		"002_create_release_automation",
		CreateReleaseAutomationTables,
	))
}

struct CreateUsersTable;
struct CreateReleaseAutomationTables;

fn sql_statements(sql: &str) -> Vec<String> {
	sql.split(';')
		.map(str::trim)
		.filter(|statement| !statement.is_empty())
		.map(|statement| format!("{statement};"))
		.collect()
}

impl welds::migrations::MigrationWriter for CreateUsersTable {
	fn up_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
		sql_statements(
			r#"
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
        "#,
		)
	}

	fn down_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
		sql_statements(
			r#"
            DROP TABLE IF EXISTS repositories CASCADE;
            DROP TABLE IF EXISTS installations CASCADE;
            DROP TABLE IF EXISTS organization_members CASCADE;
            DROP TABLE IF EXISTS organizations CASCADE;
            DROP TABLE IF EXISTS users CASCADE;
        "#,
		)
	}
}
impl welds::migrations::MigrationWriter for CreateReleaseAutomationTables {
	fn up_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
		sql_statements(
			r#"
            CREATE TABLE IF NOT EXISTS release_schedules (
                id SERIAL PRIMARY KEY,
                repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                enabled BOOLEAN NOT NULL DEFAULT true,
                cadence_json TEXT NOT NULL,
                next_run_at TIMESTAMPTZ NOT NULL,
                window_batch_index INTEGER NOT NULL DEFAULT 0,
                last_enqueued_at TIMESTAMPTZ,
                base_ref VARCHAR(255) NOT NULL DEFAULT 'main',
                requested_by_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE TABLE IF NOT EXISTS release_jobs (
                id VARCHAR(36) PRIMARY KEY,
                schedule_id INTEGER NOT NULL REFERENCES release_schedules(id) ON DELETE CASCADE,
                repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                kind VARCHAR(64) NOT NULL,
                status VARCHAR(32) NOT NULL,
                run_after TIMESTAMPTZ NOT NULL,
                scheduled_for TIMESTAMPTZ NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 5,
                locked_by VARCHAR(255),
                locked_until TIMESTAMPTZ,
                idempotency_key VARCHAR(255) NOT NULL UNIQUE,
                payload_json TEXT NOT NULL,
                result_json TEXT,
                last_error TEXT,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );

            CREATE INDEX IF NOT EXISTS idx_release_schedules_due ON release_schedules(enabled, next_run_at);
            CREATE INDEX IF NOT EXISTS idx_release_jobs_due ON release_jobs(status, run_after);
            CREATE INDEX IF NOT EXISTS idx_release_jobs_lock ON release_jobs(locked_until);
            CREATE INDEX IF NOT EXISTS idx_release_jobs_repository ON release_jobs(repository_id);
        "#,
		)
	}

	fn down_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
		sql_statements(
			r#"
            DROP TABLE IF EXISTS release_jobs CASCADE;
            DROP TABLE IF EXISTS release_schedules CASCADE;
        "#,
		)
	}
}
