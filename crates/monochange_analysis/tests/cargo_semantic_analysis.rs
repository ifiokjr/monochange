use insta::assert_json_snapshot;
use monochange_analysis::AnalysisConfig;
use monochange_analysis::ChangeFrame;
use monochange_analysis::ReleaseTrajectoryRefs;
use monochange_analysis::analyze_changes;
use monochange_analysis::analyze_release_trajectory_for_refs;
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

#[test]
fn analyze_release_trajectory_reports_release_main_and_head_frames() {
	let release = fixture_path("analysis/release-trajectory/release");
	let main = fixture_path("analysis/release-trajectory/main");
	let head = fixture_path("analysis/release-trajectory/head");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

	copy_directory(&release, tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "release"]);
	git(tempdir.path(), &["branch", "-M", "main"]);
	git(tempdir.path(), &["tag", "v1.0.0"]);

	copy_directory(&main, tempdir.path());
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "main evolution"]);
	git(tempdir.path(), &["checkout", "-b", "feature"]);

	copy_directory(&head, tempdir.path());
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "feature changes"]);

	let analysis = analyze_release_trajectory_for_refs(
		tempdir.path(),
		&ReleaseTrajectoryRefs {
			release_ref: "v1.0.0".to_string(),
			main_ref: "main".to_string(),
			head_ref: "feature".to_string(),
		},
		&AnalysisConfig::default(),
	)
	.unwrap_or_else(|error| panic!("trajectory analysis: {error}"));

	snapshot_settings().bind(|| {
		assert_json_snapshot!(analysis);
	});
}
