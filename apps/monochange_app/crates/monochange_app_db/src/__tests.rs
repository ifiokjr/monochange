//! Unit tests for database models, migrations, and error types.

#[cfg(test)]
mod tests {
	use rstest::rstest;
	use welds::state::DbState;

	use crate::models::*;

	// ── Model construction tests ──

	#[rstest]
	fn test_user_new_has_defaults() {
		let user = User::new();
		assert_eq!(user.id, 0);
		assert_eq!(user.github_id, 0);
		assert_eq!(user.github_login, String::new());
	}

	#[rstest]
	fn test_installation_new() {
		let inst = Installation::new();
		assert_eq!(inst.github_installation_id, 0);
		assert_eq!(inst.target_type, String::new());
	}

	#[rstest]
	fn test_repository_new() {
		let repo = Repository::new();
		assert!(!repo.github_private);
		assert_eq!(repo.plan_tier, String::new());
	}

	#[rstest]
	fn test_organization_new() {
		let org = Organization::new();
		assert_eq!(org.github_id, 0);
		assert_eq!(org.github_login, String::new());
	}

	#[rstest]
	fn test_org_member_new() {
		let member = OrganizationMember::new();
		assert_eq!(member.role, String::new());
		assert_eq!(member.user_id, 0);
		assert_eq!(member.org_id, 0);
	}

	// ── Migration tests ──

	#[rstest]
	fn test_migration_writer_up_sql() {
		use welds::migrations::MigrationWriter;
		let writer = crate::CreateUsersTable;
		let sql = writer.up_sql(welds::Syntax::Sqlite);
		assert!(!sql.is_empty());
		let combined = sql.join("\n");
		assert!(combined.contains("CREATE TABLE IF NOT EXISTS users"));
		assert!(combined.contains("CREATE TABLE IF NOT EXISTS installations"));
		assert!(combined.contains("CREATE TABLE IF NOT EXISTS repositories"));
		assert!(combined.contains("organization_members"));
		assert!(combined.contains("organizations"));
	}

	#[rstest]
	fn test_migration_writer_down_sql() {
		use welds::migrations::MigrationWriter;
		let writer = crate::CreateUsersTable;
		let sql = writer.down_sql(welds::Syntax::Sqlite);
		let combined = sql.join("\n");
		assert!(combined.contains("DROP TABLE IF EXISTS users"));
		assert!(combined.contains("DROP TABLE IF EXISTS installations"));
		assert!(combined.contains("DROP TABLE IF EXISTS repositories"));
	}

	#[rstest]
	fn test_release_automation_migration_writer_up_sql() {
		use welds::migrations::MigrationWriter;
		let writer = crate::CreateReleaseAutomationTables;
		let sql = writer.up_sql(welds::Syntax::Sqlite);
		let combined = sql.join("\n");
		assert!(combined.contains("CREATE TABLE IF NOT EXISTS release_schedules"));
		assert!(combined.contains("CREATE TABLE IF NOT EXISTS release_jobs"));
		assert!(combined.contains("idempotency_key TEXT NOT NULL UNIQUE"));
	}

	#[rstest]
	fn test_release_automation_migration_writer_down_sql() {
		use welds::migrations::MigrationWriter;
		let writer = crate::CreateReleaseAutomationTables;
		let sql = writer.down_sql(welds::Syntax::Sqlite);
		let combined = sql.join("\n");
		assert!(combined.contains("DROP TABLE IF EXISTS release_jobs"));
		assert!(combined.contains("DROP TABLE IF EXISTS release_schedules"));
	}

