use std::fs;

use monochange_test_helpers::copy_directory;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output_trimmed;
use tempfile::tempdir;

use super::*;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_analysis_repo(relative: &str) -> tempfile::TempDir {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path(relative), tempdir.path());
	git(tempdir.path(), &["init"]);
	git(tempdir.path(), &["config", "user.name", "monochange-tests"]);
	git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(tempdir.path(), &["add", "."]);
	git(tempdir.path(), &["commit", "-m", "base"]);
	git(tempdir.path(), &["branch", "-M", "main"]);
	tempdir
}

#[test]
fn preferred_package_id_uses_config_id_when_available() {
	let mut package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/repo/crates/core/Cargo.toml"),
		PathBuf::from("/repo"),
		None,
		monochange_core::PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), "core".to_string());

	assert_eq!(preferred_package_id(&package), "core");
}

#[test]
fn classify_file_change_uses_presence_of_before_and_after_contents() {
	assert_eq!(
		classify_file_change(None, Some(&"after".to_string())),
		FileChangeKind::Added
	);
	assert_eq!(
		classify_file_change(Some(&"before".to_string()), None),
		FileChangeKind::Deleted
	);
	assert_eq!(
		classify_file_change(Some(&"before".to_string()), Some(&"after".to_string())),
		FileChangeKind::Modified
	);
}

#[test]
fn normalize_package_ids_skips_manifests_outside_the_repo_root() {
	let root = PathBuf::from("/repo");
	let mut packages = vec![PackageRecord {
		id: "core".to_string(),
		name: "core".to_string(),
		ecosystem: Ecosystem::Cargo,
		manifest_path: PathBuf::from("/outside/Cargo.toml"),
		workspace_root: root.clone(),
		current_version: None,
		publish_state: monochange_core::PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::new(),
		declared_dependencies: Vec::new(),
	}];

	normalize_package_ids(&root, &mut packages);

	assert_eq!(
		packages
			.first()
			.unwrap_or_else(|| panic!("expected one normalized package"))
			.id,
		"core"
	);
}

#[test]
fn packages_for_path_prefers_the_longest_matching_package_root() {
	let root = PathBuf::from("/repo");
	let packages = vec![
		PackageRecord::new(
			Ecosystem::Npm,
			"workspace",
			root.join("packages/package.json"),
			root.clone(),
			None,
			monochange_core::PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			None,
			monochange_core::PublishState::Public,
		),
	];

	let matched = packages_for_path(&root, &packages, Path::new("packages/web/src/index.ts"));

	assert_eq!(matched.len(), 1);
	assert_eq!(
		matched
			.first()
			.unwrap_or_else(|| panic!("expected one matched package"))
			.name,
		"web"
	);
	assert!(packages_for_path(&root, &packages, Path::new("README.md")).is_empty());
}

#[test]
fn discover_analysis_workspace_collects_multi_ecosystem_packages() {
	let tempdir = monochange_test_helpers::fs::setup_fixture_from(
		env!("CARGO_MANIFEST_DIR"),
		"analysis/multi-ecosystem-diff/before",
	);

	let workspace = discover_analysis_workspace(tempdir.path())
		.unwrap_or_else(|error| panic!("discover analysis workspace: {error}"));

	assert_eq!(workspace.packages.len(), 4);
	assert!(
		workspace
			.packages
			.iter()
			.any(|package| package.name == "core")
	);
	assert!(
		workspace
			.packages
			.iter()
			.any(|package| package.name == "@acme/web")
	);
	assert!(
		workspace
			.packages
			.iter()
			.any(|package| package.name == "runtime")
	);
	assert!(
		workspace
			.packages
			.iter()
			.any(|package| package.name == "mobile_app")
	);
}

#[test]
fn analyze_changes_reports_unmatched_paths_as_warnings() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let readme = tempdir.path().join("README.md");
	fs::write(&readme, "base\n").unwrap_or_else(|error| panic!("write README: {error}"));
	git(tempdir.path(), &["add", "README.md"]);
	git(tempdir.path(), &["commit", "-m", "add readme"]);
	fs::write(&readme, "updated\n").unwrap_or_else(|error| panic!("update README: {error}"));

	let analysis = analyze_changes(
		tempdir.path(),
		&ChangeFrame::WorkingDirectory,
		&AnalysisConfig::default(),
	)
	.unwrap_or_else(|error| panic!("analyze changes: {error}"));

	assert!(
		analysis
			.warnings
			.iter()
			.any(|warning| warning.contains("did not match any configured package"))
	);
}

