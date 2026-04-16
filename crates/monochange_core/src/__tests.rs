use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_test_helpers::copy_directory;
use semver::Version;
use serde_json::json;
use tempfile::TempDir;
use tempfile::tempdir;

use crate::BumpSeverity;
use crate::ChangelogFormat;
use crate::ChangelogTarget;
use crate::ChangesetPolicyStatus;
use crate::ChangesetVerificationSettings;
use crate::CliStepDefinition;
use crate::DependencyKind;
use crate::Ecosystem;
use crate::EcosystemSettings;
use crate::ExtraChangelogSection;
use crate::GroupChangelogInclude;
use crate::GroupDefinition;
use crate::HostedIssueCommentPlan;
use crate::HostedSourceAdapter;
use crate::HostedSourceFeatures;
use crate::HostingProviderKind;
use crate::MonochangeError;
use crate::PackageDefinition;
use crate::PackageDependency;
use crate::PackageRecord;
use crate::PackageType;
use crate::ProviderBotSettings;
use crate::ProviderMergeRequestSettings;
use crate::ProviderReleaseSettings;
use crate::PublishMode;
use crate::PublishSettings;
use crate::PublishState;
use crate::RELEASE_RECORD_END_MARKER;
use crate::RELEASE_RECORD_HEADING;
use crate::RELEASE_RECORD_KIND;
use crate::RELEASE_RECORD_SCHEMA_VERSION;
use crate::RELEASE_RECORD_START_MARKER;
use crate::RateLimitOperation;
use crate::RegistryKind;
use crate::ReleaseManifest;
use crate::ReleaseManifestPlan;
use crate::ReleaseNotesDocument;
use crate::ReleaseNotesSection;
use crate::ReleaseNotesSettings;
use crate::ReleaseOwnerKind;
use crate::ReleaseRecord;
use crate::ReleaseRecordDiscovery;
use crate::ReleaseRecordError;
use crate::ReleaseRecordProvider;
use crate::ReleaseRecordTarget;
use crate::RetargetOperation;
use crate::RetargetPlan;
use crate::RetargetProviderOperation;
use crate::RetargetProviderResult;
use crate::RetargetResult;
use crate::RetargetTagResult;
use crate::ShellConfig;
use crate::SourceConfiguration;
use crate::SourceProvider;
use crate::VersionFormat;
use crate::VersionedFileDefinition;
use crate::WorkspaceConfiguration;
use crate::WorkspaceDefaults;
use crate::default_cli_commands;
use crate::git::git_checkout_branch_command;
use crate::git::git_command;
use crate::git::git_current_branch;
use crate::git::git_head_commit;
use crate::git::git_push_branch_command;
use crate::materialize_dependency_edges;
use crate::render_release_notes;

fn must_ok<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
	match result {
		Ok(value) => value,
		Err(error) => panic!("{context}: {error}"),
	}
}

fn must_err<T, E>(result: Result<T, E>, context: &str) -> E {
	match result {
		Ok(_) => panic!("{context}"),
		Err(error) => error,
	}
}

#[derive(Clone)]
struct TestHostedSourceAdapter {
	provider: SourceProvider,
	features: HostedSourceFeatures,
	issue_comment_plans: Vec<HostedIssueCommentPlan>,
}

impl HostedSourceAdapter for TestHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		self.provider
	}

	fn features(&self) -> HostedSourceFeatures {
		self.features
	}

	fn annotate_changeset_context(
		&self,
		_source: &SourceConfiguration,
		_changesets: &mut [crate::PreparedChangeset],
	) {
	}

	fn plan_released_issue_comments(
		&self,
		_source: &SourceConfiguration,
		_manifest: &ReleaseManifest,
	) -> Vec<HostedIssueCommentPlan> {
		self.issue_comment_plans.clone()
	}
}

struct DefaultHostedSourceAdapter {
	provider: SourceProvider,
}

impl HostedSourceAdapter for DefaultHostedSourceAdapter {
	fn provider(&self) -> SourceProvider {
		self.provider
	}

	fn annotate_changeset_context(
		&self,
		_source: &SourceConfiguration,
		changesets: &mut [crate::PreparedChangeset],
	) {
		if let Some(first) = changesets.first_mut() {
			first.summary = Some("annotated".to_string());
		}
	}
}

fn test_source_configuration(provider: SourceProvider) -> SourceConfiguration {
	SourceConfiguration {
		provider,
		owner: "org".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
		bot: ProviderBotSettings::default(),
	}
}

fn test_release_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: true,
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: Vec::new(),
		released_packages: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		package_publications: Vec::new(),
		changesets: Vec::new(),
		deleted_changesets: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}

fn test_retarget_tags() -> Vec<RetargetTagResult> {
	vec![RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Planned,
		message: None,
	}]
}

