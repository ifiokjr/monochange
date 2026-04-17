use insta::assert_json_snapshot;
use monochange_analysis::AnalysisConfig;
use monochange_analysis::ChangeFrame;
use monochange_analysis::analyze_changes;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git;
use monochange_test_helpers::snapshot_settings;
use tempfile::tempdir;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn analyze_changes_reports_cargo_public_api_and_manifest_diffs() {
	let before = fixture_path("analysis/cargo-public-api-diff/before");
	let after = fixture_path("analysis/cargo-public-api-diff/after");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	copy_directory(&before, tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);

	copy_directory(&after, tempdir.path());

	let analysis = analyze_changes(
		tempdir.path(),
		&ChangeFrame::WorkingDirectory,
		&AnalysisConfig::default(),
	)
	.unwrap_or_else(|error| panic!("analysis: {error}"));

	snapshot_settings().bind(|| {
		assert_json_snapshot!(analysis);
	});
}

#[test]
fn analyze_changes_reports_multi_ecosystem_semantic_diffs() {
	let before = fixture_path("analysis/multi-ecosystem-diff/before");
	let after = fixture_path("analysis/multi-ecosystem-diff/after");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	copy_directory(&before, tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);

	copy_directory(&after, tempdir.path());

	let analysis = analyze_changes(
		tempdir.path(),
		&ChangeFrame::WorkingDirectory,
		&AnalysisConfig::default(),
	)
	.unwrap_or_else(|error| panic!("analysis: {error}"));

	snapshot_settings().bind(|| {
		assert_json_snapshot!(analysis);
	});
}