#[test]
fn snapshot_helpers_cover_error_paths_and_filtered_content() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let root = tempdir.path().to_path_buf();
	let head = git_output_trimmed(&root, &["rev-parse", "HEAD"]);
	let large_file = root.join("crates/core/src/large.rs");
	fs::write(&large_file, "a".repeat(300_000))
		.unwrap_or_else(|error| panic!("write large file: {error}"));
	git(&root, &["add", "."]);
	git(&root, &["commit", "-m", "add large file"]);

	assert!(read_working_tree_text(&large_file).is_none());
	assert!(
		read_text_file_from_git_object(&root, "HEAD:missing.rs")
			.unwrap_or_else(|error| panic!("read missing git object: {error}"))
			.is_none()
	);
	assert!(
		read_text_file_from_git_object(&root, "HEAD:crates/core/src/large.rs")
			.unwrap_or_else(|error| panic!("read large git object: {error}"))
			.is_none()
	);
	assert!(should_skip_directory(Path::new("target")));
	assert!(!should_skip_directory(Path::new("src")));
	assert_eq!(snapshot_label(&SnapshotTarget::WorkingTree), "working_tree");
	assert_eq!(snapshot_label(&SnapshotTarget::GitIndex), "index");
	assert_eq!(
		snapshot_label(&SnapshotTarget::GitRevision(head.clone())),
		head
	);

	let outside_package = PackageRecord::new(
		Ecosystem::Cargo,
		"outside",
		PathBuf::from("/outside/Cargo.toml"),
		PathBuf::from("/outside"),
		None,
		monochange_core::PublishState::Public,
	);
	assert!(snapshot_package(&root, &outside_package, &SnapshotTarget::WorkingTree).is_err());

	let not_a_repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let git_list_error = git_list_files(not_a_repo.path(), &["ls-files"])
		.unwrap_err()
		.render();
	assert!(git_list_error.contains("git"));
	let missing_repo = root.join("missing-repo");
	assert!(read_text_file_from_git_object(&missing_repo, "HEAD:file.rs").is_err());
}

#[test]
fn snapshot_target_helpers_cover_branch_range_pr_and_index_paths() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let root = tempdir.path().to_path_buf();
	git(&root, &["branch", "feature"]);

	let staged_targets = resolve_snapshot_targets(&root, &ChangeFrame::StagedOnly)
		.unwrap_or_else(|error| panic!("resolve staged targets: {error}"));
	assert!(matches!(staged_targets.after, SnapshotTarget::GitIndex));

	let range_targets = resolve_snapshot_targets(
		&root,
		&ChangeFrame::BranchRange {
			base: "main".to_string(),
			head: "feature".to_string(),
		},
	)
	.unwrap_or_else(|error| panic!("resolve branch targets: {error}"));
	assert!(matches!(
		range_targets.before,
		SnapshotTarget::GitRevision(_)
	));
	assert!(matches!(
		range_targets.after,
		SnapshotTarget::GitRevision(_)
	));

	let pr_targets = resolve_snapshot_targets(
		&root,
		&ChangeFrame::PullRequest {
			target: "main".to_string(),
			pr_branch: "feature".to_string(),
		},
	)
	.unwrap_or_else(|error| panic!("resolve pr targets: {error}"));
	assert!(matches!(pr_targets.before, SnapshotTarget::GitRevision(_)));

	let package_root = Path::new("crates/core");
	fs::create_dir_all(root.join("crates/core/target/generated"))
		.unwrap_or_else(|error| panic!("create skipped directory: {error}"));
	fs::write(
		root.join("crates/core/target/generated/ignored.rs"),
		"pub struct Ignored;\n",
	)
	.unwrap_or_else(|error| panic!("write skipped file: {error}"));
	let working_files = snapshot_files_from_working_tree(&root, package_root)
		.unwrap_or_else(|error| panic!("working tree snapshot: {error}"));
	assert!(!working_files.is_empty());
	assert!(
		snapshot_files_from_working_tree(&root, Path::new("missing"))
			.unwrap()
			.is_empty()
	);

	fs::write(root.join("crates/core/src/lib.rs"), "pub struct Changed;\n")
		.unwrap_or_else(|error| panic!("rewrite lib.rs: {error}"));
	git(&root, &["add", "crates/core/src/lib.rs"]);

	let index_files = snapshot_files_from_index(&root, package_root)
		.unwrap_or_else(|error| panic!("index snapshot: {error}"));
	assert!(
		index_files
			.iter()
			.any(|file| file.path == Path::new("src/lib.rs"))
	);
	assert!(
		read_text_file_from_target(
			&root,
			&SnapshotTarget::GitIndex,
			Path::new("crates/core/src/lib.rs")
		)
		.unwrap_or_else(|error| panic!("read index target: {error}"))
		.is_some()
	);
	assert!(
		read_text_file_from_target(
			&root,
			&SnapshotTarget::GitRevision(git_output_trimmed(&root, &["rev-parse", "HEAD"])),
			Path::new("crates/core/src/lib.rs"),
		)
		.unwrap_or_else(|error| panic!("read revision target: {error}"))
		.is_some()
	);

	let built = build_snapshot_files_from_paths(
		&root,
		package_root,
		&SnapshotTarget::GitIndex,
		&[
			PathBuf::from("outside.txt"),
			PathBuf::from("crates/core/src/lib.rs"),
			PathBuf::from("crates/core/src/missing.rs"),
		],
	)
	.unwrap_or_else(|error| panic!("build snapshot files: {error}"));
	assert_eq!(built.len(), 1);
	assert_eq!(
		built
			.first()
			.unwrap_or_else(|| panic!("expected one built snapshot file"))
			.path,
		PathBuf::from("src/lib.rs")
	);
}