fn init_git_repository(root: &Path) {
	let init = git_command(root)
		.args(["init", "-b", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git init: {error}"));
	if init.status.success() {
		return;
	}
	let fallback = git_command(root)
		.arg("init")
		.output()
		.unwrap_or_else(|error| panic!("git init fallback: {error}"));
	assert!(
		fallback.status.success(),
		"git init failed:\n{}",
		String::from_utf8_lossy(&init.stderr)
	);
	let checkout = git_command(root)
		.args(["checkout", "-B", "main"])
		.output()
		.unwrap_or_else(|error| panic!("git checkout -B main: {error}"));
	assert!(
		checkout.status.success(),
		"git checkout -B main failed:\n{}",
		String::from_utf8_lossy(&checkout.stderr)
	);
}

#[test]
fn publish_mode_and_registry_kind_display_canonical_names() {
	assert_eq!(PublishMode::Builtin.as_str(), "builtin");
	assert_eq!(PublishMode::External.as_str(), "external");
	assert_eq!(PublishMode::Builtin.to_string(), "builtin");
	assert_eq!(PublishMode::External.to_string(), "external");

	assert_eq!(RegistryKind::CratesIo.as_str(), "crates_io");
	assert_eq!(RegistryKind::Npm.as_str(), "npm");
	assert_eq!(RegistryKind::Jsr.as_str(), "jsr");
	assert_eq!(RegistryKind::PubDev.as_str(), "pub_dev");
}

#[test]
fn must_ok_panics_on_errors() {
	assert!(std::panic::catch_unwind(|| must_ok::<(), _>(Err("boom"), "context")).is_err());
}

#[test]
fn must_err_panics_on_ok_results() {
	assert!(std::panic::catch_unwind(|| must_err(Ok::<(), &str>(()), "context")).is_err());
}

#[test]
fn hosted_source_adapter_default_comment_publishing_returns_empty_when_no_plans_exist() {
	let adapter = DefaultHostedSourceAdapter {
		provider: SourceProvider::GitLab,
	};
	let source = test_source_configuration(SourceProvider::GitLab);
	let manifest = test_release_manifest();

	let outcomes = must_ok(
		adapter.comment_released_issues(&source, &manifest),
		"default comment publishing should allow empty plans",
	);

	assert!(outcomes.is_empty());
}

#[test]
fn hosted_source_adapter_default_features_are_empty_and_comment_plans_are_empty() {
	let adapter = DefaultHostedSourceAdapter {
		provider: SourceProvider::GitLab,
	};
	let source = test_source_configuration(SourceProvider::GitLab);
	let manifest = test_release_manifest();

	assert_eq!(adapter.features(), HostedSourceFeatures::default());
	assert!(
		adapter
			.plan_released_issue_comments(&source, &manifest)
			.is_empty()
	);
}

#[test]
fn hosted_source_adapter_default_enrich_delegates_to_annotate() {
	let adapter = DefaultHostedSourceAdapter {
		provider: SourceProvider::GitLab,
	};
	let source = test_source_configuration(SourceProvider::GitLab);
	let mut changesets = vec![crate::PreparedChangeset {
		path: PathBuf::from(".changeset/example.md"),
		summary: None,
		details: None,
		targets: Vec::new(),
		context: None,
	}];

	adapter.enrich_changeset_context(&source, &mut changesets);

	assert_eq!(
		changesets
			.first()
			.and_then(|changeset| changeset.summary.as_deref()),
		Some("annotated")
	);
}

#[test]
fn hosted_source_adapter_default_comment_publishing_errors_when_provider_lacks_support() {
	let adapter = TestHostedSourceAdapter {
		provider: SourceProvider::GitLab,
		features: HostedSourceFeatures::default(),
		issue_comment_plans: vec![HostedIssueCommentPlan {
			repository: "org/repo".to_string(),
			issue_id: "#7".to_string(),
			issue_url: Some("https://gitlab.example.com/org/repo/-/issues/7".to_string()),
			body: "Released in v1.2.3.".to_string(),
		}],
	};
	let source = test_source_configuration(SourceProvider::GitLab);
	let manifest = test_release_manifest();

	let error = must_err(
		adapter.comment_released_issues(&source, &manifest),
		"default comment publishing should reject unsupported providers",
	);

	assert!(
		error
			.to_string()
			.contains("released issue comments are not yet supported for gitlab")
	);
}

#[test]
fn hosted_source_adapter_default_retarget_planning_marks_unsupported_providers() {
	let adapter = TestHostedSourceAdapter {
		provider: SourceProvider::GitLab,
		features: HostedSourceFeatures::default(),
		issue_comment_plans: Vec::new(),
	};

	let plan = adapter.plan_retargeted_releases(&test_retarget_tags());
	let plan_entry = plan
		.first()
		.expect("default retarget planning should emit one entry");

	assert_eq!(plan.len(), 1);
	assert_eq!(plan_entry.provider, SourceProvider::GitLab);
	assert_eq!(plan_entry.operation, RetargetProviderOperation::Unsupported);
	assert_eq!(
		plan_entry.message.as_deref(),
		Some("provider sync is not yet supported for gitlab release retargeting")
	);
}

#[test]
fn hosted_source_adapter_default_retarget_sync_uses_dry_run_plans_and_blocks_real_runs() {
	let adapter = TestHostedSourceAdapter {
		provider: SourceProvider::GitHub,
		features: HostedSourceFeatures {
			batched_changeset_context_lookup: true,
			released_issue_comments: true,
			release_retarget_sync: true,
		},
		issue_comment_plans: Vec::new(),
	};
	let source = test_source_configuration(SourceProvider::GitHub);
	let tags = test_retarget_tags();

	let dry_run_plan = must_ok(
		adapter.sync_retargeted_releases(&source, &tags, true),
		"default retarget sync should reuse dry-run planning",
	);
	let dry_run_entry = dry_run_plan
		.first()
		.expect("default retarget sync should emit one dry-run entry");
	assert_eq!(dry_run_entry.operation, RetargetProviderOperation::Planned);

	let error = must_err(
		adapter.sync_retargeted_releases(&source, &tags, false),
		"default retarget sync should reject unsupported real runs",
	);
	assert!(
		error
			.to_string()
			.contains("provider sync is not yet supported for github release retargeting")
	);
}

#[test]
fn git_checkout_branch_command_builds_expected_arguments() {
	let root = PathBuf::from("/tmp/test-repo");
	let command = git_checkout_branch_command(&root, "release/v1.0.0");
	let args: Vec<_> = command.get_args().collect();
	assert_eq!(args, &["checkout", "-B", "release/v1.0.0"]);
	assert_eq!(command.get_current_dir(), Some(root.as_path()));
}

#[test]
fn git_push_branch_command_builds_expected_arguments() {
	let root = PathBuf::from("/tmp/test-repo");
	let command = git_push_branch_command(&root, "release/v1.0.0");
	let args: Vec<_> = command.get_args().collect();
	assert_eq!(
		args,
		&[
			"push",
			"--force-with-lease",
			"origin",
			"HEAD:release/v1.0.0"
		]
	);
	assert_eq!(command.get_current_dir(), Some(root.as_path()));
}

#[test]
fn git_current_branch_reports_checked_out_branch_name() {
	let tempdir = must_ok(tempdir(), "tempdir");
	let root = tempdir.path();
	init_git_repository(root);
	let branch = must_ok(git_current_branch(root), "current branch");
	assert_eq!(branch, "main");
}

#[test]
fn git_current_branch_reports_detached_head_as_an_error() {
	let tempdir = must_ok(tempdir(), "tempdir");
	let root = tempdir.path();
	init_git_repository(root);
	for args in [
		["config", "user.name", "monochange Tests"],
		["config", "user.email", "monochange@example.com"],
		["config", "commit.gpgsign", "false"],
	] {
		let output = git_command(root)
			.args(args)
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(
			output.status.success(),
			"git {args:?} failed: {}",
			String::from_utf8_lossy(&output.stderr)
		);
	}
	must_ok(
		fs::write(root.join("README.md"), "hello\n"),
		"write README.md",
	);
	for args in [
		&["add", "README.md"][..],
		&["commit", "-m", "initial"][..],
		&["checkout", "--detach"][..],
	] {
		let output = git_command(root)
			.args(args)
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(
			output.status.success(),
			"git {args:?} failed: {}",
			String::from_utf8_lossy(&output.stderr)
		);
	}

	let error = must_err(git_current_branch(root), "expected detached-head error");
	assert!(
		error
			.to_string()
			.contains("failed to read current git branch"),
		"error: {error}"
	);
}

#[test]
fn git_current_branch_reports_missing_directory_as_io_error() {
	let tempdir = must_ok(tempdir(), "tempdir");
	let missing_root = tempdir.path().join("missing");
	let error = must_err(
		git_current_branch(&missing_root),
		"expected missing-directory error",
	);
	assert!(
		matches!(error, MonochangeError::Io(message) if message.contains("failed to read current git branch"))
	);
}

#[test]
fn git_head_commit_reports_current_commit_sha() {
	let tempdir = must_ok(tempdir(), "tempdir");
	let root = tempdir.path();
	init_git_repository(root);
	for args in [
		["config", "user.name", "monochange Tests"],
		["config", "user.email", "monochange@example.com"],
		["config", "commit.gpgsign", "false"],
	] {
		let output = git_command(root)
			.args(args)
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(
			output.status.success(),
			"git {args:?} failed: {}",
			String::from_utf8_lossy(&output.stderr)
		);
	}
	must_ok(
		fs::write(root.join("README.md"), "hello\n"),
		"write README.md",
	);
	for args in [&["add", "README.md"][..], &["commit", "-m", "initial"][..]] {
		let output = git_command(root)
			.args(args)
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(
			output.status.success(),
			"git {args:?} failed: {}",
			String::from_utf8_lossy(&output.stderr)
		);
	}

	let sha = must_ok(git_head_commit(root), "head commit");
	assert_eq!(sha.len(), 40);
}

#[test]
fn git_head_commit_reports_unborn_head_as_config_error() {
	let tempdir = must_ok(tempdir(), "tempdir");
	let root = tempdir.path();
	init_git_repository(root);
	let error = must_err(git_head_commit(root), "expected unborn HEAD config error");
	assert!(
		matches!(error, MonochangeError::Config(message) if message.contains("failed to read HEAD commit"))
	);
}

#[test]
fn source_change_request_operation_serializes_skipped_variant() {
	assert_eq!(
		serde_json::to_value(crate::SourceChangeRequestOperation::Skipped)
			.unwrap_or_else(|error| panic!("serialize skipped operation: {error}")),
		json!("skipped")
	);
}

#[test]
fn versioned_file_definition_uses_regex_returns_true_when_set() {
	let definition = VersionedFileDefinition {
		path: "README.md".to_string(),
		ecosystem_type: None,
		prefix: None,
		fields: None,
		name: None,
		regex: Some(r"v(?<version>\d+\.\d+\.\d+)".to_string()),
	};
	assert!(definition.uses_regex());
}

#[test]
fn versioned_file_definition_uses_regex_returns_false_when_unset() {
	let definition = VersionedFileDefinition {
		path: "Cargo.toml".to_string(),
		ecosystem_type: Some(crate::EcosystemType::Cargo),
		prefix: None,
		fields: None,
		name: None,
		regex: None,
	};
	assert!(!definition.uses_regex());
}

#[test]
fn workspace_defaults_default_has_no_extra_changelog_sections() {
	assert!(
		WorkspaceDefaults::default()
			.extra_changelog_sections
			.is_empty()
	);
}

#[test]
fn extra_changelog_section_supports_description_field() {
	let section = ExtraChangelogSection {
		name: "Testing".to_string(),
		types: vec!["test".to_string()],
		default_bump: None,
		description: Some("Changes that only modify tests".to_string()),
	};
	assert_eq!(section.name, "Testing");
	assert_eq!(section.types, vec!["test"]);
	assert_eq!(section.default_bump, None);
	assert_eq!(
		section.description,
		Some("Changes that only modify tests".to_string())
	);
}

#[test]
fn extra_changelog_section_description_is_optional() {
	let section = ExtraChangelogSection {
		name: "Security".to_string(),
		types: vec!["security".to_string()],
		default_bump: Some(BumpSeverity::Patch),
		description: None,
	};
	assert_eq!(section.description, None);
}

#[test]
fn bump_severity_orders_from_none_to_major() {
	assert!(BumpSeverity::Patch > BumpSeverity::None);
	assert!(BumpSeverity::Minor > BumpSeverity::Patch);
	assert!(BumpSeverity::Major > BumpSeverity::Minor);
}

#[test]
fn apply_to_version_bumps_stable_versions_normally() {
	let version = Version::new(1, 2, 3);
	assert_eq!(
		BumpSeverity::Patch.apply_to_version(&version),
		Version::new(1, 2, 4)
	);
	assert_eq!(
		BumpSeverity::Minor.apply_to_version(&version),
		Version::new(1, 3, 0)
	);
	assert_eq!(
		BumpSeverity::Major.apply_to_version(&version),
		Version::new(2, 0, 0)
	);
	assert_eq!(
		BumpSeverity::None.apply_to_version(&version),
		Version::new(1, 2, 3)
	);
}

#[test]
fn apply_to_version_shifts_bumps_for_pre_stable_versions() {
	let version = Version::new(0, 1, 0);

	// major becomes minor for pre-1.0
	assert_eq!(
		BumpSeverity::Major.apply_to_version(&version),
		Version::new(0, 2, 0)
	);

	// minor becomes patch for pre-1.0
	assert_eq!(
		BumpSeverity::Minor.apply_to_version(&version),
		Version::new(0, 1, 1)
	);

	// patch stays patch
	assert_eq!(
		BumpSeverity::Patch.apply_to_version(&version),
		Version::new(0, 1, 1)
	);

	// none stays none
	assert_eq!(
		BumpSeverity::None.apply_to_version(&version),
		Version::new(0, 1, 0)
	);
}

#[test]
fn apply_to_version_pre_stable_at_zero_zero() {
	let version = Version::new(0, 0, 1);
	assert_eq!(
		BumpSeverity::Major.apply_to_version(&version),
		Version::new(0, 1, 0)
	);
	assert_eq!(
		BumpSeverity::Minor.apply_to_version(&version),
		Version::new(0, 0, 2)
	);
	assert_eq!(
		BumpSeverity::Patch.apply_to_version(&version),
		Version::new(0, 0, 2)
	);
}

#[test]
fn is_pre_stable_returns_true_for_zero_major() {
	assert!(BumpSeverity::is_pre_stable(&Version::new(0, 1, 0)));
	assert!(BumpSeverity::is_pre_stable(&Version::new(0, 0, 1)));
	assert!(BumpSeverity::is_pre_stable(&Version::new(0, 99, 99)));
	assert!(!BumpSeverity::is_pre_stable(&Version::new(1, 0, 0)));
	assert!(!BumpSeverity::is_pre_stable(&Version::new(2, 0, 0)));
}

#[test]
fn discovery_path_filter_rejects_gitignored_paths() {
	let fixture = setup_discovery_fixture("ignore-gitignored-nested-worktree");
	let root = fixture.path();
	let filter = crate::DiscoveryPathFilter::new(root);

	assert!(!filter.should_descend(&root.join(".claude")));
	assert!(!filter.allows(&root.join(".claude/worktrees/feature")));
	assert!(filter.allows(&root.join("crates/root/Cargo.toml")));
}

#[test]
fn discovery_path_filter_rejects_paths_under_nested_git_worktrees() {
	let fixture = setup_discovery_fixture("ignore-automatic-nested-worktree");
	let root = fixture.path();
	let filter = crate::DiscoveryPathFilter::new(root);

	assert!(!filter.should_descend(&root.join("sandbox/feature")));
	assert!(!filter.allows(&root.join("sandbox/feature/crates/ignored/Cargo.toml")));
	assert!(filter.allows(&root.join("crates/root/Cargo.toml")));
}

fn setup_discovery_fixture(name: &str) -> TempDir {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo")
		.join(name);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	materialize_nested_worktree_gitdir(tempdir.path());
	tempdir
}

fn materialize_nested_worktree_gitdir(root: &Path) {
	for (placeholder, git_path) in [
		(
			root.join("sandbox/feature/gitdir.txt"),
			root.join("sandbox/feature/.git"),
		),
		(
			root.join("feature.gitdir"),
			root.join(".claude/worktrees/feature/.git"),
		),
	] {
		if placeholder.is_file() {
			let gitdir = fs::read_to_string(&placeholder)
				.unwrap_or_else(|error| panic!("read {}: {error}", placeholder.display()));
			if let Some(parent) = git_path.parent() {
				fs::create_dir_all(parent)
					.unwrap_or_else(|error| panic!("create parent {}: {error}", parent.display()));
			}
			fs::write(&git_path, gitdir)
				.unwrap_or_else(|error| panic!("write {}: {error}", git_path.display()));
		}
	}
}

#[test]
fn package_record_uses_manifest_path_for_stable_id() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("fixtures/cargo/workspace/crates/core/Cargo.toml"),
		PathBuf::from("fixtures/cargo/workspace"),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);

	assert_eq!(package.id, "cargo:crates/core/Cargo.toml");
	assert_eq!(package.current_version, Some(Version::new(1, 2, 3)));
}

