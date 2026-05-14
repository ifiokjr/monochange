use std::fs;

use super::*;

fn write_file(root: &std::path::Path, relative_path: &str, contents: &str) {
	let path = root.join(relative_path);
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create parent: {error}"));
	}
	fs::write(&path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

#[test]
fn audit_migration_detects_legacy_release_tooling() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	for path in [
		"monochange.toml",
		"knope.toml",
		".knope.toml",
		".changeset/config.json",
		"release-please-config.json",
		".release-please-manifest.json",
		".releaserc",
		".releaserc.json",
		".releaserc.yml",
		".releaserc.yaml",
		".releaserc.js",
		"release.config.js",
		"release.toml",
		"mdt.toml",
		"CHANGELOG.md",
		"changelog.md",
	] {
		write_file(root, path, "configured\n");
	}

	write_file(
		root,
		"package.json",
		r#"{
			"devDependencies": {
				"@changesets/cli": "latest",
				"release-please": "latest",
				"semantic-release": "latest",
				"knope": "latest"
			},
			"scripts": { "changeset": "changeset" }
		}"#,
	);
	write_file(root, ".github/workflows/ignore.txt", "semantic-release\n");
	write_file(
		root,
		".github/workflows/release.yml",
		"changesets/action\nrelease-please\nsemantic-release\nknope\ncargo release\n",
	);

	let report = audit_migration(root).unwrap_or_else(|error| panic!("audit migration: {error}"));

	assert_eq!(report.status, MigrationAuditStatus::MigrationNeeded);
	assert!(
		report
			.signals
			.iter()
			.any(|signal| signal.kind == "monochange-config")
	);
	assert!(
		report.signals.iter().any(|signal| {
			signal.kind == "legacy-changeset-tool" && signal.tool == "changesets"
		})
	);
	assert!(report.signals.iter().any(|signal| {
		signal.kind == "legacy-release-tool" && signal.tool == "semantic-release"
	}));
	assert!(
		report
			.signals
			.iter()
			.any(|signal| { signal.kind == "package-script" && signal.tool == "release-please" })
	);
	assert!(report.signals.iter().any(|signal| {
		signal.kind == "ci-workflow"
			&& signal.tool == "cargo-release"
			&& signal.path == ".github/workflows/release.yml"
	}));
	assert!(
		!report
			.signals
			.iter()
			.any(|signal| signal.path.ends_with("ignore.txt"))
	);

	let recommendation_ids = report
		.recommendations
		.iter()
		.map(|recommendation| recommendation.id.as_str())
		.collect::<Vec<_>>();
	assert!(!recommendation_ids.contains(&"generate-config"));
	assert!(recommendation_ids.contains(&"translate-release-tooling"));
	assert!(recommendation_ids.contains(&"audit-changelogs"));
	assert!(recommendation_ids.contains(&"replace-ci-workflows"));
	assert!(recommendation_ids.contains(&"trusted-publishing-checklist"));
	assert_eq!(report.next_steps.len(), 3);

	let text = render_migration_audit_report(&report, OutputFormat::Text);
	assert!(text.contains("migration audit: migration-needed"));
	assert!(text.contains("GitHub Actions workflow references cargo-release"));
	let json = render_migration_audit_report(&report, OutputFormat::Json);
	assert!(json.contains("\"status\": \"migration-needed\""));
}

#[test]
fn audit_migration_reports_ready_for_empty_workspace() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	let report =
		audit_migration(tempdir.path()).unwrap_or_else(|error| panic!("audit migration: {error}"));

	assert_eq!(report.status, MigrationAuditStatus::Ready);
	assert!(report.signals.is_empty());
	assert!(report.recommendations.iter().any(|recommendation| {
		recommendation.id == "generate-config"
			&& recommendation.title == "Generate monochange configuration"
	}));
	assert!(
		report
			.recommendations
			.iter()
			.any(|recommendation| { recommendation.id == "trusted-publishing-checklist" })
	);

	let text = render_migration_audit_report(&report, OutputFormat::Markdown);
	assert!(text.contains("migration audit: ready"));
	assert!(text.contains("- none detected"));
	assert!(text.contains("Generate monochange configuration"));
}

#[test]
fn audit_migration_ignores_non_migration_signals_for_status() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_file(root, "monochange.toml", "[workspace]\n");
	write_file(root, "CHANGELOG.md", "# Changelog\n");

	let report = audit_migration(root).unwrap_or_else(|error| panic!("audit migration: {error}"));

	assert_eq!(report.status, MigrationAuditStatus::Ready);
	assert_eq!(migration_signals(&report.signals).count(), 0);
	assert!(
		report
			.recommendations
			.iter()
			.any(|recommendation| { recommendation.id == "audit-changelogs" })
	);
	assert!(
		!report
			.recommendations
			.iter()
			.any(|recommendation| { recommendation.id == "trusted-publishing-checklist" })
	);
}