#[test]
fn analyze_release_trajectory_for_refs_uses_explicit_ranges_and_warns_when_head_matches_main() {
	let release = fixture_path("analysis/release-trajectory/release");
	let main = fixture_path("analysis/release-trajectory/main");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&release, root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);
	git(root, &["tag", "v1.0.0"]);

	copy_directory(&main, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main evolution"]);

	let analysis = analyze_release_trajectory_for_refs(
		root,
		&ReleaseTrajectoryRefs {
			release_ref: "v1.0.0".to_string(),
			main_ref: "main".to_string(),
			head_ref: "main".to_string(),
		},
		&AnalysisConfig::default(),
	)
	.unwrap_or_else(|error| panic!("release trajectory refs: {error}"));

	assert_eq!(
		analysis.frames.release_to_main.frame.revision_range(),
		"v1.0.0...main"
	);
	assert_eq!(
		analysis.frames.main_to_head.frame.revision_range(),
		"main...main"
	);
	assert_eq!(
		analysis.frames.release_to_head.frame.revision_range(),
		"v1.0.0...main"
	);
	assert_eq!(analysis.warnings.len(), 1);
	assert!(
		analysis
			.warnings
			.first()
			.unwrap_or_else(|| panic!("expected release trajectory warning"))
			.contains("head matches main")
	);
	assert!(
		analysis
			.frames
			.release_to_main
			.package_analyses
			.contains_key("core")
	);
}

#[test]
fn latest_workspace_release_tag_reports_missing_tags_and_git_failures() {
	let missing_repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing_repo_error = latest_workspace_release_tag(missing_repo.path())
		.unwrap_err()
		.render();
	assert!(missing_repo_error.contains("failed to list git tags"));

	let repo_without_tags = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = repo_without_tags.path();
	copy_directory(&fixture_path("analysis/release-trajectory/release"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);

	let no_tag_error = latest_workspace_release_tag(root).unwrap_err().render();
	assert!(no_tag_error.contains("failed to resolve a workspace release baseline"));
}

#[test]
fn latest_workspace_release_tag_ignores_higher_namespaced_tags() {
	let repo = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = repo.path();
	copy_directory(&fixture_path("analysis/release-trajectory/release"), root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);
	git(root, &["tag", "v9.9.9/namespace"]);
	git(root, &["tag", "v1.2.3"]);

	assert_eq!(
		latest_workspace_release_tag(root)
			.unwrap_or_else(|error| panic!("latest workspace tag: {error}")),
		"v1.2.3"
	);
}

#[test]
fn auto_release_trajectory_resolution_uses_latest_workspace_tag_and_branch_refs() {
	let release = fixture_path("analysis/release-trajectory/release");
	let main = fixture_path("analysis/release-trajectory/main");
	let head = fixture_path("analysis/release-trajectory/head");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&release, root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);
	git(root, &["tag", "pkg-a/v9.9.9"]);
	git(root, &["tag", "v1.0.0"]);

	copy_directory(&main, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main evolution"]);
	git(root, &["checkout", "-b", "feature"]);

	copy_directory(&head, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "feature changes"]);

	let latest_tag = latest_workspace_release_tag(root)
		.unwrap_or_else(|error| panic!("latest workspace tag: {error}"));
	assert_eq!(latest_tag, "v1.0.0");
	assert_eq!(
		default_branch_ref(root).unwrap_or_else(|error| panic!("default branch: {error}")),
		"main"
	);

	let analysis = analyze_release_trajectory(root, &AnalysisConfig::default())
		.unwrap_or_else(|error| panic!("release trajectory: {error}"));
	assert_eq!(analysis.refs.release_ref, "v1.0.0");
	assert_eq!(analysis.refs.main_ref, "main");
	assert_eq!(analysis.refs.head_ref, "feature");
	assert!(analysis.warnings.is_empty());
	assert!(
		analysis
			.frames
			.main_to_head
			.package_analyses
			.get("core")
			.unwrap_or_else(|| panic!("missing core package analysis"))
			.semantic_changes
			.iter()
			.any(|change| change.item_path == "shout")
	);
}