	#[rstest]
	fn test_migration_writers_emit_one_statement_per_entry() {
		use welds::migrations::MigrationWriter;

		let statements = [
			crate::CreateUsersTable.up_sql(welds::Syntax::Sqlite),
			crate::CreateUsersTable.down_sql(welds::Syntax::Sqlite),
			crate::CreateReleaseAutomationTables.up_sql(welds::Syntax::Sqlite),
			crate::CreateReleaseAutomationTables.down_sql(welds::Syntax::Sqlite),
		]
		.concat();

		assert!(!statements.is_empty());
		for statement in statements {
			assert!(
				statement.trim_end().ends_with(';'),
				"statement should keep its terminator: {statement}",
			);
			assert_eq!(
				statement.matches(';').count(),
				1,
				"statement should be safe for prepared execution: {statement}",
			);
		}
	}

	#[rstest]
	fn test_up_sql_includes_indexes() {
		use welds::migrations::MigrationWriter;
		let user_writer = crate::CreateUsersTable;
		let automation_writer = crate::CreateReleaseAutomationTables;
		let combined = [
			user_writer.up_sql(welds::Syntax::Sqlite),
			automation_writer.up_sql(welds::Syntax::Sqlite),
		]
		.concat()
		.join("\n");
		assert!(combined.contains("CREATE INDEX IF NOT EXISTS"));
		assert!(combined.contains("idx_release_jobs_due"));
	}

	// ── SQLite integration tests ──

	#[tokio::test]
	async fn sqlite_migrations_create_expected_timestamp_columns() {
		let pool = crate::create_pool("sqlite::memory:")
			.await
			.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
		crate::run_migrations(&pool)
			.await
			.unwrap_or_else(|error| panic!("run sqlite migrations: {error}"));

		for table in [
			"users",
			"organizations",
			"organization_members",
			"installations",
			"repositories",
			"release_schedules",
			"release_jobs",
		] {
			let columns: Vec<String> =
				sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
					.fetch_all(&pool)
					.await
					.unwrap_or_else(|error| panic!("load columns for {table}: {error}"));
			assert!(
				columns.contains(&"created_at".to_string()),
				"{table} should have created_at"
			);
			assert!(
				columns.contains(&"updated_at".to_string()),
				"{table} should have updated_at"
			);
		}
	}

	#[tokio::test]
	async fn welds_timestamp_hooks_set_created_and_updated_at() {
		let pool = crate::create_pool("sqlite::memory:")
			.await
			.unwrap_or_else(|error| panic!("create sqlite pool: {error}"));
		crate::run_migrations(&pool)
			.await
			.unwrap_or_else(|error| panic!("run sqlite migrations: {error}"));
		let client = crate::get_client(&pool)
			.await
			.unwrap_or_else(|error| panic!("create welds client: {error}"));

		let stale =
			chrono::DateTime::from_timestamp(0, 0).unwrap_or_else(|| panic!("valid timestamp"));
		let mut user = DbState::new_uncreated(
			User::builder()
				.github_id(42)
				.github_login("octocat")
				.github_access_token("token")
				.created_at(stale)
				.updated_at(stale)
				.build(),
		);
		user.save(&client)
			.await
			.unwrap_or_else(|error| panic!("save user: {error}"));
		assert!(user.created_at > stale);
		assert_eq!(user.created_at, user.updated_at);

		let created_at = user.created_at;
		user.github_login = "octocat-renamed".to_string();
		user.save(&client)
			.await
			.unwrap_or_else(|error| panic!("update user: {error}"));
		assert_eq!(user.created_at, created_at);
		assert!(user.updated_at >= created_at);
	}

	// ── Error type tests ──

	#[rstest]
	fn test_db_error_connection_display() {
		let err = crate::DbError::Connection("timeout".into());
		assert_eq!(err.to_string(), "Connection failed: timeout");
	}

	#[rstest]
	fn test_db_error_migration_display() {
		let err = crate::DbError::Migration("invalid sql".into());
		assert_eq!(err.to_string(), "Migration failed: invalid sql");
	}

	#[rstest]
	fn test_db_error_debug() {
		let err = crate::DbError::Connection("test".into());
		let debug = format!("{err:?}");
		assert!(debug.contains("Connection"));
	}
}
