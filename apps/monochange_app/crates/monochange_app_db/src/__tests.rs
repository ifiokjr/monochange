//! Unit tests for database models, migrations, and error types.

#[cfg(test)]
mod tests {
    use crate::models::*;
    use rstest::rstest;

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
        let sql = writer.up_sql(welds::Syntax::Postgres);
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
        let sql = writer.down_sql(welds::Syntax::Postgres);
        let combined = sql.join("\n");
        assert!(combined.contains("DROP TABLE IF EXISTS users"));
        assert!(combined.contains("DROP TABLE IF EXISTS installations"));
        assert!(combined.contains("DROP TABLE IF EXISTS repositories"));
    }

    #[rstest]
    fn test_up_sql_includes_indexes() {
        use welds::migrations::MigrationWriter;
        let writer = crate::CreateUsersTable;
        let sql = writer.up_sql(welds::Syntax::Postgres);
        let combined = sql.join("\n");
        assert!(combined.contains("CREATE INDEX IF NOT EXISTS"));
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
