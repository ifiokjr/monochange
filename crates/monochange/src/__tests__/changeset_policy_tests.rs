#![allow(clippy::disallowed_methods)]
use std::fs;
use std::process::Command;

use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::SourceProvider as ProviderKind;
use monochange_core::VersionFormat;

use super::*;

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn sample_package() -> PackageDefinition {
	PackageDefinition {
		id: "core".to_string(),
		path: Path::new("crates/core").to_path_buf(),
		package_type: PackageType::Cargo,
		changelog: None,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: vec!["README.md".to_string(), "docs/**".to_string()],
		additional_paths: vec!["shared/**".to_string()],
		tag: true,
		release: true,
		publish: monochange_core::PublishSettings::default(),
		version_format: VersionFormat::Primary,
	}
}

fn git(root: &Path, args: &[&str]) {
	let mut command = Command::new("git");
	command.current_dir(root);
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}
	let output = command
		.args(["-c", "commit.gpgsign=false"])
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_requires_changeset_verification_to_be_enabled() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(
		tempdir.path().join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/core\"]\n",
	)
	.unwrap_or_else(|error| panic!("write workspace Cargo.toml: {error}"));
	fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write package Cargo.toml: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write lib.rs: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n\n[changesets.affected]\nenabled = false\nrequired = true\ncomment_on_failure = true\n\n[package.core]\npath = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let error = affected_packages(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&Vec::new(),
	)
	.await
	.err()
	.unwrap_or_else(|| panic!("expected disabled verification error"));
	assert!(matches!(error, MonochangeError::Config(_)));
	assert_eq!(
		error.to_string(),
		"config error: changeset verification requires `[changesets.affected].enabled = true`"
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_skips_release_pull_request_branches() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		"\n[source]\nprovider = \"github\"\nowner = \"monochange\"\nrepo = \"monochange\"\n\n[source.pull_requests]\nbranch_prefix = \"monochange/release\"\n",
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	git(fixture.path(), &["init", "-b", "main"]);
	git(fixture.path(), &["config", "user.name", "monochange"]);
	git(
		fixture.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	git(fixture.path(), &["config", "commit.gpgsign", "false"]);
	git(fixture.path(), &["add", "."]);
	git(fixture.path(), &["commit", "-m", "initial"]);
	git(
		fixture.path(),
		&["checkout", "-b", "monochange/release/affected"],
	);

	let evaluation = affected_packages(
		fixture.path(),
		&["crates/core/src/lib.rs".to_string()],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Skipped);
	assert!(!evaluation.required);
	assert!(evaluation.affected_package_ids.is_empty());
	assert_eq!(
		evaluation.changed_paths,
		vec!["crates/core/src/lib.rs".to_string()]
	);
	assert!(
		evaluation
			.summary
			.contains("current branch `monochange/release/affected` starts")
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_uses_global_affected_path_filters() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let config_path = fixture.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config = config.replace(
		"comment_on_failure = true",
		"comment_on_failure = true\nchanged_paths = [\"infra/**\"]\nignored_paths = [\"docs/**\"]",
	);
	config = config.replace(
		"path = \"crates/core\"\nignored_paths",
		"path = \"crates/core\"\nchangelog = \"crates/core/CHANGELOG.md\"\nignored_paths",
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let evaluation = affected_packages(
		fixture.path(),
		&[
			"docs/readme.md".to_string(),
			"crates/core/CHANGELOG.md".to_string(),
			"infra/config.yml".to_string(),
		],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("affected packages for global paths: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert_eq!(
		evaluation.ignored_paths,
		vec![
			"docs/readme.md".to_string(),
			"crates/core/CHANGELOG.md".to_string()
		]
	);
	assert_eq!(
		evaluation.matched_paths,
		vec!["infra/config.yml".to_string()]
	);
	assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
}

#[tokio::test(flavor = "multi_thread")]
async fn current_branch_prefix_matching_requires_non_empty_prefix() {
	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	git(repo.path(), &["init", "-b", "main"]);
	git(repo.path(), &["config", "user.name", "monochange"]);
	git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	git(repo.path(), &["config", "commit.gpgsign", "false"]);
	fs::write(repo.path().join("tracked.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write tracked file: {error}"));
	git(repo.path(), &["add", "tracked.txt"]);
	git(repo.path(), &["commit", "-m", "initial"]);

	let mut source = SourceConfiguration {
		provider: ProviderKind::default(),
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	};

	source.pull_requests.branch_prefix = "   ".to_string();
	assert_eq!(
		current_branch_matches_pull_request_branch_prefix(repo.path(), Some(&source)).await,
		None
	);

	source.pull_requests.branch_prefix = "monochange/release/".to_string();
	assert_eq!(
		current_branch_matches_pull_request_branch_prefix(repo.path(), Some(&source)).await,
		None
	);

	git(repo.path(), &["checkout", "-b", "monochange/release/core"]);
	assert_eq!(
		current_branch_matches_pull_request_branch_prefix(repo.path(), Some(&source)).await,
		Some((
			"monochange/release/core".to_string(),
			"monochange/release/".to_string()
		))
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_defers_workspace_discovery_until_changesets_are_present() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(tempdir.path().join("crates/core/Cargo.toml"), "not toml\n")
		.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\n\
		package_type = \"cargo\"\n\
		\n\
		[changesets.affected]\n\
		enabled = true\n\
		required = true\n\
		\n\
		[package.core]\n\
		path = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let evaluation = affected_packages(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
	assert!(evaluation.covered_package_ids.is_empty());
	assert!(
		evaluation
			.errors
			.iter()
			.any(|error| error.contains("not covered by attached changesets"))
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_uses_configuration_index_for_attached_changesets() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(tempdir.path().join("crates/core/Cargo.toml"), "not toml\n")
		.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(
		tempdir.path().join(".changeset/core.md"),
		"---\ncore: patch\n---\n\n#### cover core\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\n\
		package_type = \"cargo\"\n\
		\n\
		[changesets.affected]\n\
		enabled = true\n\
		required = true\n\
		\n\
		[package.core]\n\
		path = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let evaluation = affected_packages(
		tempdir.path(),
		&[
			"crates/core/src/lib.rs".to_string(),
			".changeset/core.md".to_string(),
		],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Passed);
	assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
	assert_eq!(evaluation.covered_package_ids, vec!["core".to_string()]);
	assert!(evaluation.errors.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_reports_missing_and_invalid_changeset_inputs() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	fs::create_dir_all(fixture.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(fixture.path().join(".changeset/invalid.md"), "---\ncore:\n")
		.unwrap_or_else(|error| panic!("write invalid changeset: {error}"));

	let evaluation = affected_packages(
		fixture.path(),
		&[
			"crates/core/src/lib.rs".to_string(),
			".changeset/missing.md".to_string(),
			".changeset/invalid.md".to_string(),
		],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert!(
		evaluation
			.errors
			.iter()
			.any(|error| error.contains("does not exist in the checked-out workspace"))
	);
	assert!(
		evaluation
			.errors
			.iter()
			.any(|error| error.contains("failed to parse") || error.contains("invalid"))
	);
	assert!(evaluation.summary.contains("changeset verification failed"));
	assert_eq!(
		evaluation.summary,
		"changeset verification failed: attached changesets do not cover 1 changed package"
	);
	assert!(evaluation.comment.is_some());
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_marks_ignored_paths_and_omits_failure_comments_when_disabled() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let evaluation = affected_packages(
		fixture.path(),
		&["crates/core/tests/policy.rs".to_string()],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("affected packages for ignored path: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::NotRequired);
	assert_eq!(
		evaluation.ignored_paths,
		vec!["crates/core/tests/policy.rs".to_string()]
	);
	assert!(evaluation.affected_package_ids.is_empty());
	assert!(evaluation.comment.is_none());
}

#[test]
fn compute_changed_paths_since_reports_git_failures_and_includes_untracked_files() {
	let failing_repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	git(failing_repo.path(), &["init", "-b", "main"]);
	let error = compute_changed_paths_since(failing_repo.path(), "definitely-missing-rev")
		.err()
		.unwrap_or_else(|| panic!("expected git diff failure"));
	assert!(
		error
			.to_string()
			.contains("git diff --name-only definitely-missing-rev failed")
	);

	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	git(repo.path(), &["init", "-b", "main"]);
	git(repo.path(), &["config", "user.name", "monochange"]);
	git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	git(repo.path(), &["config", "commit.gpgsign", "false"]);
	fs::write(repo.path().join("tracked.txt"), "one\n")
		.unwrap_or_else(|error| panic!("write tracked file: {error}"));
	git(repo.path(), &["add", "tracked.txt"]);
	git(repo.path(), &["commit", "-m", "initial"]);
	fs::write(repo.path().join("tracked.txt"), "two\n")
		.unwrap_or_else(|error| panic!("update tracked file: {error}"));
	fs::write(repo.path().join("new.txt"), "new\n")
		.unwrap_or_else(|error| panic!("write untracked file: {error}"));

	let changed = compute_changed_paths_since(repo.path(), "HEAD")
		.unwrap_or_else(|error| panic!("compute changed paths: {error}"));
	assert_eq!(
		changed,
		vec!["new.txt".to_string(), "tracked.txt".to_string()]
	);

	let removed_root = {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		tempdir.path().to_path_buf()
	};
	let spawn_error = compute_changed_paths_since(&removed_root, "HEAD")
		.err()
		.unwrap_or_else(|| panic!("expected git spawn failure"));
	assert!(
		spawn_error
			.to_string()
			.contains("failed to run git diff --name-only HEAD")
	);
}

#[test]
fn path_helpers_cover_normalization_matching_and_comment_rendering() {
	let package = sample_package();
	assert_eq!(
		normalize_changed_path("./crates\\core//src/lib.rs"),
		"crates/core//src/lib.rs"
	);
	assert!(is_changeset_markdown_path(".changeset/test.md"));
	assert!(!is_changeset_markdown_path(".changeset/test.txt"));

	let matcher = PackagePathMatcher::new(&package);
	assert_eq!(
		matcher.classify("crates/core/src/lib.rs"),
		PackagePathMatch::Touched
	);
	assert_eq!(
		matcher.classify("shared/config.json"),
		PackagePathMatch::Touched
	);
	assert!(matcher.is_ignored("crates/core/README.md"));
	let docs_relative_path = package_relative_path(
		"crates/core/docs/guide.md",
		&matcher.package_root,
		&matcher.package_root_prefix,
	);
	assert!(matches_any_compiled_package_pattern(
		"crates/core/docs/guide.md",
		docs_relative_path,
		&matcher.ignored_patterns
	));

	let verify = ChangesetAffectedSettings {
		enabled: true,
		required: true,
		comment_on_failure: true,
		skip_labels: vec!["release-notes-not-needed".to_string()],
		changed_paths: Vec::new(),
		ignored_paths: Vec::new(),
	};
	let evaluation = ChangesetPolicyEvaluation {
		status: ChangesetPolicyStatus::Failed,
		required: true,
		enforce: true,
		summary: "missing changesets".to_string(),
		comment: None,
		labels: vec!["release-notes-not-needed".to_string()],
		matched_skip_labels: Vec::new(),
		changed_paths: vec!["crates/core/src/lib.rs".to_string()],
		matched_paths: vec!["crates/core/src/lib.rs".to_string()],
		ignored_paths: vec!["crates/core/README.md".to_string()],
		changeset_paths: vec![".changeset/core.md".to_string()],
		affected_package_ids: vec!["core".to_string()],
		covered_package_ids: Vec::new(),
		uncovered_package_ids: vec!["core".to_string()],
		errors: vec!["core is uncovered".to_string()],
	};
	let comment = render_changeset_verification_comment(&verify, &evaluation);
	assert!(comment.contains("Changed package paths:"));
	assert!(comment.contains("Affected packages:"));
	assert!(comment.contains("Attached changeset files:"));
	assert!(comment.contains("Errors:"));
	assert!(comment.contains("Allowed skip labels:"));
	assert!(comment.contains("How to fix:"));
}

#[tokio::test(flavor = "multi_thread")]
async fn evaluate_and_verify_changesets_delegate_to_affected_packages() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	let changed_paths = vec!["crates/core/src/lib.rs".to_string()];
	let labels = vec!["no-changeset-required".to_string()];

	let affected = affected_packages(fixture.path(), &changed_paths, &labels)
		.await
		.unwrap_or_else(|error| panic!("affected packages: {error}"));
	let verified = verify_changesets(fixture.path(), &changed_paths, &labels)
		.await
		.unwrap_or_else(|error| panic!("verify changesets: {error}"));
	let evaluated = evaluate_changeset_policy(fixture.path(), &changed_paths, &labels)
		.await
		.unwrap_or_else(|error| panic!("evaluate changeset policy: {error}"));

	assert_eq!(verified, affected);
	assert_eq!(evaluated, affected);
}

#[test]
fn render_comment_includes_related_skip_guidance() {
	let verify = ChangesetAffectedSettings {
		enabled: true,
		required: false,
		comment_on_failure: true,
		skip_labels: vec!["docs-only".to_string()],
		changed_paths: Vec::new(),
		ignored_paths: Vec::new(),
	};
	let evaluation = ChangesetPolicyEvaluation {
		status: ChangesetPolicyStatus::Failed,
		required: false,
		enforce: false,
		summary: "no coverage".to_string(),
		comment: None,
		labels: Vec::new(),
		matched_skip_labels: Vec::new(),
		changed_paths: Vec::new(),
		matched_paths: Vec::new(),
		ignored_paths: Vec::new(),
		changeset_paths: Vec::new(),
		affected_package_ids: Vec::new(),
		covered_package_ids: Vec::new(),
		uncovered_package_ids: Vec::new(),
		errors: vec!["problem".to_string()],
	};
	let comment = render_changeset_verification_comment(&verify, &evaluation);
	assert!(comment.contains("docs-only"));
	assert!(comment.contains("apply one of the configured skip labels"));
}

#[test]
fn package_pattern_helpers_cover_root_relative_and_invalid_patterns() {
	let package = sample_package();
	let matcher = PackagePathMatcher::new(&package);

	assert_eq!(
		package_relative_path(
			"crates/core",
			&matcher.package_root,
			&matcher.package_root_prefix,
		),
		Some("")
	);
	assert!(
		package_relative_path(
			"docs/readme.md",
			&matcher.package_root,
			&matcher.package_root_prefix,
		)
		.is_none()
	);
	assert!(matcher.is_ignored("crates/core/docs/guide.md"));
	assert!(matches_any_compiled_package_pattern(
		"crates/core/README.md",
		package_relative_path(
			"crates/core/README.md",
			&matcher.package_root,
			&matcher.package_root_prefix,
		),
		&compile_patterns(&package.ignored_paths)
	));
	assert!(!matches_any_compiled_package_pattern(
		"crates/core/src/lib.rs",
		package_relative_path(
			"crates/core/src/lib.rs",
			&matcher.package_root,
			&matcher.package_root_prefix,
		),
		&compile_patterns(&["[".to_string()])
	));
	assert!(path_matches_compiled_patterns(
		"docs/readme.md",
		&compile_patterns(&["docs/**".to_string()])
	));
	assert!(!path_matches_compiled_patterns(
		"docs/readme.md",
		&compile_patterns(&["[".to_string()])
	));
	assert_eq!(
		matcher.classify("docs/readme.md"),
		PackagePathMatch::Unmatched
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn affected_packages_uses_invalid_changeset_summary_when_only_changeset_inputs_fail() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	fs::create_dir_all(fixture.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
	fs::write(fixture.path().join(".changeset/invalid.md"), "---\ncore:\n")
		.unwrap_or_else(|error| panic!("write invalid changeset: {error}"));

	let evaluation = affected_packages(
		fixture.path(),
		&[".changeset/invalid.md".to_string()],
		&Vec::new(),
	)
	.await
	.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

	assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
	assert_eq!(
		evaluation.summary,
		"changeset verification failed: one or more attached changeset files are invalid"
	);
	assert!(
		evaluation
			.errors
			.iter()
			.any(|error| error.contains("failed to parse"))
	);
}