#[test]
fn package_record_ids_are_stable_for_relative_and_absolute_roots() {
	let workspace_root = PathBuf::from("fixtures/cargo/workspace");
	let manifest_path = workspace_root.join("crates/core/Cargo.toml");
	let relative = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		manifest_path.clone(),
		workspace_root.clone(),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);
	let absolute_root = std::env::current_dir()
		.unwrap_or_else(|error| panic!("cwd: {error}"))
		.join(&workspace_root);
	let absolute = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		absolute_root.join("crates/core/Cargo.toml"),
		absolute_root,
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);

	assert_eq!(relative.id, absolute.id);
	assert_eq!(relative.id, "cargo:crates/core/Cargo.toml");
}

#[test]
fn package_dependencies_preserve_kind_and_constraint() {
	let dependency = PackageDependency {
		name: "workspace-shared".to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("^1.0.0".to_string()),
		optional: false,
	};

	assert_eq!(dependency.kind, DependencyKind::Runtime);
	assert_eq!(dependency.version_constraint.as_deref(), Some("^1.0.0"));
}

#[test]
fn materialize_dependency_edges_matches_dependency_names_to_packages() {
	let target = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-shared",
		PathBuf::from("fixtures/cargo/workspace/crates/shared/Cargo.toml"),
		PathBuf::from("fixtures/cargo/workspace"),
		None,
		PublishState::Public,
	);
	let mut source = PackageRecord::new(
		Ecosystem::Cargo,
		"workspace-app",
		PathBuf::from("fixtures/cargo/workspace/crates/app/Cargo.toml"),
		PathBuf::from("fixtures/cargo/workspace"),
		None,
		PublishState::Public,
	);
	source.declared_dependencies.push(PackageDependency {
		name: "workspace-shared".to_string(),
		kind: DependencyKind::Runtime,
		version_constraint: Some("^1.0.0".to_string()),
		optional: false,
	});

	let edges = materialize_dependency_edges(&[source.clone(), target.clone()]);
	assert_eq!(edges.len(), 1);
	let edge = edges.first().unwrap_or_else(|| panic!("expected one edge"));
	assert_eq!(edge.from_package_id, source.id);
	assert_eq!(edge.to_package_id, target.id);
}

#[test]
fn changeset_policy_status_renders_stable_strings() {
	assert_eq!(ChangesetPolicyStatus::Passed.as_str(), "passed");
	assert_eq!(ChangesetPolicyStatus::Failed.to_string(), "failed");
	assert_eq!(ChangesetPolicyStatus::Skipped.as_str(), "skipped");
	assert_eq!(ChangesetPolicyStatus::NotRequired.as_str(), "not_required");
}

#[test]
fn changeset_verification_settings_default_to_enabled_enforcement() {
	let settings = ChangesetVerificationSettings::default();
	assert!(settings.enabled);
	assert!(settings.required);
	assert!(settings.comment_on_failure);
	assert!(settings.skip_labels.is_empty());
}

#[test]
fn default_cli_commands_expose_publish_flows_alongside_release_planning() {
	let cli = default_cli_commands();
	let cli_command_names = cli
		.iter()
		.map(|cli_command| cli_command.name.as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		cli_command_names,
		vec![
			"validate",
			"discover",
			"change",
			"release",
			"placeholder-publish",
			"publish",
			"publish-plan",
			"affected",
			"diagnostics",
			"repair-release"
		]
	);
	let validate_cli_command = cli
		.first()
		.unwrap_or_else(|| panic!("expected validate cli command"));
	assert_eq!(
		validate_cli_command.steps,
		vec![CliStepDefinition::Validate {
			name: Some("validate workspace".to_string()),
			when: None,
			inputs: BTreeMap::new(),
		}]
	);

	let placeholder_publish = cli
		.iter()
		.find(|command| command.name == "placeholder-publish")
		.unwrap_or_else(|| panic!("expected placeholder-publish cli command"));
	assert_eq!(
		placeholder_publish.steps,
		vec![CliStepDefinition::PlaceholderPublish {
			name: Some("publish placeholder packages".to_string()),
			when: None,
			inputs: BTreeMap::new(),
		}]
	);

	let publish = cli
		.iter()
		.find(|command| command.name == "publish")
		.unwrap_or_else(|| panic!("expected publish cli command"));
	assert_eq!(
		publish.steps,
		vec![CliStepDefinition::PublishPackages {
			name: Some("publish packages".to_string()),
			when: None,
			inputs: BTreeMap::new(),
		}]
	);

	let publish_plan = cli
		.iter()
		.find(|command| command.name == "publish-plan")
		.unwrap_or_else(|| panic!("expected publish-plan cli command"));
	assert_eq!(
		publish_plan.steps,
		vec![CliStepDefinition::PlanPublishRateLimits {
			name: Some("plan publish rate limits".to_string()),
			when: None,
			inputs: BTreeMap::new(),
		}]
	);
}

#[test]
fn default_release_command_prefers_markdown_output() {
	let release = default_cli_commands()
		.into_iter()
		.find(|command| command.name == "release")
		.unwrap_or_else(|| panic!("expected release command"));
	let format = release
		.inputs
		.iter()
		.find(|input| input.name == "format")
		.unwrap_or_else(|| panic!("expected release format input"));
	assert_eq!(format.default.as_deref(), Some("markdown"));
	assert_eq!(
		format.choices,
		vec![
			"markdown".to_string(),
			"text".to_string(),
			"json".to_string(),
		]
	);
	let versions = release
		.inputs
		.iter()
		.find(|input| input.name == "versions")
		.unwrap_or_else(|| panic!("expected release versions input"));
	assert_eq!(versions.kind, crate::CliInputKind::Boolean);
	assert_eq!(versions.default.as_deref(), Some("false"));
	assert_eq!(
		versions.help_text.as_deref(),
		Some("Print only planned package and group versions (implies dry-run)")
	);
}

