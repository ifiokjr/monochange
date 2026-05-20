//! SQLite database connection and migration runner for monochange_app.
//!
//! The app stores its state in a local SQLite database so development and
//! production use the same database engine.

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;

use sqlx::sqlite::SqliteConnectOptions;
use sqlx::sqlite::SqlitePoolOptions;
use thiserror::Error;

pub mod models;

pub type DbPool = sqlx::SqlitePool;
pub type DbClient = welds_connections::sqlite::SqliteClient;

/// Database error type.
#[derive(Debug, Error)]
pub enum DbError {
	#[error("Connection failed: {0}")]
	Connection(String),

	#[error("Migration failed: {0}")]
	Migration(String),
}

/// Create a SQLite connection pool.
pub async fn create_pool(database_url: &str) -> Result<DbPool, DbError> {
	let options = database_url
		.parse::<SqliteConnectOptions>()
		.map_err(|error| DbError::Connection(error.to_string()))?
		.create_if_missing(true)
		.foreign_keys(true)
		.journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
		.busy_timeout(std::time::Duration::from_secs(5));

	SqlitePoolOptions::new()
		.max_connections(5)
		.connect_with(options)
		.await
		.map_err(|error| DbError::Connection(error.to_string()))
}

/// Get a Welds client from the pool for ORM operations.
pub async fn get_client(pool: &DbPool) -> Result<DbClient, DbError> {
	Ok(welds_connections::sqlite::SqliteClient::from(pool.clone()))
}

/// Run database migrations.
pub async fn run_migrations(pool: &DbPool) -> Result<(), DbError> {
	let client = get_client(pool).await?;

	let migrations: &[welds::migrations::MigrationFn] =
		&[create_users_tables, create_release_automation_tables];

	welds::migrations::up(&client, migrations)
		.await
		.map_err(|error| DbError::Migration(error.to_string()))?;

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

pub(crate) struct CreateUsersTable;
pub(crate) struct CreateReleaseAutomationTables;

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
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                github_id INTEGER NOT NULL UNIQUE,
                github_login TEXT NOT NULL,
                github_avatar_url TEXT,
                github_access_token TEXT NOT NULL,
                email TEXT,
                plan_tier TEXT NOT NULL DEFAULT 'free',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS organizations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                github_id INTEGER NOT NULL UNIQUE,
                github_login TEXT NOT NULL,
                github_avatar_url TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS organization_members (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                org_id INTEGER NOT NULL REFERENCES organizations(id) ON DELETE CASCADE,
                role TEXT NOT NULL DEFAULT 'member',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                UNIQUE(user_id, org_id)
            );

            CREATE TABLE IF NOT EXISTS installations (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
                github_installation_id INTEGER NOT NULL UNIQUE,
                github_account_login TEXT NOT NULL,
                github_account_type TEXT NOT NULL,
                target_type TEXT NOT NULL DEFAULT 'selected',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS repositories (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                installation_id INTEGER NOT NULL REFERENCES installations(id) ON DELETE CASCADE,
                github_repo_id INTEGER NOT NULL UNIQUE,
                github_full_name TEXT NOT NULL,
                github_private INTEGER NOT NULL DEFAULT 0,
                monochange_config_hash TEXT,
                settings_json TEXT,
                plan_tier TEXT NOT NULL DEFAULT 'free',
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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
            DROP TABLE IF EXISTS repositories;
            DROP TABLE IF EXISTS installations;
            DROP TABLE IF EXISTS organization_members;
            DROP TABLE IF EXISTS organizations;
            DROP TABLE IF EXISTS users;
        "#,
		)
	}
}

impl welds::migrations::MigrationWriter for CreateReleaseAutomationTables {
	fn up_sql(&self, _syntax: welds::Syntax) -> Vec<String> {
		sql_statements(
			r#"
            CREATE TABLE IF NOT EXISTS release_schedules (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                enabled INTEGER NOT NULL DEFAULT 1,
                cadence_json TEXT NOT NULL,
                next_run_at TEXT NOT NULL,
                window_batch_index INTEGER NOT NULL DEFAULT 0,
                last_enqueued_at TEXT,
                base_ref TEXT NOT NULL DEFAULT 'main',
                requested_by_user_id INTEGER REFERENCES users(id) ON DELETE SET NULL,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS release_jobs (
                id TEXT PRIMARY KEY,
                schedule_id INTEGER NOT NULL REFERENCES release_schedules(id) ON DELETE CASCADE,
                repository_id INTEGER NOT NULL REFERENCES repositories(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                status TEXT NOT NULL,
                run_after TEXT NOT NULL,
                scheduled_for TEXT NOT NULL,
                attempts INTEGER NOT NULL DEFAULT 0,
                max_attempts INTEGER NOT NULL DEFAULT 5,
                locked_by TEXT,
                locked_until TEXT,
                idempotency_key TEXT NOT NULL UNIQUE,
                payload_json TEXT NOT NULL,
                result_json TEXT,
                last_error TEXT,
                created_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
                updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
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
            DROP TABLE IF EXISTS release_jobs;
            DROP TABLE IF EXISTS release_schedules;
        "#,
		)
	}
}