#[test]
fn git_and_snapshot_helpers_cover_remaining_error_paths() {
	let tempdir = setup_analysis_repo("analysis/cargo-public-api-diff/before");
	let root = tempdir.path().to_path_buf();
	let package_root = Path::new("crates/core");
	let large_file = root.join("crates/core/src/large.rs");
	fs::write(&large_file, "a".repeat(300_000))
		.unwrap_or_else(|error| panic!("write large file: {error}"));

	let medium_file = root.join("crates/core/src/medium.rs");
	fs::write(&medium_file, "m".repeat(2_000))
		.unwrap_or_else(|error| panic!("write medium file: {error}"));
	let exact_limit_file = root.join("crates/core/src/exact.rs");
	fs::write(&exact_limit_file, "e".repeat(256 * 1024))
		.unwrap_or_else(|error| panic!("write exact limit file: {error}"));
	let file_named_target = root.join("crates/core/src/target");
	fs::write(&file_named_target, "not a directory")
		.unwrap_or_else(|error| panic!("write target file: {error}"));
	git(&root, &["add", "."]);
	git(&root, &["commit", "-m", "add fixture files"]);

	let working_files = snapshot_files_from_working_tree(&root, package_root)
		.unwrap_or_else(|error| panic!("working tree snapshot: {error}"));
	assert!(
		!working_files
			.iter()
			.any(|file| file.path == Path::new("src/large.rs"))
	);
	assert!(
		working_files
			.iter()
			.any(|file| file.path == Path::new("src/medium.rs"))
	);
	assert!(
		working_files
			.iter()
			.any(|file| file.path == Path::new("src/target")),
		"a file named 'target' should be included in the snapshot"
	);
	assert_eq!(
		read_working_tree_text(&medium_file),
		Some("m".repeat(2_000))
	);
	assert_eq!(
		read_text_file_from_git_object(&root, "HEAD:crates/core/src/medium.rs")
			.unwrap_or_else(|error| panic!("read medium git object: {error}")),
		Some("m".repeat(2_000))
	);
	assert_eq!(
		read_text_file_from_git_object(&root, "HEAD:crates/core/src/exact.rs")
			.unwrap_or_else(|error| panic!("read exact-size git object: {error}")),
		Some("e".repeat(256 * 1024))
	);

	let merge_base_error = git_merge_base(&root, "main", "missing-branch")
		.unwrap_err()
		.to_string();
	assert!(merge_base_error.contains("git merge-base main missing-branch failed"));

	fs::write(root.join("crates/core/src/lib.rs"), "pub struct Indexed;\n")
		.unwrap_or_else(|error| panic!("rewrite lib.rs: {error}"));
	git(&root, &["add", "crates/core/src/lib.rs"]);

	let package = discover_analysis_workspace(&root)
		.unwrap_or_else(|error| panic!("discover analysis workspace: {error}"))
		.packages
		.into_iter()
		.find(|package| package.name == "core")
		.unwrap_or_else(|| panic!("missing core package"));
	let index_snapshot = snapshot_package(&root, &package, &SnapshotTarget::GitIndex)
		.unwrap_or_else(|error| panic!("index package snapshot: {error}"));
	assert!(
		index_snapshot
			.files
			.iter()
			.any(|file| file.path == Path::new("src/lib.rs"))
	);

	let spawn_error = git_list_files(&root.join("missing-repo"), &["ls-files"])
		.unwrap_err()
		.render();
	assert!(spawn_error.contains("failed to run git [\"ls-files\"]"));

	let binary_file = root.join("crates/core/src/invalid.bin");
	fs::write(&binary_file, [0_u8, 159, 146, 150])
		.unwrap_or_else(|error| panic!("write invalid binary file: {error}"));
	git(&root, &["add", "crates/core/src/invalid.bin"]);
	git(&root, &["commit", "-m", "add invalid binary file"]);

	let utf8_error = git_list_files(&root, &["show", "HEAD:crates/core/src/invalid.bin"])
		.unwrap_err()
		.render();
	assert!(utf8_error.contains("invalid utf-8"));
}