#[test]
fn cli_step_definition_kind_name_covers_all_variants() {
	use std::collections::BTreeMap;
	let cases: Vec<(CliStepDefinition, &str)> = vec![
		(
			CliStepDefinition::Validate {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"Validate",
		),
		(
			CliStepDefinition::Discover {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"Discover",
		),
		(
			CliStepDefinition::CreateChangeFile {
				show_progress: None,
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"CreateChangeFile",
		),
		(
			CliStepDefinition::PrepareRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"PrepareRelease",
		),
		(
			CliStepDefinition::CommitRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"CommitRelease",
		),
		(
			CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"PublishRelease",
		),
		(
			CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"OpenReleaseRequest",
		),
		(
			CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"CommentReleasedIssues",
		),
		(
			CliStepDefinition::AffectedPackages {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"AffectedPackages",
		),
		(
			CliStepDefinition::DiagnoseChangesets {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"DiagnoseChangesets",
		),
		(
			CliStepDefinition::RetargetRelease {
				name: None,
				when: None,
				inputs: BTreeMap::new(),
			},
			"RetargetRelease",
		),
		(
			CliStepDefinition::Command {
				show_progress: None,
				name: None,
				when: None,
				command: "echo".into(),
				dry_run_command: None,
				shell: ShellConfig::None,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			},
			"Command",
		),
	];
	for (step, expected) in cases {
		assert_eq!(step.kind_name(), expected);
	}
}

#[test]
fn cli_step_show_progress_returns_configured_values_for_interactive_steps() {
	let create_change = CliStepDefinition::CreateChangeFile {
		show_progress: Some(false),
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	let command = CliStepDefinition::Command {
		show_progress: Some(true),
		name: None,
		when: None,
		command: "echo hi".to_string(),
		dry_run_command: None,
		shell: ShellConfig::None,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	let validate = CliStepDefinition::Validate {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(create_change.show_progress(), Some(false));
	assert_eq!(command.show_progress(), Some(true));
	assert_eq!(validate.show_progress(), None);
}

#[test]
fn cli_step_display_name_prefers_explicit_name_over_kind() {
	let named = CliStepDefinition::PrepareRelease {
		name: Some("plan release".to_string()),
		when: None,
		inputs: BTreeMap::new(),
	};
	let unnamed = CliStepDefinition::PrepareRelease {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(named.display_name(), "plan release");
	assert_eq!(unnamed.display_name(), "PrepareRelease");
}

#[test]
fn cli_step_name_returns_explicit_names_for_all_variants() {
	let expected = "named step";
	let steps = vec![
		CliStepDefinition::Discover {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::CreateChangeFile {
			show_progress: None,
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PrepareRelease {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::CommitRelease {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::PublishRelease {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::OpenReleaseRequest {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::CommentReleasedIssues {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::AffectedPackages {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::DiagnoseChangesets {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::RetargetRelease {
			name: Some(expected.to_string()),
			when: None,
			inputs: BTreeMap::new(),
		},
		CliStepDefinition::Command {
			show_progress: None,
			name: Some(expected.to_string()),
			when: None,
			command: "echo hi".to_string(),
			dry_run_command: None,
			shell: ShellConfig::None,
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		},
	];
	for step in steps {
		assert_eq!(step.name(), Some(expected));
	}
}

#[test]
fn valid_input_names_returns_none_for_command_steps() {
	let step = CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: None,
		command: "echo hi".into(),
		dry_run_command: None,
		shell: ShellConfig::None,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert!(step.valid_input_names().is_none());
}

#[test]
fn valid_input_names_returns_fix_for_validate() {
	let step = CliStepDefinition::Validate {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(step.valid_input_names(), Some(["fix"].as_slice()));
}

#[test]
fn valid_input_names_returns_empty_for_commit_release() {
	let step = CliStepDefinition::CommitRelease {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(step.valid_input_names(), Some([].as_slice()));
}

#[test]
fn valid_input_names_returns_expected_names_for_affected_packages() {
	let step = CliStepDefinition::AffectedPackages {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	let names = step.valid_input_names().unwrap();
	assert!(names.contains(&"format"));
	assert!(names.contains(&"changed_paths"));
	assert!(names.contains(&"since"));
	assert!(names.contains(&"verify"));
	assert!(names.contains(&"label"));
}

#[test]
fn valid_input_names_returns_expected_names_for_retarget_release() {
	let step = CliStepDefinition::RetargetRelease {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	let names = step.valid_input_names().unwrap();
	for expected in ["from", "target", "force", "sync_provider"] {
		assert!(names.contains(&expected), "missing: {expected}");
	}
}

#[test]
fn valid_input_names_returns_expected_names_for_publish_steps() {
	let publish = CliStepDefinition::PublishPackages {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		publish.valid_input_names(),
		Some(["format", "package"].as_slice())
	);

	let plan = CliStepDefinition::PlanPublishRateLimits {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	let names = plan.valid_input_names().unwrap();
	for expected in ["format", "mode", "package", "ci"] {
		assert!(names.contains(&expected), "missing: {expected}");
	}
}

#[test]
fn valid_input_names_returns_expected_names_for_create_change_file() {
	let step = CliStepDefinition::CreateChangeFile {
		show_progress: None,
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	let names = step.valid_input_names().unwrap();
	for expected in [
		"interactive",
		"package",
		"bump",
		"version",
		"reason",
		"type",
		"details",
		"output",
	] {
		assert!(names.contains(&expected), "missing: {expected}");
	}
}

#[test]
fn default_change_command_supports_none_bump_and_omits_legacy_evidence_input() {
	let change = default_cli_commands()
		.into_iter()
		.find(|command| command.name == "change")
		.unwrap_or_else(|| panic!("expected change command"));
	let bump = change
		.inputs
		.iter()
		.find(|input| input.name == "bump")
		.unwrap_or_else(|| panic!("expected bump input"));
	assert_eq!(
		bump.choices,
		vec![
			"none".to_string(),
			"patch".to_string(),
			"minor".to_string(),
			"major".to_string(),
		]
	);
	assert!(change.inputs.iter().all(|input| input.name != "evidence"));
}

#[test]
fn expected_input_kind_returns_correct_types_for_affected_packages() {
	use crate::CliInputKind;
	let step = CliStepDefinition::AffectedPackages {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		step.expected_input_kind("format"),
		Some(CliInputKind::Choice)
	);
	assert_eq!(
		step.expected_input_kind("changed_paths"),
		Some(CliInputKind::StringList)
	);
	assert_eq!(
		step.expected_input_kind("since"),
		Some(CliInputKind::String)
	);
	assert_eq!(
		step.expected_input_kind("verify"),
		Some(CliInputKind::Boolean)
	);
	assert_eq!(
		step.expected_input_kind("label"),
		Some(CliInputKind::StringList)
	);
	assert_eq!(step.expected_input_kind("unknown"), None);
}

#[test]
fn expected_input_kind_returns_none_for_command_steps() {
	let step = CliStepDefinition::Command {
		show_progress: None,
		name: None,
		when: None,
		command: "echo".into(),
		dry_run_command: None,
		shell: ShellConfig::None,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(step.expected_input_kind("anything"), None);
}

#[test]
fn expected_input_kind_returns_none_for_commit_release() {
	let step = CliStepDefinition::CommitRelease {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(step.expected_input_kind("format"), None);
}

#[test]
fn expected_input_kind_returns_correct_types_for_publish_steps() {
	use crate::CliInputKind;
	let prepare = CliStepDefinition::PrepareRelease {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		prepare.expected_input_kind("format"),
		Some(CliInputKind::Choice)
	);
	assert_eq!(
		prepare.expected_input_kind("versions"),
		Some(CliInputKind::Boolean)
	);
	assert_eq!(prepare.expected_input_kind("unknown"), None);

	let publish = CliStepDefinition::PublishPackages {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		publish.expected_input_kind("format"),
		Some(CliInputKind::Choice)
	);
	assert_eq!(
		publish.expected_input_kind("package"),
		Some(CliInputKind::StringList)
	);
	assert_eq!(publish.expected_input_kind("unknown"), None);

	let plan = CliStepDefinition::PlanPublishRateLimits {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		plan.expected_input_kind("format"),
		Some(CliInputKind::Choice)
	);
	assert_eq!(plan.expected_input_kind("mode"), Some(CliInputKind::Choice));
	assert_eq!(
		plan.expected_input_kind("package"),
		Some(CliInputKind::StringList)
	);
	assert_eq!(plan.expected_input_kind("ci"), Some(CliInputKind::Choice));
	assert_eq!(plan.expected_input_kind("unknown"), None);
}

#[test]
fn rate_limit_operation_labels_cover_all_variants() {
	assert_eq!(
		RateLimitOperation::PlaceholderPublish.as_str(),
		"placeholder_publish"
	);
	assert_eq!(RateLimitOperation::Publish.as_str(), "publish");
	assert_eq!(RateLimitOperation::Update.as_str(), "update");
}

#[test]
fn expected_input_kind_returns_correct_types_for_create_change_file() {
	use crate::CliInputKind;
	let step = CliStepDefinition::CreateChangeFile {
		show_progress: None,
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		step.expected_input_kind("interactive"),
		Some(CliInputKind::Boolean)
	);
	assert_eq!(
		step.expected_input_kind("package"),
		Some(CliInputKind::StringList)
	);
	assert_eq!(step.expected_input_kind("bump"), Some(CliInputKind::Choice));
	assert_eq!(
		step.expected_input_kind("reason"),
		Some(CliInputKind::String)
	);
	assert_eq!(step.expected_input_kind("output"), Some(CliInputKind::Path));
}

#[test]
fn expected_input_kind_returns_correct_types_for_diagnose_changesets() {
	use crate::CliInputKind;
	let step = CliStepDefinition::DiagnoseChangesets {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(
		step.expected_input_kind("format"),
		Some(CliInputKind::Choice)
	);
	assert_eq!(
		step.expected_input_kind("changeset"),
		Some(CliInputKind::StringList)
	);
	assert_eq!(step.expected_input_kind("nonexistent"), None);
}

#[test]
fn expected_input_kind_returns_correct_types_for_retarget_release() {
	use crate::CliInputKind;
	let step = CliStepDefinition::RetargetRelease {
		name: None,
		when: None,
		inputs: BTreeMap::new(),
	};
	assert_eq!(step.expected_input_kind("from"), Some(CliInputKind::String));
	assert_eq!(
		step.expected_input_kind("target"),
		Some(CliInputKind::String)
	);
	assert_eq!(
		step.expected_input_kind("force"),
		Some(CliInputKind::Boolean)
	);
	assert_eq!(
		step.expected_input_kind("sync_provider"),
		Some(CliInputKind::Boolean)
	);
	assert_eq!(step.expected_input_kind("nonexistent"), None);
}

#[test]
fn hosting_provider_kind_as_str_and_display_cover_all_variants() {
	assert_eq!(HostingProviderKind::GenericGit.as_str(), "generic_git");
	assert_eq!(HostingProviderKind::GitHub.as_str(), "github");
	assert_eq!(HostingProviderKind::GitLab.as_str(), "gitlab");
	assert_eq!(HostingProviderKind::Gitea.as_str(), "gitea");
	assert_eq!(HostingProviderKind::Bitbucket.as_str(), "bitbucket");
	assert_eq!(HostingProviderKind::Gitea.to_string(), "gitea");
}

#[test]
fn hosted_review_request_kind_as_str_and_display() {
	use crate::HostedReviewRequestKind;
	assert_eq!(
		HostedReviewRequestKind::PullRequest.as_str(),
		"pull_request"
	);
	assert_eq!(
		HostedReviewRequestKind::MergeRequest.as_str(),
		"merge_request"
	);
	assert_eq!(
		HostedReviewRequestKind::PullRequest.to_string(),
		"pull_request"
	);
	assert_eq!(
		HostedReviewRequestKind::MergeRequest.to_string(),
		"merge_request"
	);
}

#[test]
fn hosted_issue_relationship_kind_as_str_and_display() {
	use crate::HostedIssueRelationshipKind;
	let cases = [
		(
			HostedIssueRelationshipKind::ClosedByReviewRequest,
			"closed_by_review_request",
		),
		(
			HostedIssueRelationshipKind::ReferencedByReviewRequest,
			"referenced_by_review_request",
		),
		(HostedIssueRelationshipKind::Mentioned, "mentioned"),
		(HostedIssueRelationshipKind::Manual, "manual"),
	];
	for (kind, expected) in cases {
		assert_eq!(kind.as_str(), expected);
		assert_eq!(kind.to_string(), expected);
	}
}

#[test]
fn cli_step_definition_rejects_legacy_source_automation_step_aliases() {
	for legacy_type in [
		"PublishGitHubRelease",
		"OpenReleasePullRequest",
		"EnforceChangesetPolicy",
		"VerifyChangesets",
	] {
		let error = serde_json::from_value::<CliStepDefinition>(json!({
			"type": legacy_type
		}))
		.expect_err("legacy step alias should be rejected");
		assert!(
			error.to_string().contains("unknown variant"),
			"legacy type {legacy_type}: {error}"
		);
	}
}

#[test]
fn cli_step_definition_rejects_legacy_command_field_aliases() {
	let error = serde_json::from_value::<CliStepDefinition>(json!({
		"type": "Command",
		"command": "echo hi",
		"dry_run": "echo dry-run"
	}))
	.expect_err("legacy command field alias should be rejected");
	assert!(error.to_string().contains("unknown field `dry_run`"));
}

#[test]
fn source_provider_rejects_legacy_provider_aliases() {
	for legacy_provider in ["git_hub", "git_lab"] {
		let error = serde_json::from_value::<SourceProvider>(json!(legacy_provider))
			.expect_err("legacy provider alias should be rejected");
		assert!(
			error.to_string().contains("unknown variant"),
			"legacy provider {legacy_provider}: {error}"
		);
	}
}

#[test]
fn render_release_notes_supports_monochange_and_keep_a_changelog_formats() {
	let _snapshot = insta::Settings::clone_current().bind_to_scope();
	let document = ReleaseNotesDocument {
		title: "1.2.3".to_string(),
		summary: vec!["Grouped release for `sdk`.".to_string()],
		sections: vec![ReleaseNotesSection {
			title: "Changed".to_string(),
			entries: vec!["add release automation".to_string()],
		}],
	};

	let monochange = render_release_notes(ChangelogFormat::Monochange, &document);
	let keep_a_changelog = render_release_notes(ChangelogFormat::KeepAChangelog, &document);

	insta::assert_snapshot!(
		"render_release_notes_supports_monochange_and_keep_a_changelog_formats__monochange",
		monochange
	);
	insta::assert_snapshot!(
		"render_release_notes_supports_monochange_and_keep_a_changelog_formats__keep_a_changelog",
		keep_a_changelog
	);
}

#[test]
fn workspace_configuration_can_find_group_membership_for_a_package() {
	let configuration = sample_workspace_configuration();
	let group = configuration
		.group_for_package("monochange")
		.unwrap_or_else(|| panic!("expected package group"));

	assert_eq!(group.id, "workspace");
	assert_eq!(group.packages, vec!["monochange", "monochange_core"]);
}

#[test]
fn workspace_configuration_uses_group_release_identity_for_group_members() {
	let configuration = sample_workspace_configuration();
	let identity = configuration
		.effective_release_identity("monochange")
		.unwrap_or_else(|| panic!("expected release identity"));

	assert_eq!(identity.owner_id, "workspace");
	assert_eq!(identity.owner_kind, ReleaseOwnerKind::Group);
	assert_eq!(identity.group_id.as_deref(), Some("workspace"));
	assert!(identity.tag);
	assert!(identity.release);
	assert_eq!(identity.version_format, VersionFormat::Primary);
	assert_eq!(identity.members, vec!["monochange", "monochange_core"]);
}

#[test]
fn workspace_configuration_uses_package_release_identity_when_not_grouped() {
	let configuration = sample_workspace_configuration();
	let identity = configuration
		.effective_release_identity("monochange_graph")
		.unwrap_or_else(|| panic!("expected release identity"));

	assert_eq!(identity.owner_id, "monochange_graph");
	assert_eq!(identity.owner_kind, ReleaseOwnerKind::Package);
	assert_eq!(identity.group_id, None);
	assert!(!identity.tag);
	assert!(!identity.release);
	assert_eq!(identity.version_format, VersionFormat::Namespaced);
	assert_eq!(identity.members, vec!["monochange_graph"]);
}

fn sample_workspace_configuration() -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: WorkspaceDefaults::default(),
		release_notes: ReleaseNotesSettings::default(),
		packages: vec![
			PackageDefinition {
				id: "monochange".to_string(),
				path: PathBuf::from("crates/monochange"),
				package_type: PackageType::Cargo,
				changelog: Some(ChangelogTarget {
					path: PathBuf::from("crates/monochange/changelog.md"),
					format: ChangelogFormat::Monochange,
				}),
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
				publish: PublishSettings::default(),
			},
			PackageDefinition {
				id: "monochange_core".to_string(),
				path: PathBuf::from("crates/monochange_core"),
				package_type: PackageType::Cargo,
				changelog: Some(ChangelogTarget {
					path: PathBuf::from("crates/monochange_core/changelog.md"),
					format: ChangelogFormat::Monochange,
				}),
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
				publish: PublishSettings::default(),
			},
			PackageDefinition {
				id: "monochange_graph".to_string(),
				path: PathBuf::from("crates/monochange_graph"),
				package_type: PackageType::Cargo,
				changelog: None,
				extra_changelog_sections: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: false,
				release: false,
				version_format: VersionFormat::Namespaced,
				publish: PublishSettings::default(),
			},
		],
		groups: vec![GroupDefinition {
			id: "workspace".to_string(),
			packages: vec!["monochange".to_string(), "monochange_core".to_string()],
			changelog: Some(ChangelogTarget {
				path: PathBuf::from("changelog.md"),
				format: ChangelogFormat::Monochange,
			}),
			changelog_include: GroupChangelogInclude::All,
			extra_changelog_sections: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		cli: Vec::new(),
		changesets: crate::ChangesetSettings::default(),
		source: None,
		cargo: EcosystemSettings::default(),
		npm: EcosystemSettings::default(),
		deno: EcosystemSettings::default(),
		dart: EcosystemSettings::default(),
	}
}

#[test]
fn group_definition_defaults_changelog_include_when_omitted() {
	let group: GroupDefinition = serde_json::from_value(json!({
		"id": "sdk",
		"packages": ["core", "app"],
		"changelog": null,
		"extra_changelog_sections": [],
		"empty_update_message": null,
		"release_title": null,
		"changelog_version_title": null,
		"versioned_files": [],
		"tag": false,
		"release": false,
		"version_format": "namespaced"
	}))
	.unwrap_or_else(|error| panic!("group: {error}"));

	assert_eq!(group.changelog_include, GroupChangelogInclude::All);
}

#[test]
fn shell_config_deserializes_from_bool_and_string() {
	let from_true: ShellConfig = serde_json::from_str("true").unwrap();
	assert_eq!(from_true, ShellConfig::Default);
	assert_eq!(from_true.shell_binary(), Some("sh"));

	let from_false: ShellConfig = serde_json::from_str("false").unwrap();
	assert_eq!(from_false, ShellConfig::None);
	assert_eq!(from_false.shell_binary(), None);

	let from_bash: ShellConfig = serde_json::from_str(r#""bash""#).unwrap();
	assert_eq!(from_bash, ShellConfig::Custom("bash".to_string()));
	assert_eq!(from_bash.shell_binary(), Some("bash"));

	let from_empty: Result<ShellConfig, _> = serde_json::from_str(r#""""#);
	assert!(from_empty.is_err());

	assert_eq!(ShellConfig::default(), ShellConfig::None);
}

#[test]
fn shell_config_serializes_roundtrip() {
	assert_eq!(serde_json::to_string(&ShellConfig::None).unwrap(), "false");
	assert_eq!(
		serde_json::to_string(&ShellConfig::Default).unwrap(),
		"true"
	);
	assert_eq!(
		serde_json::to_string(&ShellConfig::Custom("bash".into())).unwrap(),
		r#""bash""#
	);
}

#[test]
fn cli_step_command_with_id_deserializes() {
	let json_str = r#"{"type":"Command","command":"echo hello","id":"greet","shell":"bash"}"#;
	let step: CliStepDefinition =
		serde_json::from_str(json_str).unwrap_or_else(|error| panic!("deserialize: {error}"));
	match &step {
		CliStepDefinition::Command {
			command, id, shell, ..
		} => {
			assert_eq!(command, "echo hello");
			assert_eq!(id.as_deref(), Some("greet"));
			assert_eq!(shell, &ShellConfig::Custom("bash".to_string()));
		}
		_ => panic!("expected Command step"),
	}
}

#[test]
fn cli_step_command_without_id_has_none() {
	let json_str = r#"{"type":"Command","command":"echo hello","shell":true}"#;
	let step: CliStepDefinition =
		serde_json::from_str(json_str).unwrap_or_else(|error| panic!("deserialize: {error}"));
	match &step {
		CliStepDefinition::Command { id, shell, .. } => {
			assert!(id.is_none());
			assert_eq!(shell, &ShellConfig::Default);
		}
		_ => panic!("expected Command step"),
	}
}

#[test]
fn release_record_deserializes_defaults_for_schema_and_kind() {
	let record: ReleaseRecord = serde_json::from_str(
		r#"{
		  "createdAt": "2026-04-06T12:00:00Z",
		  "command": "release-pr",
		  "releaseTargets": [],
		  "releasedPackages": [],
		  "changedFiles": []
		}"#,
	)
	.unwrap_or_else(|error| panic!("deserialize release record defaults: {error}"));
	assert_eq!(record.schema_version, RELEASE_RECORD_SCHEMA_VERSION);
	assert_eq!(record.kind, RELEASE_RECORD_KIND);
}

#[test]
fn release_record_block_roundtrips_with_reserved_markers() {
	let record = sample_release_record();
	let rendered = crate::render_release_record_block(&record)
		.unwrap_or_else(|error| panic!("render release record: {error}"));

	assert!(rendered.starts_with(RELEASE_RECORD_HEADING));
	assert!(rendered.contains(RELEASE_RECORD_START_MARKER));
	assert!(rendered.contains(RELEASE_RECORD_END_MARKER));
	assert!(rendered.contains("```json"));

	let parsed = crate::parse_release_record_block(&rendered)
		.unwrap_or_else(|error| panic!("parse release record: {error}"));
	assert_eq!(parsed, record);
}

#[test]
fn parse_release_record_block_returns_not_found_without_markers() {
	let error = crate::parse_release_record_block("chore(release): prepare release")
		.err()
		.unwrap_or_else(|| panic!("expected not found error"));
	assert!(matches!(error, ReleaseRecordError::NotFound));
}

#[test]
fn parse_release_record_block_rejects_duplicate_blocks() {
	let rendered = crate::render_release_record_block(&sample_release_record())
		.unwrap_or_else(|error| panic!("render release record: {error}"));
	let duplicated = format!("{rendered}\n\n{rendered}");

	let error = crate::parse_release_record_block(&duplicated)
		.err()
		.unwrap_or_else(|| panic!("expected duplicate block error"));
	assert!(matches!(error, ReleaseRecordError::MultipleBlocks));
}

#[test]
fn parse_release_record_block_rejects_missing_json_fence() {
	let malformed = format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n{{}}\n{RELEASE_RECORD_END_MARKER}"
	);
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected missing json block error"));
	assert!(matches!(error, ReleaseRecordError::MissingJsonBlock));
}

#[test]
fn parse_release_record_block_rejects_invalid_json() {
	let malformed = format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n```json\n{{\n```\n{RELEASE_RECORD_END_MARKER}"
	);
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected invalid json error"));
	assert!(matches!(error, ReleaseRecordError::InvalidJson(_)));
}

#[test]
fn parse_release_record_block_rejects_unsupported_kind() {
	let heading = RELEASE_RECORD_HEADING;
	let start = RELEASE_RECORD_START_MARKER;
	let end = RELEASE_RECORD_END_MARKER;
	let invalid_kind = format!(
		r#"{heading}

{start}
```json
{{
  "schemaVersion": 1,
  "kind": "monochange.otherRecord",
  "createdAt": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": []
}}
```
{end}"#
	);
	let error = crate::parse_release_record_block(&invalid_kind)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported kind error"));
	assert!(matches!(
		error,
		ReleaseRecordError::UnsupportedKind(kind) if kind == "monochange.otherRecord"
	));
}

#[test]
fn parse_release_record_block_rejects_unsupported_schema_version() {
	let heading = RELEASE_RECORD_HEADING;
	let start = RELEASE_RECORD_START_MARKER;
	let end = RELEASE_RECORD_END_MARKER;
	let kind = RELEASE_RECORD_KIND;
	let unsupported_schema = format!(
		r#"{heading}

{start}
```json
{{
  "schemaVersion": 2,
  "kind": "{kind}",
  "createdAt": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": []
}}
```
{end}"#
	);
	let error = crate::parse_release_record_block(&unsupported_schema)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported schema error"));
	assert!(matches!(
		error,
		ReleaseRecordError::UnsupportedSchemaVersion(2)
	));
}

#[test]
fn parse_release_record_block_ignores_unknown_fields() {
	let heading = RELEASE_RECORD_HEADING;
	let start = RELEASE_RECORD_START_MARKER;
	let end = RELEASE_RECORD_END_MARKER;
	let schema = RELEASE_RECORD_SCHEMA_VERSION;
	let kind = RELEASE_RECORD_KIND;
	let with_unknown = format!(
		r#"{heading}

{start}
```json
{{
  "schemaVersion": {schema},
  "kind": "{kind}",
  "createdAt": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": [],
  "unknownField": "ignored"
}}
```
{end}"#
	);
	let parsed = crate::parse_release_record_block(&with_unknown)
		.unwrap_or_else(|error| panic!("parse release record with unknown field: {error}"));
	assert_eq!(parsed.kind, RELEASE_RECORD_KIND);
	assert_eq!(parsed.schema_version, RELEASE_RECORD_SCHEMA_VERSION);
	assert!(parsed.release_targets.is_empty());
}

fn sample_release_record() -> ReleaseRecord {
	ReleaseRecord {
		schema_version: RELEASE_RECORD_SCHEMA_VERSION,
		kind: RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-06T12:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![ReleaseRecordTarget {
			id: "main".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			members: vec![
				"monochange".to_string(),
				"monochange_core".to_string(),
				"monochange_config".to_string(),
			],
		}],
		released_packages: vec![
			"monochange".to_string(),
			"monochange_core".to_string(),
			"monochange_config".to_string(),
		],
		changed_files: vec![
			PathBuf::from("Cargo.lock"),
			PathBuf::from("crates/monochange/Cargo.toml"),
		],
		package_publications: Vec::new(),
		updated_changelogs: vec![PathBuf::from("crates/monochange/CHANGELOG.md")],
		deleted_changesets: vec![PathBuf::from(".changeset/032-step-outputs.md")],
		provider: Some(ReleaseRecordProvider {
			kind: SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

#[test]
fn render_release_record_block_rejects_unsupported_kind() {
	let mut record = sample_release_record();
	record.kind = "monochange.otherRecord".to_string();

	let error = crate::render_release_record_block(&record)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported kind render error"));
	assert!(matches!(
		error,
		ReleaseRecordError::UnsupportedKind(kind) if kind == "monochange.otherRecord"
	));
}

#[test]
fn render_release_record_block_rejects_unsupported_schema_version() {
	let mut record = sample_release_record();
	record.schema_version = 2;

	let error = crate::render_release_record_block(&record)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported schema render error"));
	assert!(matches!(
		error,
		ReleaseRecordError::UnsupportedSchemaVersion(2)
	));
}

#[test]
fn parse_release_record_block_rejects_missing_end_marker() {
	let malformed =
		format!("{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n```json\n{{}}\n```");
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected missing end marker error"));
	assert!(matches!(error, ReleaseRecordError::MissingEndMarker));
}

#[test]
fn parse_release_record_block_rejects_missing_kind() {
	let missing_kind = format!(
		r#"{RELEASE_RECORD_HEADING}

{RELEASE_RECORD_START_MARKER}
```json
{{
  "schemaVersion": 1,
  "createdAt": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": []
}}
```
{RELEASE_RECORD_END_MARKER}"#
	);
	let error = crate::parse_release_record_block(&missing_kind)
		.err()
		.unwrap_or_else(|| panic!("expected missing kind error"));
	assert!(matches!(error, ReleaseRecordError::MissingKind));
}

#[test]
fn parse_release_record_block_rejects_missing_schema_version() {
	let missing_schema = format!(
		r#"{RELEASE_RECORD_HEADING}

{RELEASE_RECORD_START_MARKER}
```json
{{
  "kind": "{RELEASE_RECORD_KIND}",
  "createdAt": "2026-04-06T12:00:00Z",
  "command": "release-pr",
  "releaseTargets": [],
  "releasedPackages": [],
  "changedFiles": []
}}
```
{RELEASE_RECORD_END_MARKER}"#
	);
	let error = crate::parse_release_record_block(&missing_schema)
		.err()
		.unwrap_or_else(|| panic!("expected missing schema error"));
	assert!(matches!(error, ReleaseRecordError::MissingSchemaVersion));
}

#[test]
fn parse_release_record_block_rejects_end_marker_before_start_marker() {
	let malformed = format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_END_MARKER}\n{RELEASE_RECORD_START_MARKER}\n```json\n{{}}\n```"
	);
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected end-before-start error"));
	assert!(matches!(error, ReleaseRecordError::MissingEndMarker));
}

#[test]
fn parse_release_record_block_rejects_trailing_non_empty_lines_after_json_block() {
	let malformed = format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n```json\n{{}}\n```\nextra\n{RELEASE_RECORD_END_MARKER}"
	);
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected trailing-line error"));
	assert!(matches!(error, ReleaseRecordError::MissingJsonBlock));
}

#[test]
fn parse_release_record_block_rejects_empty_json_payload() {
	let malformed = format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n```json\n\n```\n{RELEASE_RECORD_END_MARKER}"
	);
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected empty-json error"));
	assert!(matches!(error, ReleaseRecordError::MissingJsonBlock));
}

#[test]
fn parse_release_record_block_rejects_missing_closing_json_fence() {
	let malformed = format!(
		"{RELEASE_RECORD_HEADING}\n\n{RELEASE_RECORD_START_MARKER}\n```json\n{{}}\n{RELEASE_RECORD_END_MARKER}"
	);
	let error = crate::parse_release_record_block(&malformed)
		.err()
		.unwrap_or_else(|| panic!("expected missing closing fence error"));
	assert!(matches!(error, ReleaseRecordError::MissingJsonBlock));
}

#[test]
fn release_record_discovery_serializes_with_camel_case_keys() {
	let discovery = ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234567890".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 0,
		record: sample_release_record(),
	};
	let value = serde_json::to_value(&discovery)
		.unwrap_or_else(|error| panic!("serialize release record discovery: {error}"));
	let input_ref = value
		.get("inputRef")
		.unwrap_or_else(|| panic!("expected inputRef"));
	assert_eq!(input_ref, "v1.2.3");
	let resolved_commit = value
		.get("resolvedCommit")
		.unwrap_or_else(|| panic!("expected resolvedCommit"));
	assert_eq!(resolved_commit, "abc1234567890");
	let record_commit = value
		.get("recordCommit")
		.unwrap_or_else(|| panic!("expected recordCommit"));
	assert_eq!(record_commit, "abc1234567890");
	let distance = value
		.get("distance")
		.unwrap_or_else(|| panic!("expected distance"));
	assert_eq!(distance, 0);
	let record = value
		.get("record")
		.and_then(serde_json::Value::as_object)
		.unwrap_or_else(|| panic!("expected record object"));
	assert_eq!(
		record
			.get("kind")
			.unwrap_or_else(|| panic!("expected record.kind")),
		RELEASE_RECORD_KIND
	);
}

#[test]
fn release_record_tag_helpers_deduplicate_tags() {
	let mut record = sample_release_record();
	record.release_targets.push(ReleaseRecordTarget {
		id: "duplicate".to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "1.2.3".to_string(),
		version_format: VersionFormat::Primary,
		tag: true,
		release: true,
		tag_name: "v1.2.3".to_string(),
		members: Vec::new(),
	});

	assert_eq!(crate::release_record_tag_names(&record), vec!["v1.2.3"]);
	assert_eq!(
		crate::release_record_release_tag_names(&record),
		vec!["v1.2.3"]
	);
}

#[test]
fn retarget_plan_and_result_serialize_with_camel_case_keys() {
	let tag_result = RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Planned,
		message: None,
	};
	let provider_result = RetargetProviderResult {
		provider: SourceProvider::GitHub,
		tag_name: "v1.2.3".to_string(),
		target_commit: "def5678".to_string(),
		operation: RetargetProviderOperation::Planned,
		url: Some("https://example.com/releases/1".to_string()),
		message: None,
	};
	let plan = RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: vec![tag_result.clone()],
		provider_updates: vec![provider_result.clone()],
		sync_provider: true,
		dry_run: true,
	};
	let result = RetargetResult {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		force: false,
		git_tag_results: vec![tag_result],
		provider_results: vec![provider_result],
		sync_provider: true,
		dry_run: false,
	};

	let plan_value =
		serde_json::to_value(&plan).unwrap_or_else(|error| panic!("serialize plan: {error}"));
	assert_eq!(
		plan_value
			.get("recordCommit")
			.unwrap_or_else(|| panic!("expected recordCommit")),
		"abc1234"
	);
	assert_eq!(
		plan_value
			.get("isDescendant")
			.unwrap_or_else(|| panic!("expected isDescendant")),
		true
	);
	assert_eq!(
		plan_value
			.pointer("/gitTagUpdates/0/operation")
			.unwrap_or_else(|| panic!("expected gitTagUpdates[0].operation")),
		"planned"
	);
	assert_eq!(
		plan_value
			.pointer("/providerUpdates/0/operation")
			.unwrap_or_else(|| panic!("expected providerUpdates[0].operation")),
		"planned"
	);

	let result_value =
		serde_json::to_value(&result).unwrap_or_else(|error| panic!("serialize result: {error}"));
	assert_eq!(
		result_value
			.pointer("/gitTagResults/0/operation")
			.unwrap_or_else(|| panic!("expected gitTagResults[0].operation")),
		"planned"
	);
	assert_eq!(
		result_value
			.pointer("/providerResults/0/operation")
			.unwrap_or_else(|| panic!("expected providerResults[0].operation")),
		"planned"
	);
}

#[test]
fn update_json_manifest_text_updates_arbitrary_nested_fields() {
	let contents = r#"{
  "name": "tool",
  "version": "1.0.0",
  "workspace": {
    "metadata": {
      "bin": {
        "monochange": {
          "version": "1.0.0"
        }
      }
    }
  }
}
"#;
	let updated = crate::update_json_manifest_text(
		contents,
		Some("2.0.0"),
		&["workspace.metadata.bin.monochange.version"],
		&BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("update json manifest: {error}"));

	assert!(updated.contains("\"version\": \"2.0.0\""));
	assert!(updated.contains("\"monochange\": {\n          \"version\": \"2.0.0\""));
}

#[test]
fn update_json_manifest_text_updates_nested_object_fields_and_ignores_invalid_paths() {
	let contents = r#"{
  "version": "1.0.0",
  "workspace": {
    "metadata": {
      "bin": {
        "monochange": {
          "version": "1.0.0"
        },
        "dependencies": {
          "core": "^1.0.0"
        }
      }
    }
  }
}
"#;
	let updated = crate::update_json_manifest_text(
		contents,
		Some("2.0.0"),
		&[
			"",
			"workspace.version.major",
			"workspace.metadata.bin.dependencies",
			"workspace.metadata.bin.monochange.version",
			"workspace.metadata.bin.monochange.version.major",
			"workspace.metadata.bin.missing.version",
		],
		&BTreeMap::from([("core".to_string(), "^2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update nested json manifest: {error}"));

	assert!(updated.contains("\"version\": \"2.0.0\""));
	assert!(updated.contains("\"core\": \"^2.0.0\""));
	assert!(!updated.contains("\"major\""));
	assert!(!updated.contains("\"missing\""));
}

#[test]
fn update_json_manifest_text_preserves_existing_formatting() {
	let contents = r#"{
  // keep comment
  "name": "tool",
  "version": "1.0.0",
  "imports": {
    "core": "^1.0.0"
  },
  "dependencies": { "left-pad": "^1.0.0" }
}
"#;
	let updated = crate::update_json_manifest_text(
		contents,
		Some("2.0.0"),
		&["imports"],
		&BTreeMap::from([("core".to_string(), "^2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update json manifest: {error}"));

	assert!(updated.contains("// keep comment"));
	assert!(updated.contains("\"version\": \"2.0.0\""));
	assert!(updated.contains("\"core\": \"^2.0.0\""));
	assert!(updated.contains("\"left-pad\": \"^1.0.0\""));
	assert!(updated.contains("  \"dependencies\": { \"left-pad\": \"^1.0.0\" }"));
}

#[test]
fn update_json_manifest_text_ignores_missing_or_non_object_sections() {
	let contents = r#"{
  "version": "1.0.0",
  "dependencies": ["core"],
  "imports": {
    "core": "^1.0.0"
  }
}
"#;
	let updated = crate::update_json_manifest_text(
		contents,
		None,
		&["dependencies", "imports"],
		&BTreeMap::from([("core".to_string(), "^2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update json manifest: {error}"));

	assert!(updated.contains("\"dependencies\": [\"core\"]"));
	assert!(updated.contains("\"core\": \"^2.0.0\""));
}

#[test]
fn strip_json_comments_removes_comments_but_preserves_string_literals() {
	let stripped = crate::strip_json_comments(
		r#"{
  // comment
  "text": "https://example.com//still-string",
  "escaped": "quote: \" // still string",
  /* block */
  "value": 1
}
"#,
	);
	assert!(!stripped.contains("// comment"));
	assert!(!stripped.contains("/* block */"));
	assert!(stripped.contains("https://example.com//still-string"));
	assert!(stripped.contains("quote: \\\" // still string"));
}

#[test]
fn json_helper_functions_cover_error_paths() {
	let range_error = crate::apply_json_replacements(
		"{}",
		vec![(crate::JsonSpan { start: 10, end: 11 }, "\"x\"".to_string())],
	)
	.err()
	.unwrap_or_else(|| panic!("expected range error"));
	assert!(range_error.to_string().contains("out of bounds"));

	let root_error = crate::json_root_object_start("[]")
		.err()
		.unwrap_or_else(|| panic!("expected root error"));
	assert!(root_error.to_string().contains("expected JSON object"));

	let locate_error = crate::find_json_object_field_value_span("[]", 0, "name")
		.err()
		.unwrap_or_else(|| panic!("expected locate error"));
	assert!(
		locate_error
			.to_string()
			.contains("expected JSON object when locating field")
	);

	for (contents, key) in [
		("{1:2}", "a"),
		("{\"a\" 1}", "a"),
		("{\"a\":1 !}", "missing"),
		("{\"a\":1", "missing"),
	] {
		assert!(
			crate::find_json_object_field_value_span(contents, 0, key).is_err(),
			"contents: {contents}"
		);
	}

	assert!(crate::skip_json_value("", 0).is_err());
	assert!(crate::skip_json_array("[1 !]", 0).is_err());
	assert!(crate::skip_json_array("[1", 0).is_err());
	assert!(crate::skip_json_array("[", 0).is_err());
	assert!(crate::skip_json_object("{\"a\":1 !}", 0).is_err());
	assert!(crate::skip_json_object("{\"a\":1", 0).is_err());
	assert!(crate::skip_json_object("{", 0).is_err());
	assert!(crate::skip_json_object("{1}", 0).is_err());
	assert!(crate::skip_json_object("{\"a\" 1}", 0).is_err());
	assert!(crate::parse_json_string_span("abc", 0).is_err());
	assert!(crate::parse_json_string_span("\"abc", 0).is_err());
	// Truncated escape: backslash at end of input.
	let error = crate::parse_json_string_span("\"abc\\", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for truncated escape"));
	assert!(
		error.to_string().contains("unterminated escape sequence"),
		"expected truncated-escape error, got: {error}"
	);
	// Escaped quote should not close the string.
	assert!(crate::parse_json_string_span("\"abc\\\"", 0).is_err());
	// Double backslash followed by closing quote should work.
	let (span, next) = crate::parse_json_string_span("\"abc\\\\\"", 0)
		.unwrap_or_else(|error| panic!("double-backslash: {error}"));
	assert_eq!(span, crate::JsonSpan { start: 1, end: 6 });
	assert_eq!(next, 7);
	// Valid unicode escape \u0041 (letter A).
	let (span, next) = crate::parse_json_string_span("\"\\u0041\"", 0)
		.unwrap_or_else(|error| panic!("unicode escape: {error}"));
	assert_eq!(span, crate::JsonSpan { start: 1, end: 7 });
	assert_eq!(next, 8);
	// Incomplete unicode escape: fewer than 4 hex digits before input ends.
	let error = crate::parse_json_string_span("\"\\u00", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for incomplete unicode escape"));
	assert!(
		error.to_string().contains("incomplete unicode escape"),
		"got: {error}"
	);
	// Incomplete unicode escape inside a quoted string (quote seen as non-hex).
	let error = crate::parse_json_string_span("\"\\u00\"", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for short unicode escape"));
	assert!(
		error.to_string().contains("invalid unicode escape"),
		"got: {error}"
	);
	// Invalid hex digit in unicode escape.
	let error = crate::parse_json_string_span("\"\\u00ZZ\"", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for invalid hex in unicode escape"));
	assert!(
		error.to_string().contains("invalid unicode escape"),
		"got: {error}"
	);
	// Truncated unicode escape: string ends before 4 hex digits.
	let error = crate::parse_json_string_span("\"\\u00", 0)
		.err()
		.unwrap_or_else(|| panic!("expected error for truncated unicode escape"));
	assert!(
		error.to_string().contains("incomplete unicode escape"),
		"got: {error}"
	);
}

#[test]
fn json_helper_functions_cover_success_paths() {
	let (string_span, next) = crate::parse_json_string_span("\"a\\\"b\"", 0)
		.unwrap_or_else(|error| panic!("parse string span: {error}"));
	assert_eq!(string_span, crate::JsonSpan { start: 1, end: 5 });
	assert_eq!(next, 6);

	assert_eq!(
		crate::skip_json_value("\"text\"", 0)
			.unwrap_or_else(|error| panic!("skip string value: {error}")),
		6
	);
	assert_eq!(
		crate::skip_json_value("{\"a\":1}", 0)
			.unwrap_or_else(|error| panic!("skip object value: {error}")),
		7
	);
	assert_eq!(
		crate::skip_json_value("[1,2]", 0)
			.unwrap_or_else(|error| panic!("skip array value: {error}")),
		5
	);
	assert_eq!(crate::skip_json_primitive("true /* comment */", 0), 4);
	assert_eq!(crate::skip_json_primitive("true//comment", 0), 4);
	assert_eq!(
		crate::skip_json_ws_and_comments(" // comment\n /* block */ {", 0),
		25
	);
	assert_eq!(
		crate::skip_json_object("{}", 0)
			.unwrap_or_else(|error| panic!("skip empty object: {error}")),
		2
	);
	assert_eq!(
		crate::skip_json_object("{\"a\":1,\"b\":2}", 0)
			.unwrap_or_else(|error| panic!("skip object with comma: {error}")),
		13
	);
	assert_eq!(
		crate::skip_json_array("[]", 0).unwrap_or_else(|error| panic!("skip empty array: {error}")),
		2
	);
	assert_eq!(
		crate::find_json_object_field_value_span("{}", 0, "name")
			.unwrap_or_else(|error| panic!("find empty object field: {error}")),
		None
	);
	let field_span = crate::find_json_object_field_value_span(
		r#"{"name":"tool","deps":{"core":"^1.0.0"}}"#,
		0,
		"deps",
	)
	.unwrap_or_else(|error| panic!("find field span: {error}"))
	.unwrap_or_else(|| panic!("expected deps field"));
	assert!(crate::json_span_is_object(
		r#"{"name":"tool","deps":{"core":"^1.0.0"}}"#,
		field_span
	));
	let updated = crate::update_json_manifest_text(
		r#"{"version":1,"imports":{"core":{"path":"./core"}}}"#,
		Some("2.0.0"),
		&["imports"],
		&BTreeMap::from([("core".to_string(), "^2.0.0".to_string())]),
	)
	.unwrap_or_else(|error| panic!("update json manifest with non-string values: {error}"));
	assert_eq!(
		updated,
		r#"{"version":1,"imports":{"core":{"path":"./core"}}}"#
	);
}

#[test]
fn monochange_error_io_source_render_includes_path_and_source() {
	let error = MonochangeError::IoSource {
		path: PathBuf::from("/tmp/missing.toml"),
		source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
	};
	let rendered = error.render();
	assert!(rendered.contains("/tmp/missing.toml"), "got: {rendered}");
	assert!(rendered.contains("not found"), "got: {rendered}");
}

#[test]
fn monochange_error_parse_render_includes_path_and_source() {
	let error = MonochangeError::Parse {
		path: PathBuf::from("Cargo.toml"),
		source: Box::new(std::io::Error::new(
			std::io::ErrorKind::InvalidData,
			"bad format",
		)),
	};
	let rendered = error.render();
	assert!(rendered.contains("Cargo.toml"), "got: {rendered}");
	assert!(rendered.contains("bad format"), "got: {rendered}");
}

#[test]
fn monochange_error_interactive_render_returns_message() {
	let error = MonochangeError::Interactive {
		message: "prompt failed".to_string(),
	};
	assert_eq!(error.render(), "prompt failed");
}

#[test]
fn monochange_error_cancelled_render_returns_cancelled() {
	let error = MonochangeError::Cancelled;
	assert_eq!(error.render(), "cancelled");
}

#[cfg(feature = "http")]
#[test]
fn monochange_error_http_request_render_includes_context_and_source() {
	let client = reqwest::blocking::Client::new();
	let error = MonochangeError::HttpRequest {
		context: "fetching releases".to_string(),
		source: client.get("http://0.0.0.0:1").send().unwrap_err(),
	};
	let rendered = error.render();
	assert!(rendered.contains("fetching releases"), "got: {rendered}");
}
