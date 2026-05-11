use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::collections::VecDeque;
use std::env;
use std::fs;
use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::MockServer;
use monochange_cargo::extract_workspace_package_table;
use monochange_cargo::read_workspace_package_table;
use monochange_cargo::write_cargo_placeholder_manifest;
use monochange_core::DependencyKind;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::ReleaseRecord;
use monochange_core::SourceProvider;
use monochange_core::TrustedPublishingSettings;
use monochange_github::GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN;
use monochange_github::GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL;
use monochange_github::GitHubTrustContext;
use monochange_github::json_value_contains;
use monochange_github::parse_github_workflow_ref;
use monochange_github::resolve_github_job_environment;
use monochange_github::trust_list_contains_context;
use monochange_npm::append_npm_trust_environment_arg;
use monochange_npm::build_npm_trust_command;
use monochange_publish::CommandExecutor;
use monochange_publish::CommandOutput;
use monochange_publish::CommandSpec;
use monochange_publish::PLACEHOLDER_VERSION;
use monochange_publish::ProcessCommandExecutor;
use monochange_publish::RegistryEndpoints;
use monochange_publish::append_publish_dry_run_args;
use monochange_publish::build_npm_placeholder_publish_command;
use monochange_publish::build_npm_release_publish_command;
use monochange_publish::build_placeholder_directory as build_placeholder_directory_with_writers;
use monochange_publish::build_publish_command;
use monochange_publish::crates_io_index_entry_path;
use monochange_publish::crates_io_index_version_exists;
use monochange_publish::default_registry_kind_for_ecosystem;
use monochange_publish::enforce_release_attestation_prerequisites as enforce_release_attestation_prerequisites_impl;
use monochange_publish::ensure_publish_report_succeeded;
use monochange_publish::execute_publish_requests as execute_publish_requests_impl;
use monochange_publish::filter_pending_publish_requests_with_transport;
use monochange_publish::forbidden_npm_token_env_keys;
use monochange_publish::publish_report_json_error;
use monochange_publish::registry_version_exists;
use monochange_publish::render_command;
use monochange_publish::render_command_error;
use monochange_publish::resolve_placeholder_readme;
use monochange_publish::resolve_registry_kind;
use monochange_publish::write_publish_report_artifact;
use monochange_test_helpers::git;
use reqwest::blocking::Client;
use semver::Version;
use serde_json::Value as JsonValue;
use temp_env::with_vars;
use tempfile::TempDir;
use toml::Value as TomlValue;

use super::*;
use crate::tests::TEST_ENV_LOCK;

const NPM_TRUST_DOCS_URL: &str = "https://docs.npmjs.com/cli/v11/commands/npm-trust";
const CRATES_TRUST_DOCS_URL: &str = "https://crates.io/docs/trusted-publishing";
const DART_TRUST_DOCS_URL: &str = "https://dart.dev/tools/pub/automated-publishing";
const JSR_TRUST_DOCS_URL: &str = "https://jsr.io/docs/publishing-packages";
const PYPI_TRUST_DOCS_URL: &str = "https://docs.pypi.org/trusted-publishers/";

fn trust_docs_url(registry: RegistryKind) -> &'static str {
	(if registry == RegistryKind::CratesIo {
		CRATES_TRUST_DOCS_URL
	} else if registry == RegistryKind::PubDev {
		DART_TRUST_DOCS_URL
	} else if registry == RegistryKind::Jsr {
		JSR_TRUST_DOCS_URL
	} else if registry == RegistryKind::Pypi {
		PYPI_TRUST_DOCS_URL
	} else if registry == RegistryKind::GoProxy {
		"https://go.dev/ref/mod#publishing"
	} else {
		NPM_TRUST_DOCS_URL
	}) as _
}

struct FakeExecutor {
	outputs: VecDeque<CommandOutput>,
	commands: Vec<CommandSpec>,
}

impl FakeExecutor {
	fn new(outputs: Vec<CommandOutput>) -> Self {
		Self {
			outputs: VecDeque::from(outputs),
			commands: Vec::new(),
		}
	}
}

impl CommandExecutor for FakeExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		self.commands.push(spec.clone());
		self.outputs
			.pop_front()
			.ok_or_else(|| MonochangeError::Discovery("missing fake command output".to_string()))
	}
}

fn sample_request(registry: RegistryKind) -> PublishRequest {
	PublishRequest {
		package_id: "pkg".to_string(),
		package_name: if registry == RegistryKind::Jsr {
			"@scope/pkg".to_string()
		} else {
			"pkg".to_string()
		},
		ecosystem: if registry == RegistryKind::CratesIo {
			Ecosystem::Cargo
		} else if registry == RegistryKind::Npm {
			Ecosystem::Npm
		} else if registry == RegistryKind::PubDev {
			Ecosystem::Dart
		} else if registry == RegistryKind::Pypi {
			Ecosystem::Python
		} else if registry == RegistryKind::GoProxy {
			Ecosystem::Go
		} else {
			Ecosystem::Deno
		},
		manifest_path: PathBuf::from("/workspace/pkg/manifest"),
		package_root: PathBuf::from("/workspace/pkg"),
		registry,
		package_manager: (registry == RegistryKind::Npm).then(|| "npm".to_string()),
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.2.3".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings {
			enabled: false,
			repository: None,
			workflow: None,
			environment: None,
		},
		attestations: PublishAttestationSettings::default(),
		placeholder_readme: "placeholder".to_string(),
	}
}

fn sample_publish_outcome(package: &str, status: PackagePublishStatus) -> PackagePublishOutcome {
	PackagePublishOutcome {
		package: package.to_string(),
		ecosystem: Ecosystem::Npm,
		registry: RegistryKind::Npm.to_string(),
		version: "1.2.3".to_string(),
		status,
		message: format!("{status:?}"),
		placeholder: false,
		trusted_publishing: disabled_trust_outcome(),
		command: None,
		stdout: None,
		stderr: None,
	}
}

fn sample_source() -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "monochange".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
	}
}

fn sample_prepared_release(
	root: &Path,
	package_publications: Vec<PackagePublicationTarget>,
) -> PreparedRelease {
	PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: root.to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: Vec::new(),
		package_publications,
		version: None,
		group_version: None,
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
	}
}

fn trusted_request(registry: RegistryKind) -> PublishRequest {
	let mut request = sample_request(registry);
	request.trusted_publishing.enabled = true;
	request
}

fn trusted_provenance_request(registry: RegistryKind) -> PublishRequest {
	let mut request = trusted_request(registry);
	request.attestations.require_registry_provenance = true;
	request
}

fn github_oidc_env() -> BTreeMap<String, String> {
	BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		("GITHUB_JOB".to_string(), "release".to_string()),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL.to_string(),
			"https://token.actions.githubusercontent.com".to_string(),
		),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN.to_string(),
			"request-token".to_string(),
		),
	])
}

fn sample_endpoints(base_url: &str) -> RegistryEndpoints {
	RegistryEndpoints {
		npm_registry: base_url.to_string(),
		crates_io_api: base_url.to_string(),
		crates_io_index: base_url.to_string(),
		pub_dev_api: base_url.to_string(),
		jsr_base: base_url.to_string(),
		pypi_api: base_url.to_string(),
		go_proxy: base_url.to_string(),
	}
}

fn with_locked_env_vars<T>(action: impl FnOnce() -> T) -> T {
	let _env_lock = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	action()
}

#[allow(clippy::too_many_arguments)]
fn execute_publish_requests(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
) -> MonochangeResult<PackagePublishReport> {
	execute_publish_requests_impl(
		root,
		source,
		mode,
		dry_run,
		requests,
		client,
		endpoints,
		env_map,
		executor,
		&build_publish_command_builder(),
		&placeholder_manifest_writer_registry(),
		&publish_readiness_registry(),
		&CliPublishTrustHandler,
	)
}

fn enforce_release_attestation_prerequisites(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<()> {
	enforce_release_attestation_prerequisites_impl(
		request,
		env_map,
		&build_publish_command_builder(),
	)
}

fn build_npm_trust_list_command(request: &PublishRequest) -> CommandSpec {
	CommandSpec {
		program: "npm".to_string(),
		args: vec![
			"trust".to_string(),
			"ls".to_string(),
			request.package_name.clone(),
		],
		cwd: PathBuf::new(),
	}
}

fn configure_npm_trusted_publishing(
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	root: &Path,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
) -> MonochangeResult<TrustedPublishingOutcome> {
	let context = resolve_github_trust_context(root, source, &request.trusted_publishing, env_map)?;
	let list_command = build_npm_trust_list_command(request);
	let list_output = executor.run(&list_command)?;
	if trust_list_contains_context(&list_output.stdout, &context) {
		return Ok(TrustedPublishingOutcome {
			status: TrustedPublishingStatus::Configured,
			repository: Some(context.repository),
			workflow: Some(context.workflow),
			environment: context.environment,
			setup_url: Some(manual_setup_url(request)),
			message: "npm trusted publishing already matches the current GitHub workflow"
				.to_string(),
		});
	}

	let trust_command = build_npm_trust_command(request, &context);
	let trust_output = executor.run(&trust_command)?;
	if !trust_output.success {
		return Err(MonochangeError::Discovery(format!(
			"`{}` failed: {}",
			render_command(&trust_command),
			render_command_error(&trust_output)
		)));
	}

	let verify_output = executor.run(&list_command)?;
	if !trust_list_contains_context(&verify_output.stdout, &context) {
		return Err(MonochangeError::Discovery(format!(
			"npm trusted publishing could not be verified for `{}` after running `{}`",
			request.package_name,
			render_command(&trust_command)
		)));
	}

	Ok(TrustedPublishingOutcome {
		status: TrustedPublishingStatus::Configured,
		repository: Some(context.repository),
		workflow: Some(context.workflow),
		environment: context.environment,
		setup_url: Some(manual_setup_url(request)),
		message: "configured npm trusted publishing for the current GitHub workflow".to_string(),
	})
}

fn build_placeholder_directory(
	root: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<TempDir> {
	build_placeholder_directory_with_writers(
		root,
		request,
		source,
		&placeholder_manifest_writer_registry(),
	)
}

fn placeholder_tempdir_error(error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!("failed to create placeholder tempdir: {error}"))
}

fn workflow_root() -> TempDir {
	let root = tempfile::tempdir().expect("tempdir:");
	let workflows = root.path().join(".github/workflows");
	fs::create_dir_all(&workflows).expect("mkdir:");
	fs::write(
		workflows.join("publish.yml"),
		"jobs:\n  release:\n    environment: publisher\n",
	)
	.expect("write workflow:");
	root
}

fn sample_configuration(
	packages: &[(&str, monochange_core::PackageType, bool)],
) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: PathBuf::from("/workspace"),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: packages
			.iter()
			.map(|(id, package_type, enabled)| {
				monochange_core::PackageDefinition {
					id: (*id).to_string(),
					path: PathBuf::from(id),
					package_type: *package_type,
					changelog: None,
					excluded_changelog_types: Vec::new(),
					empty_update_message: None,
					release_title: None,
					changelog_version_title: None,
					versioned_files: Vec::new(),
					ignore_ecosystem_versioned_files: false,
					ignored_paths: Vec::new(),
					additional_paths: Vec::new(),
					tag: true,
					release: true,
					version_format: monochange_core::VersionFormat::Primary,
					publish: monochange_core::PublishSettings {
						enabled: *enabled,
						..monochange_core::PublishSettings::default()
					},
				}
			})
			.collect(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	}
}

fn commit_release_record(root: &Path, publications: Vec<PackagePublicationTarget>) {
	let record = ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION.to_string(),
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-14T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		versions: BTreeMap::from([("pkg".to_string(), "1.2.3".to_string())]),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "pkg".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Package,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: monochange_core::VersionFormat::Primary,
			tag_name: "pkg-v1.2.3".to_string(),
			members: Vec::new(),
		}],
		released_packages: vec!["pkg".to_string()],
		changed_files: vec![PathBuf::from("tracked.txt")],
		package_publications: publications,
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		changesets: Vec::new(),
		changelogs: Vec::new(),
		provider: None,
	};
	let json = serde_json::to_string_pretty(&record).expect("serialize release record");
	let hash = {
		use std::collections::hash_map::DefaultHasher;
		use std::hash::Hasher;
		let mut hasher = DefaultHasher::new();
		for target in &record.release_targets {
			hasher.write(target.id.as_bytes());
			hasher.write(target.version.as_bytes());
		}
		format!("{:016x}", hasher.finish())
	};
	let dir = root.join(".monochange/releases").join(&hash);
	fs::create_dir_all(&dir).expect("create release record dir");
	let record_path = dir.join("release.json");
	fs::write(&record_path, &json).expect("write release record");
	fs::write(root.join("tracked.txt"), "release\n").expect("write tracked release file");
	git(root, &["add", "."]);
	git(
		root,
		&["commit", "--message", "chore(release): prepare release"],
	);
}

#[test]
fn parse_github_workflow_ref_extracts_filename() {
	assert_eq!(
		parse_github_workflow_ref(
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main"
		),
		Some("publish.yml".to_string())
	);
	assert_eq!(parse_github_workflow_ref("bad-value"), None);
}

#[test]
fn resolve_github_job_environment_reads_string_and_mapping_values() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let workflows = tempdir.path().join(".github/workflows");
	fs::create_dir_all(&workflows).expect("mkdir:");
	fs::write(
		workflows.join("publish.yml"),
		r"
jobs:
  release:
    environment: publisher
  docs:
    environment:
      name: docs
",
	)
	.expect("write workflow:");
	let release_env = BTreeMap::from([("GITHUB_JOB".to_string(), "release".to_string())]);
	let docs_env = BTreeMap::from([("GITHUB_JOB".to_string(), "docs".to_string())]);

	assert_eq!(
		resolve_github_job_environment(tempdir.path(), "publish.yml", &release_env),
		Some("publisher".to_string())
	);
	assert_eq!(
		resolve_github_job_environment(tempdir.path(), "publish.yml", &docs_env),
		Some("docs".to_string())
	);
}

#[test]
fn resolve_github_job_environment_returns_none_for_missing_inputs() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let workflows = tempdir.path().join(".github/workflows");
	fs::create_dir_all(&workflows).expect("mkdir:");
	fs::write(workflows.join("invalid.yml"), "jobs: [").expect("write workflow:");
	fs::write(
		workflows.join("missing-env.yml"),
		"jobs:\n  release:\n    runs-on: ubuntu-latest\n",
	)
	.expect("write workflow:");

	assert_eq!(
		resolve_github_job_environment(tempdir.path(), "publish.yml", &BTreeMap::new()),
		None
	);
	assert_eq!(
		resolve_github_job_environment(
			tempdir.path(),
			"missing.yml",
			&BTreeMap::from([("GITHUB_JOB".to_string(), "release".to_string())]),
		),
		None
	);
	assert_eq!(
		resolve_github_job_environment(
			tempdir.path(),
			"invalid.yml",
			&BTreeMap::from([("GITHUB_JOB".to_string(), "release".to_string())]),
		),
		None
	);
	assert_eq!(
		resolve_github_job_environment(
			tempdir.path(),
			"missing-env.yml",
			&BTreeMap::from([("GITHUB_JOB".to_string(), "release".to_string())]),
		),
		None
	);
}

#[test]
fn resolve_github_trust_context_prefers_explicit_settings() {
	let trust = TrustedPublishingSettings {
		enabled: true,
		repository: Some("owner/repo".to_string()),
		workflow: Some("publish.yml".to_string()),
		environment: Some("publisher".to_string()),
	};
	let context = resolve_github_trust_context(Path::new("."), None, &trust, &BTreeMap::new())
		.expect("context:");
	assert_eq!(
		context,
		GitHubTrustContext {
			repository: "owner/repo".to_string(),
			workflow: "publish.yml".to_string(),
			environment: Some("publisher".to_string()),
		}
	);
}

#[test]
fn resolve_github_trust_context_falls_back_to_source_and_environment() {
	let root = workflow_root();
	let context = resolve_github_trust_context(
		root.path(),
		Some(&sample_source()),
		&TrustedPublishingSettings {
			enabled: true,
			repository: None,
			workflow: None,
			environment: None,
		},
		&BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]),
	)
	.expect("context:");
	assert_eq!(context.repository, "monochange/monochange");
	assert_eq!(context.workflow, "publish.yml");
	assert_eq!(context.environment, Some("publisher".to_string()));
}

#[test]
fn resolve_github_trust_context_requires_repository_and_workflow() {
	let missing_repository = resolve_github_trust_context(
		Path::new("."),
		None,
		&TrustedPublishingSettings {
			enabled: true,
			repository: None,
			workflow: Some("publish.yml".to_string()),
			environment: None,
		},
		&BTreeMap::new(),
	)
	.expect_err("expected repository error");
	assert!(
		missing_repository
			.to_string()
			.contains("could not determine the GitHub repository")
	);

	let missing_workflow = resolve_github_trust_context(
		Path::new("."),
		Some(&sample_source()),
		&TrustedPublishingSettings {
			enabled: true,
			repository: None,
			workflow: None,
			environment: None,
		},
		&BTreeMap::new(),
	)
	.expect_err("expected workflow error");
	assert!(
		missing_workflow
			.to_string()
			.contains("could not determine the GitHub workflow")
	);
}

#[test]
fn trust_list_contains_context_supports_json_and_text() {
	let context = GitHubTrustContext {
		repository: "monochange/monochange".to_string(),
		workflow: "publish.yml".to_string(),
		environment: Some("publisher".to_string()),
	};
	assert!(trust_list_contains_context(
		r#"{"publisher":{"repository":"monochange/monochange","workflow":"publish.yml","environment":"publisher"}}"#,
		&context,
	));
	assert!(trust_list_contains_context(
		"repository monochange/monochange workflow publish.yml environment publisher",
		&context,
	));
}

#[test]
fn append_npm_trust_environment_arg_ignores_missing_environment() {
	let mut args = vec!["trust".to_string()];
	append_npm_trust_environment_arg(&mut args, None);
	assert_eq!(args, vec!["trust".to_string()]);
	append_npm_trust_environment_arg(&mut args, Some(&"publisher".to_string()));
	assert_eq!(
		args,
		vec![
			"trust".to_string(),
			"--env".to_string(),
			"publisher".to_string(),
		]
	);
}

#[test]
fn trust_list_contains_context_checks_arrays_and_missing_values() {
	let context = GitHubTrustContext {
		repository: "monochange/monochange".to_string(),
		workflow: "publish.yml".to_string(),
		environment: None,
	};
	assert!(trust_list_contains_context(
		r#"[{"repository":"monochange/monochange"},{"workflow":"publish.yml"}]"#,
		&context,
	));
	assert!(!json_value_contains(&serde_json::json!(false), "publish"));
	assert!(!trust_list_contains_context(
		r#"{"repository":"monochange/monochange"}"#,
		&context
	));
}

#[test]
fn resolve_registry_kind_defaults_and_custom_errors_match_expectations() {
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Cargo).expect("cargo registry:"),
		RegistryKind::CratesIo
	);
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Npm).expect("npm registry:"),
		RegistryKind::Npm
	);
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Deno).expect("jsr registry:"),
		RegistryKind::Jsr
	);
	assert_eq!(
		resolve_registry_kind(None, Ecosystem::Flutter).expect("pub registry:"),
		RegistryKind::PubDev
	);

	let error = resolve_registry_kind(
		Some(&PublishRegistry::Custom("internal".to_string())),
		Ecosystem::Npm,
	)
	.expect_err("expected custom registry error");
	assert!(
		error
			.to_string()
			.contains("does not support custom registry `internal`")
	);
	let unsupported = default_registry_kind_for_ecosystem("ruby")
		.expect_err("expected unsupported ecosystem error");
	assert!(
		unsupported
			.to_string()
			.contains("does not support ecosystem `ruby`")
	);
}

#[test]
fn build_placeholder_requests_skip_missing_or_disabled_packages_and_report_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("write config");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("write package.json");
	let mut configuration =
		crate::load_workspace_configuration(root.path()).expect("configuration");
	let package = PackageRecord {
		id: "pkg".to_string(),
		name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: root.path().join("packages/pkg/package.json"),
		workspace_root: root.path().to_path_buf(),
		current_version: Some(Version::parse("1.0.0").expect("version")),
		publish_state: PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::from([
			("config_id".to_string(), "pkg".to_string()),
			("manager".to_string(), "pnpm".to_string()),
		]),
		declared_dependencies: Vec::new(),
	};

	let mut disabled = configuration.clone();
	disabled.packages[0].publish.enabled = false;
	disabled.packages.push(monochange_core::PackageDefinition {
		id: "missing".to_string(),
		..configuration.packages[0].clone()
	});
	let requests =
		build_placeholder_requests(root.path(), &disabled, &[package.clone()], &BTreeSet::new())
			.expect("requests");
	assert!(requests.is_empty());

	let requests = build_placeholder_requests(
		root.path(),
		&configuration,
		&[package.clone()],
		&BTreeSet::new(),
	)
	.expect("requests");
	assert_eq!(requests[0].package_manager.as_deref(), Some("pnpm"));

	let selected = build_placeholder_requests(
		root.path(),
		&configuration,
		&[package.clone()],
		&BTreeSet::from(["pkg".to_string()]),
	)
	.expect("selected requests");
	assert_eq!(selected.len(), 1);

	configuration.packages[0].publish.registry =
		Some(PublishRegistry::Custom("internal".to_string()));
	let registry_error = build_placeholder_requests(
		root.path(),
		&configuration,
		&[package.clone()],
		&BTreeSet::new(),
	)
	.expect_err("expected registry error");
	assert!(registry_error.to_string().contains("custom registry"));

	let mut missing_readme =
		crate::load_workspace_configuration(root.path()).expect("configuration");
	missing_readme.packages[0].publish.placeholder.readme_file = Some(PathBuf::from("missing.md"));
	let readme_error =
		build_placeholder_requests(root.path(), &missing_readme, &[package], &BTreeSet::new())
			.expect_err("expected readme error");
	assert!(
		readme_error
			.to_string()
			.contains("failed to read placeholder README")
	);
}

#[test]
fn build_publish_command_covers_all_supported_registries() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let npm_placeholder = build_publish_command(
		&sample_request(RegistryKind::Npm),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(
		npm_placeholder.args,
		vec![
			"publish".to_string(),
			tempdir.path().display().to_string(),
			"--access".to_string(),
			"public".to_string(),
		]
	);
	let npm = build_publish_command(
		&sample_request(RegistryKind::Npm),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(npm.program, "npm");
	let pnpm_request = PublishRequest {
		package_manager: Some("pnpm".to_string()),
		..sample_request(RegistryKind::Npm)
	};
	let pnpm_placeholder = build_publish_command(
		&pnpm_request,
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(pnpm_placeholder.program, "pnpm");
	let pnpm = build_publish_command(&pnpm_request, PackagePublishRunMode::Release, None, false);
	assert_eq!(pnpm.program, "pnpm");
	let trusted_pnpm_request = PublishRequest {
		trusted_publishing: TrustedPublishingSettings {
			enabled: true,
			repository: None,
			workflow: None,
			environment: None,
		},
		..pnpm_request
	};
	let trusted_pnpm = build_publish_command(
		&trusted_pnpm_request,
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(trusted_pnpm.program, "npm");
	let cargo_placeholder = build_publish_command(
		&sample_request(RegistryKind::CratesIo),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(cargo_placeholder.program, "cargo");
	assert!(
		cargo_placeholder
			.args
			.contains(&tempdir.path().join("Cargo.toml").display().to_string())
	);
	let cargo = build_publish_command(
		&sample_request(RegistryKind::CratesIo),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(cargo.program, "cargo");
	let dart = build_publish_command(
		&sample_request(RegistryKind::PubDev),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(dart.program, "dart");
	let dart_placeholder = build_publish_command(
		&sample_request(RegistryKind::PubDev),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(dart_placeholder.cwd, tempdir.path());
	let flutter = build_publish_command(
		&PublishRequest {
			ecosystem: Ecosystem::Flutter,
			..sample_request(RegistryKind::PubDev)
		},
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(flutter.program, "flutter");
	let jsr = build_publish_command(
		&sample_request(RegistryKind::Jsr),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(jsr.program, "deno");
	assert_eq!(jsr.cwd, tempdir.path());
	let jsr_release = build_publish_command(
		&sample_request(RegistryKind::Jsr),
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(jsr_release.cwd, PathBuf::from("/workspace/pkg"));
	let pypi_placeholder = build_publish_command(
		&sample_request(RegistryKind::Pypi),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		false,
	);
	assert_eq!(pypi_placeholder.program, "sh");
	assert_eq!(pypi_placeholder.cwd, tempdir.path());
	assert!(render_command(&pypi_placeholder).contains("uv publish --trusted-publishing never"));
	let pypi_release_request = PublishRequest {
		trusted_publishing: TrustedPublishingSettings {
			enabled: true,
			..TrustedPublishingSettings::default()
		},
		..sample_request(RegistryKind::Pypi)
	};
	let pypi_release = build_publish_command(
		&pypi_release_request,
		PackagePublishRunMode::Release,
		None,
		false,
	);
	assert_eq!(pypi_release.cwd, PathBuf::from("/workspace/pkg"));
	assert!(render_command(&pypi_release).contains("uv publish --trusted-publishing always"));

	let go_request = PublishRequest {
		ecosystem: Ecosystem::Go,
		package_name: "api".to_string(),
		package_root: PathBuf::from("/workspace/api"),
		package_metadata: BTreeMap::from([
			(
				"module_path".to_string(),
				"github.com/example/api".to_string(),
			),
			("relative_path".to_string(), "api".to_string()),
		]),
		..sample_request(RegistryKind::GoProxy)
	};
	let go = build_publish_command(&go_request, PackagePublishRunMode::Release, None, false);
	assert_eq!(go.program, "git");
	assert_eq!(go.args, vec!["tag".to_string(), "api/v1.2.3".to_string()]);
}

#[test]
fn go_publish_command_uses_root_tag_when_relative_path_is_current_directory() {
	let request = PublishRequest {
		version: "v2.0.0".to_string(),
		package_metadata: BTreeMap::from([("relative_path".to_string(), ".".to_string())]),
		..sample_request(RegistryKind::GoProxy)
	};

	let command = build_publish_command(&request, PackagePublishRunMode::Release, None, false);

	assert_eq!(command.args, vec!["tag".to_string(), "v2.0.0".to_string()]);
}

#[test]
fn go_publish_command_falls_back_to_package_root_prefix() {
	let cwd = env::current_dir().expect("current dir");
	let request = PublishRequest {
		version: "1.2.3".to_string(),
		package_root: cwd.join("api"),
		package_metadata: BTreeMap::new(),
		..sample_request(RegistryKind::GoProxy)
	};

	let command = build_publish_command(&request, PackagePublishRunMode::Release, None, false);

	assert_eq!(
		command.args,
		vec!["tag".to_string(), "api/v1.2.3".to_string()]
	);
}

#[test]
fn build_placeholder_directory_writes_go_mod_for_go_proxy() {
	let root = tempfile::tempdir().expect("tempdir");
	let request = PublishRequest {
		package_metadata: BTreeMap::from([(
			"module_path".to_string(),
			"github.com/example/repo/api".to_string(),
		)]),
		..sample_request(RegistryKind::GoProxy)
	};

	let placeholder =
		build_placeholder_directory(root.path(), &request, None).expect("go placeholder directory");
	let go_mod = fs::read_to_string(placeholder.path().join("go.mod")).expect("go.mod");

	assert_eq!(go_mod, "module github.com/example/repo/api\n\ngo 1.22\n");
}

#[test]
fn build_placeholder_directory_uses_package_name_when_go_module_metadata_is_missing() {
	let root = tempfile::tempdir().expect("tempdir");
	let request = PublishRequest {
		package_name: "github.com/example/fallback".to_string(),
		package_metadata: BTreeMap::new(),
		..sample_request(RegistryKind::GoProxy)
	};

	let placeholder =
		build_placeholder_directory(root.path(), &request, None).expect("go placeholder directory");
	let go_mod = fs::read_to_string(placeholder.path().join("go.mod")).expect("go.mod");

	assert_eq!(go_mod, "module github.com/example/fallback\n\ngo 1.22\n");
}

#[test]
fn registry_version_exists_returns_false_for_missing_go_proxy_version() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET)
			.path("/github.com/example/repo/@v/v1.2.3.info");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let request = PublishRequest {
		package_metadata: BTreeMap::from([(
			"module_path".to_string(),
			"github.com/example/repo".to_string(),
		)]),
		..sample_request(RegistryKind::GoProxy)
	};

	assert!(!registry_version_exists(&client, &endpoints, &request).expect("missing go version"));
}

#[test]
fn build_publish_command_appends_dry_run_flags_for_supported_registries() {
	let tempdir = tempfile::tempdir().expect("tempdir:");

	let npm = build_publish_command(
		&sample_request(RegistryKind::Npm),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert_eq!(npm.args.last(), Some(&"--dry-run".to_string()));

	let cargo = build_publish_command(
		&sample_request(RegistryKind::CratesIo),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert_eq!(cargo.args.last(), Some(&"--dry-run".to_string()));

	let dart = build_publish_command(
		&sample_request(RegistryKind::PubDev),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert!(dart.args.contains(&"--dry-run".to_string()));
	assert!(!dart.args.contains(&"--force".to_string()));

	let jsr = build_publish_command(
		&sample_request(RegistryKind::Jsr),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert_eq!(jsr.args.last(), Some(&"--dry-run".to_string()));

	let pypi = build_publish_command(
		&sample_request(RegistryKind::Pypi),
		PackagePublishRunMode::Placeholder,
		Some(tempdir.path()),
		true,
	);
	assert!(!render_command(&pypi).contains("--dry-run"));

	let go = build_publish_command(
		&sample_request(RegistryKind::GoProxy),
		PackagePublishRunMode::Release,
		None,
		true,
	);
	assert!(!go.args.contains(&"--dry-run".to_string()));
}

#[test]
fn manual_setup_url_matches_each_registry() {
	assert_eq!(
		manual_setup_url(&sample_request(RegistryKind::Npm)),
		"https://www.npmjs.com/package/pkg/access".to_string()
	);
	assert_eq!(
		manual_setup_url(&sample_request(RegistryKind::CratesIo)),
		"https://crates.io/crates/pkg".to_string()
	);
	assert_eq!(
		manual_setup_url(&sample_request(RegistryKind::PubDev)),
		"https://pub.dev/packages/pkg/admin".to_string()
	);
	assert_eq!(
		manual_setup_url(&sample_request(RegistryKind::Jsr)),
		"https://jsr.io/@scope/pkg".to_string()
	);
	assert_eq!(
		manual_setup_url(&sample_request(RegistryKind::Pypi)),
		"https://pypi.org/manage/project/pkg/settings/publishing/".to_string()
	);
	let go_request = PublishRequest {
		package_name: "github.com/example/pkg".to_string(),
		..sample_request(RegistryKind::GoProxy)
	};
	assert_eq!(
		manual_setup_url(&go_request),
		"https://pkg.go.dev/github.com/example/pkg".to_string()
	);
	assert_eq!(trust_docs_url(RegistryKind::Npm), NPM_TRUST_DOCS_URL);
	assert_eq!(
		trust_docs_url(RegistryKind::CratesIo),
		CRATES_TRUST_DOCS_URL
	);
	assert_eq!(trust_docs_url(RegistryKind::PubDev), DART_TRUST_DOCS_URL);
	assert_eq!(trust_docs_url(RegistryKind::Jsr), JSR_TRUST_DOCS_URL);
	assert_eq!(trust_docs_url(RegistryKind::Pypi), PYPI_TRUST_DOCS_URL);
	assert_eq!(
		trust_docs_url(RegistryKind::GoProxy),
		"https://go.dev/ref/mod#publishing"
	);
}

#[test]
fn resolve_placeholder_readme_prefers_inline_then_file_then_default() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	fs::write(tempdir.path().join("README.md"), "from-file").expect("write readme:");
	assert_eq!(
		resolve_placeholder_readme(tempdir.path(), Some("inline"), None, "pkg").expect("inline:"),
		"inline"
	);
	assert_eq!(
		resolve_placeholder_readme(tempdir.path(), None, Some(Path::new("README.md")), "pkg")
			.expect("file:"),
		"from-file"
	);
	assert!(
		resolve_placeholder_readme(tempdir.path(), None, None, "pkg")
			.expect("default:")
			.contains("placeholder release")
	);
}

#[test]
fn resolve_placeholder_readme_reports_missing_file_errors() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let error =
		resolve_placeholder_readme(tempdir.path(), None, Some(Path::new("missing.md")), "pkg")
			.expect_err("expected missing readme error");
	assert!(
		error
			.to_string()
			.contains("failed to read placeholder README")
	);
}

#[test]
fn registry_version_exists_parses_all_supported_registry_shapes() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": {
				"1.2.3": { "name": "pkg" }
			}
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "num": "1.2.3" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/packages/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "version": "1.2.3" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/@scope/pkg/meta.json");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": { "1.2.3": {} }
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/pkg/json");
		then.status(200).json_body_obj(&serde_json::json!({
			"releases": { "1.2.3": [] }
		}));
	});
	server.mock(|when, then| {
		when.method(GET)
			.path("/github.com/example/!repo/api/@v/v1.2.3.info");
		then.status(200).json_body_obj(&serde_json::json!({
			"Version": "v1.2.3",
			"Time": "2026-04-28T00:00:00Z"
		}));
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};

	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::Npm))
			.expect("npm exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::CratesIo))
			.expect("cargo exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::PubDev))
			.expect("dart exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::Jsr))
			.expect("jsr exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::Pypi))
			.expect("PyPI exists:")
	);
	let go_request = PublishRequest {
		package_metadata: BTreeMap::from([(
			"module_path".to_string(),
			"github.com/example/Repo/api".to_string(),
		)]),
		..sample_request(RegistryKind::GoProxy)
	};
	assert!(registry_version_exists(&client, &endpoints, &go_request).expect("Go exists:"));
}

#[test]
fn registry_version_exists_treats_any_existing_version_as_placeholder_bootstrap() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": {
				"1.0.0": { "name": "pkg" }
			}
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "num": "1.0.0" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/packages/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "version": "1.0.0" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/@scope/pkg/meta.json");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": { "1.0.0": {} }
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/pkg/json");
		then.status(200).json_body_obj(&serde_json::json!({
			"releases": { "1.0.0": [] }
		}));
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let placeholder = |registry| {
		PublishRequest {
			version: PLACEHOLDER_VERSION.to_string(),
			placeholder: true,
			..sample_request(registry)
		}
	};

	assert!(
		registry_version_exists(&client, &endpoints, &placeholder(RegistryKind::Npm))
			.expect("npm placeholder exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &placeholder(RegistryKind::CratesIo))
			.expect("cargo placeholder exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &placeholder(RegistryKind::PubDev))
			.expect("pub.dev placeholder exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &placeholder(RegistryKind::Jsr))
			.expect("jsr placeholder exists:")
	);
	assert!(
		registry_version_exists(&client, &endpoints, &placeholder(RegistryKind::Pypi))
			.expect("PyPI placeholder exists:")
	);
}

#[test]
fn crates_io_index_entry_path_covers_sparse_layout_rules() {
	assert_eq!(crates_io_index_entry_path(""), "");
	assert_eq!(crates_io_index_entry_path("a"), "1/a");
	assert_eq!(crates_io_index_entry_path("ab"), "2/ab");
	assert_eq!(crates_io_index_entry_path("pkg"), "3/p/pkg");
	assert_eq!(crates_io_index_entry_path("Crate"), "cr/at/crate");
}

#[test]
fn registry_version_exists_falls_back_to_crates_io_index_when_api_is_forbidden() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(403);
	});
	server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(200)
			.body("{\"name\":\"pkg\",\"vers\":\"1.2.3\"}\n");
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());

	assert!(
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::CratesIo))
			.expect("crates.io fallback exists:")
	);
}

#[test]
fn registry_version_exists_reports_crates_io_index_fallback_failures() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(403);
	});
	server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(500);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let error =
		registry_version_exists(&client, &endpoints, &sample_request(RegistryKind::CratesIo))
			.expect_err("expected crates.io fallback error");

	assert!(
		error
			.to_string()
			.contains("crates.io lookup failed with http status 403 Forbidden")
	);
	assert!(
		error
			.to_string()
			.contains("crates.io index fallback failed")
	);
}

#[test]
fn crates_io_index_version_exists_handles_missing_invalid_and_nonmatching_entries() {
	let client = Client::builder().build().expect("http client:");

	let missing_server = MockServer::start();
	missing_server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(404);
	});
	assert!(
		!crates_io_index_version_exists(
			&client,
			&sample_endpoints(&missing_server.base_url()),
			&sample_request(RegistryKind::CratesIo),
		)
		.expect("missing index entry:")
	);

	let invalid_server = MockServer::start();
	invalid_server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(200).body("not-json\n");
	});
	let invalid_error = crates_io_index_version_exists(
		&client,
		&sample_endpoints(&invalid_server.base_url()),
		&sample_request(RegistryKind::CratesIo),
	)
	.expect_err("expected index decode error");
	assert!(
		invalid_error
			.to_string()
			.contains("crates.io index decode failed")
	);

	let nonmatching_server = MockServer::start();
	nonmatching_server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(200)
			.body("{\"name\":\"pkg\"}\n{\"name\":\"pkg\",\"vers\":\"9.9.9\"}\n");
	});
	assert!(
		!crates_io_index_version_exists(
			&client,
			&sample_endpoints(&nonmatching_server.base_url()),
			&sample_request(RegistryKind::CratesIo),
		)
		.expect("nonmatching index entry:")
	);
}

#[test]
fn crates_io_index_version_exists_matches_placeholder_or_requested_version() {
	let client = Client::builder().build().expect("http client:");
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/3/p/pkg");
		then.status(200)
			.body("{\"name\":\"pkg\",\"vers\":\"1.0.0\"}\n{\"name\":\"pkg\",\"vers\":\"1.2.3\"}\n");
	});
	let endpoints = sample_endpoints(&server.base_url());

	assert!(
		crates_io_index_version_exists(
			&client,
			&endpoints,
			&PublishRequest {
				placeholder: true,
				version: PLACEHOLDER_VERSION.to_string(),
				..sample_request(RegistryKind::CratesIo)
			},
		)
		.expect("placeholder index entry:")
	);
	assert!(
		crates_io_index_version_exists(
			&client,
			&endpoints,
			&sample_request(RegistryKind::CratesIo),
		)
		.expect("matching index entry:")
	);
}

#[test]
fn registry_version_exists_returns_false_for_missing_packages() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/missing");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/missing/json");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};
	let request = sample_request(RegistryKind::Npm);
	let request = PublishRequest {
		package_name: "missing".to_string(),
		..request
	};
	assert!(!registry_version_exists(&client, &endpoints, &request).expect("missing:"));
	let pypi_request = PublishRequest {
		package_name: "missing".to_string(),
		..sample_request(RegistryKind::Pypi)
	};
	assert!(!registry_version_exists(&client, &endpoints, &pypi_request).expect("PyPI missing:"));
}

#[test]
fn registry_version_exists_handles_missing_and_invalid_registry_responses() {
	let server = MockServer::start();
	for path in [
		"/crates/missing",
		"/packages/missing",
		"/@scope/missing/meta.json",
	] {
		server.mock(|when, then| {
			when.method(GET).path(path);
			then.status(404);
		});
	}
	for path in [
		"/bad-json",
		"/crates/bad-json",
		"/packages/bad-json",
		"/@scope/bad-json/meta.json",
	] {
		server.mock(|when, then| {
			when.method(GET).path(path);
			then.status(200).body("not-json");
		});
	}
	server.mock(|when, then| {
		when.method(GET).path("/boom");
		then.status(500).body("boom");
	});

	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());

	assert!(
		!registry_version_exists(
			&client,
			&endpoints,
			&PublishRequest {
				package_name: "missing".to_string(),
				..sample_request(RegistryKind::CratesIo)
			},
		)
		.expect("crates missing:")
	);
	assert!(
		!registry_version_exists(
			&client,
			&endpoints,
			&PublishRequest {
				package_name: "missing".to_string(),
				..sample_request(RegistryKind::PubDev)
			},
		)
		.expect("pub missing:")
	);
	assert!(
		!registry_version_exists(
			&client,
			&endpoints,
			&PublishRequest {
				package_name: "@scope/missing".to_string(),
				..sample_request(RegistryKind::Jsr)
			},
		)
		.expect("jsr missing:")
	);

	let decode_error = registry_version_exists(
		&client,
		&endpoints,
		&PublishRequest {
			package_name: "bad-json".to_string(),
			..sample_request(RegistryKind::Npm)
		},
	)
	.expect_err("expected npm decode error");
	assert!(
		decode_error
			.to_string()
			.contains("npm registry decode failed")
	);

	let http_error = registry_version_exists(
		&client,
		&endpoints,
		&PublishRequest {
			package_name: "boom".to_string(),
			..sample_request(RegistryKind::Npm)
		},
	)
	.expect_err("expected npm http error");
	assert!(
		http_error
			.to_string()
			.contains("npm registry lookup failed")
	);
}

#[test]
fn write_cargo_placeholder_manifest_requires_license_metadata() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let package_root = tempdir.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(
		&manifest_path,
		"[package]\nname = \"pkg\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.expect("write manifest:");
	let placeholder_dir = tempfile::tempdir().expect("tempdir:");
	let request = PublishRequest {
		manifest_path,
		package_root,
		..sample_request(RegistryKind::CratesIo)
	};
	let error = write_cargo_placeholder_manifest(
		placeholder_dir.path(),
		&request,
		tempdir.path(),
		Some(&sample_source()),
	)
	.expect_err("expected cargo placeholder error");
	let text = error.to_string();
	assert!(text.contains("license"), "{text}");
}

#[test]
fn write_cargo_placeholder_manifest_reads_workspace_license_metadata() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	fs::write(
		tempdir.path().join("Cargo.toml"),
		concat!(
			"[workspace]\n",
			"members = [\"pkg\"]\n\n",
			"[workspace.package]\n",
			"license = \"Unlicense\"\n",
		),
	)
	.expect("write workspace manifest:");
	let package_root = tempdir.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(
		&manifest_path,
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license = { workspace = true }\n",
		),
	)
	.expect("write manifest:");
	let placeholder_dir = tempfile::tempdir().expect("tempdir:");
	let request = PublishRequest {
		manifest_path,
		package_root,
		..sample_request(RegistryKind::CratesIo)
	};

	write_cargo_placeholder_manifest(
		placeholder_dir.path(),
		&request,
		tempdir.path(),
		Some(&sample_source()),
	)
	.expect("cargo placeholder:");

	let placeholder_manifest = fs::read_to_string(placeholder_dir.path().join("Cargo.toml"))
		.expect("read placeholder manifest:");
	assert!(placeholder_manifest.contains("license = \"Unlicense\""));
}

#[test]
fn write_cargo_placeholder_manifest_copies_license_file_and_repository() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let package_root = tempdir.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	fs::write(package_root.join("LICENSE.md"), "MIT").expect("write license:");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(
		&manifest_path,
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"edition = \"2024\"\n",
			"description = \"crate placeholder\"\n",
			"license-file = \"LICENSE.md\"\n",
		),
	)
	.expect("write manifest:");
	let placeholder_dir = tempfile::tempdir().expect("tempdir:");
	let request = PublishRequest {
		manifest_path,
		package_root,
		..sample_request(RegistryKind::CratesIo)
	};

	write_cargo_placeholder_manifest(
		placeholder_dir.path(),
		&request,
		tempdir.path(),
		Some(&sample_source()),
	)
	.expect("cargo placeholder:");

	let placeholder_manifest = fs::read_to_string(placeholder_dir.path().join("Cargo.toml"))
		.expect("read placeholder manifest:");
	assert!(placeholder_manifest.contains("edition = \"2024\""));
	assert!(placeholder_manifest.contains("license-file = \"LICENSE\""));
	assert!(
		placeholder_manifest.contains("repository = \"https://github.com/monochange/monochange\"")
	);
	assert_eq!(
		fs::read_to_string(placeholder_dir.path().join("LICENSE"))
			.expect("read placeholder license:"),
		"MIT"
	);
	assert!(placeholder_dir.path().join("src/lib.rs").is_file());
}

#[test]
fn write_cargo_placeholder_manifest_reports_manifest_io_parse_and_copy_failures() {
	let root = tempfile::tempdir().expect("tempdir");
	let dir = tempfile::tempdir().expect("tempdir");

	let missing_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: root.path().join("missing/Cargo.toml"),
			package_root: PathBuf::from("missing"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected missing manifest error");
	assert!(
		missing_error
			.to_string()
			.contains("failed to read Cargo manifest")
	);

	let invalid_manifest = root.path().join("invalid/Cargo.toml");
	fs::create_dir_all(invalid_manifest.parent().expect("parent")).expect("mkdir");
	fs::write(&invalid_manifest, "[package").expect("write invalid manifest");
	let parse_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: invalid_manifest,
			package_root: PathBuf::from("invalid"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected parse error");
	assert!(parse_error.to_string().contains("failed to parse"));

	let missing_package_manifest = root.path().join("no-package/Cargo.toml");
	fs::create_dir_all(missing_package_manifest.parent().expect("parent")).expect("mkdir");
	fs::write(&missing_package_manifest, "[workspace]\nmembers = []\n")
		.expect("write workspace manifest");
	let missing_package_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: missing_package_manifest,
			package_root: PathBuf::from("no-package"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected missing package error");
	assert!(
		missing_package_error
			.to_string()
			.contains("is missing [package]")
	);

	let copy_manifest = root.path().join("copy/Cargo.toml");
	fs::create_dir_all(copy_manifest.parent().expect("parent")).expect("mkdir");
	fs::write(
		&copy_manifest,
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license-file = \"LICENSE.md\"\n",
		),
	)
	.expect("write manifest");
	let copy_error = write_cargo_placeholder_manifest(
		dir.path(),
		&PublishRequest {
			manifest_path: copy_manifest,
			package_root: PathBuf::from("copy"),
			..sample_request(RegistryKind::CratesIo)
		},
		root.path(),
		None,
	)
	.expect_err("expected copy error");
	assert!(
		copy_error
			.to_string()
			.contains("failed to copy placeholder license file")
	);
}

#[test]
fn placeholder_manifest_writers_report_write_failures() {
	let tempdir = tempfile::tempdir().expect("tempdir");
	let file_root = tempdir.path().join("not-a-dir");
	fs::write(&file_root, "file").expect("write file root");

	let npm_error =
		write_npm_placeholder_manifest(&file_root, &sample_request(RegistryKind::Npm), None)
			.expect_err("expected npm write error");
	assert!(
		npm_error
			.to_string()
			.contains("failed to write placeholder package.json")
	);

	let dart_error =
		write_dart_placeholder_manifest(&file_root, &sample_request(RegistryKind::PubDev), None)
			.expect_err("expected dart write error");
	assert!(
		dart_error
			.to_string()
			.contains("failed to write placeholder pubspec.yaml")
	);

	let jsr_error =
		write_jsr_placeholder_manifest(&file_root, &sample_request(RegistryKind::Jsr), None)
			.expect_err("expected jsr write error");
	assert!(
		jsr_error
			.to_string()
			.contains("failed to write placeholder deno.json")
	);
}

#[test]
fn cargo_and_jsr_placeholder_manifests_report_directory_write_failures() {
	let tempdir = tempfile::tempdir().expect("tempdir");
	let package_root = tempdir.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(
		&manifest_path,
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license = \"MIT\"\n",
		),
	)
	.expect("write manifest");
	let request = PublishRequest {
		manifest_path,
		package_root,
		..sample_request(RegistryKind::CratesIo)
	};

	let file_root = tempdir.path().join("file-root");
	fs::write(&file_root, "file").expect("write file root");
	let mkdir_error = write_cargo_placeholder_manifest(&file_root, &request, tempdir.path(), None)
		.expect_err("expected src dir error");
	assert!(
		mkdir_error
			.to_string()
			.contains("failed to create placeholder src directory")
	);

	let src_file_root = tempdir.path().join("src-file-root");
	fs::create_dir_all(src_file_root.join("src/lib.rs")).expect("create lib.rs directory");
	let src_write_error =
		write_cargo_placeholder_manifest(&src_file_root, &request, tempdir.path(), None)
			.expect_err("expected src file write error");
	assert!(
		src_write_error
			.to_string()
			.contains("failed to write placeholder src/lib.rs")
	);

	let manifest_file_root = tempdir.path().join("manifest-file-root");
	fs::create_dir_all(manifest_file_root.join("src")).expect("create src");
	fs::create_dir_all(manifest_file_root.join("Cargo.toml")).expect("create Cargo.toml directory");
	let manifest_write_error =
		write_cargo_placeholder_manifest(&manifest_file_root, &request, tempdir.path(), None)
			.expect_err("expected cargo manifest write error");
	assert!(
		manifest_write_error
			.to_string()
			.contains("failed to write placeholder Cargo.toml")
	);

	let jsr_mod_root = tempdir.path().join("jsr-mod-root");
	fs::create_dir_all(&jsr_mod_root).expect("mkdir jsr root");
	fs::create_dir_all(jsr_mod_root.join("mod.ts")).expect("create mod.ts directory");
	let jsr_mod_error =
		write_jsr_placeholder_manifest(&jsr_mod_root, &sample_request(RegistryKind::Jsr), None)
			.expect_err("expected mod.ts write error");
	assert!(
		jsr_mod_error
			.to_string()
			.contains("failed to write placeholder mod.ts")
	);
}

fn sample_npm_package_with_dependencies(
	id: &str,
	name: &str,
	declared_dependencies: Vec<monochange_core::PackageDependency>,
) -> PackageRecord {
	PackageRecord {
		id: format!("npm:packages/{id}/package.json"),
		name: name.to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from(format!("/workspace/packages/{id}/package.json")),
		workspace_root: PathBuf::from("/workspace"),
		current_version: Some(Version::parse("1.0.0").expect("version")),
		publish_state: PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::from([
			("config_id".to_string(), id.to_string()),
			("manager".to_string(), "pnpm".to_string()),
		]),
		declared_dependencies,
	}
}

fn sample_npm_dependency(name: &str, kind: DependencyKind) -> monochange_core::PackageDependency {
	monochange_core::PackageDependency {
		name: name.to_string(),
		kind,
		version_constraint: Some("workspace:*".to_string()),
		optional: false,
	}
}

fn sample_npm_publication(package: &str) -> PackagePublicationTarget {
	PackagePublicationTarget {
		package: package.to_string(),
		ecosystem: Ecosystem::Npm,
		registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
		version: "1.2.3".to_string(),
		mode: PublishMode::Builtin,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
	}
}

#[test]
fn build_release_requests_orders_publish_relevant_dependencies_before_dependents() {
	let configuration = sample_configuration(&[
		("app", monochange_core::PackageType::Npm, true),
		("core", monochange_core::PackageType::Npm, true),
		("utils", monochange_core::PackageType::Npm, true),
	]);
	let packages = vec![
		sample_npm_package_with_dependencies(
			"app",
			"app",
			vec![
				sample_npm_dependency("core", DependencyKind::Runtime),
				sample_npm_dependency("utils", DependencyKind::Build),
			],
		),
		sample_npm_package_with_dependencies(
			"utils",
			"utils",
			vec![sample_npm_dependency("core", DependencyKind::Peer)],
		),
		sample_npm_package_with_dependencies("core", "core", Vec::new()),
	];
	let publications = vec![
		sample_npm_publication("app"),
		sample_npm_publication("utils"),
		sample_npm_publication("core"),
	];

	let requests =
		build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
			.expect("requests");
	let ordered_package_ids = requests
		.iter()
		.map(|request| request.package_id.as_str())
		.collect::<Vec<_>>();

	assert_eq!(ordered_package_ids, vec!["core", "utils", "app"]);
}

#[test]
fn build_release_requests_orders_large_interdependent_package_set_before_batching() {
	let package_ids = (1..=50)
		.map(|index| format!("pkg-{index:02}"))
		.collect::<Vec<_>>();
	let definitions = package_ids
		.iter()
		.map(|package_id| (package_id.as_str(), monochange_core::PackageType::Npm, true))
		.collect::<Vec<_>>();
	let configuration = sample_configuration(&definitions);
	let packages = package_ids
		.iter()
		.enumerate()
		.rev()
		.map(|(index, package_id)| {
			let dependencies = (0..index)
				.rev()
				.take(3)
				.map(|dependency_index| {
					sample_npm_dependency(&package_ids[dependency_index], DependencyKind::Runtime)
				})
				.collect::<Vec<_>>();
			sample_npm_package_with_dependencies(package_id, package_id, dependencies)
		})
		.collect::<Vec<_>>();
	let publications = package_ids
		.iter()
		.rev()
		.map(|package_id| sample_npm_publication(package_id))
		.collect::<Vec<_>>();

	let requests =
		build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
			.expect("requests");
	let ordered_package_ids = requests
		.iter()
		.map(|request| request.package_id.as_str())
		.collect::<Vec<_>>();

	assert_eq!(ordered_package_ids, package_ids);
}

#[test]
fn build_release_requests_ignores_dependencies_outside_selected_publications() {
	let configuration = sample_configuration(&[
		("app", monochange_core::PackageType::Npm, true),
		("core", monochange_core::PackageType::Npm, true),
	]);
	let packages = vec![
		sample_npm_package_with_dependencies(
			"app",
			"app",
			vec![sample_npm_dependency("core", DependencyKind::Runtime)],
		),
		sample_npm_package_with_dependencies("core", "core", Vec::new()),
	];
	let publications = vec![sample_npm_publication("app")];

	let requests =
		build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
			.expect("dependency outside publication set should not block publishing");

	assert_eq!(requests.len(), 1);
	assert_eq!(requests[0].package_id, "app");
}

#[test]
fn build_release_requests_detects_publish_relevant_dependency_cycles() {
	let configuration = sample_configuration(&[
		("core", monochange_core::PackageType::Npm, true),
		("utils", monochange_core::PackageType::Npm, true),
	]);
	let packages = vec![
		sample_npm_package_with_dependencies(
			"core",
			"core",
			vec![sample_npm_dependency("utils", DependencyKind::Runtime)],
		),
		sample_npm_package_with_dependencies(
			"utils",
			"utils",
			vec![sample_npm_dependency("core", DependencyKind::Workspace)],
		),
	];
	let publications = vec![
		sample_npm_publication("core"),
		sample_npm_publication("utils"),
	];

	let error = build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
		.expect_err("publish-relevant dependency cycles should fail");
	let message = error.to_string();

	assert!(message.contains("cyclic publish dependencies"));
	assert!(message.contains("core -> utils"));
	assert!(message.contains("utils -> core"));
}

#[test]
fn build_release_requests_reports_development_dependency_cycles() {
	let configuration = sample_configuration(&[
		("core", monochange_core::PackageType::Npm, true),
		("utils", monochange_core::PackageType::Npm, true),
	]);
	let packages = vec![
		sample_npm_package_with_dependencies(
			"core",
			"core",
			vec![sample_npm_dependency("utils", DependencyKind::Development)],
		),
		sample_npm_package_with_dependencies(
			"utils",
			"utils",
			vec![sample_npm_dependency("core", DependencyKind::Development)],
		),
	];
	let publications = vec![
		sample_npm_publication("utils"),
		sample_npm_publication("core"),
	];

	let error = build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
		.expect_err("development dependency cycles should fail");
	let message = error.to_string();

	assert!(message.contains("cyclic publish dependencies detected"));
	assert!(message.contains("core -> utils"));
	assert!(message.contains("utils -> core"));
}

#[test]
fn build_release_requests_skips_unknown_publication_targets() {
	let package = PackageRecord {
		id: "npm:packages/pkg/package.json".to_string(),
		name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from("/workspace/packages/pkg/package.json"),
		workspace_root: PathBuf::from("/workspace"),
		current_version: Some(Version::parse("1.0.0").expect("version")),
		publish_state: PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::from([
			("config_id".to_string(), "pkg".to_string()),
			("manager".to_string(), "pnpm".to_string()),
		]),
		declared_dependencies: Vec::new(),
	};
	let publications = vec![
		PackagePublicationTarget {
			package: "missing".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.0.0".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
		PackagePublicationTarget {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
	];

	let configuration = sample_configuration(&[("pkg", monochange_core::PackageType::Npm, true)]);

	let requests = build_release_requests(
		&configuration,
		&[package.clone()],
		&publications,
		&BTreeSet::new(),
	)
	.expect("requests");
	assert_eq!(requests.len(), 1);
	assert_eq!(requests[0].package_id, "pkg");
	assert_eq!(requests[0].package_manager.as_deref(), Some("pnpm"));

	let filtered = build_release_requests(
		&configuration,
		&[package],
		&publications,
		&BTreeSet::from(["missing".to_string()]),
	)
	.expect("filtered requests");
	assert!(filtered.is_empty());
}

#[test]
fn build_release_requests_skips_publication_targets_missing_from_discovery() {
	let configuration = sample_configuration(&[("pkg", monochange_core::PackageType::Npm, true)]);
	let publications = vec![PackagePublicationTarget {
		package: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
		version: "1.2.3".to_string(),
		mode: PublishMode::Builtin,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
	}];

	let requests = build_release_requests(&configuration, &[], &publications, &BTreeSet::new())
		.expect("requests");

	assert!(requests.is_empty());
}

#[test]
fn build_release_requests_skips_disabled_and_private_packages() {
	let configuration = sample_configuration(&[
		("public", monochange_core::PackageType::Npm, true),
		("disabled", monochange_core::PackageType::Npm, false),
		("private", monochange_core::PackageType::Cargo, true),
	]);
	let packages = vec![
		PackageRecord {
			id: "npm:packages/public/package.json".to_string(),
			name: "public".to_string(),
			ecosystem: Ecosystem::Npm,
			manifest_path: PathBuf::from("/workspace/packages/public/package.json"),
			workspace_root: PathBuf::from("/workspace"),
			current_version: Some(Version::parse("1.0.0").expect("version")),
			publish_state: PublishState::Public,
			version_group_id: None,
			metadata: BTreeMap::from([("config_id".to_string(), "public".to_string())]),
			declared_dependencies: Vec::new(),
		},
		PackageRecord {
			id: "npm:packages/disabled/package.json".to_string(),
			name: "disabled".to_string(),
			ecosystem: Ecosystem::Npm,
			manifest_path: PathBuf::from("/workspace/packages/disabled/package.json"),
			workspace_root: PathBuf::from("/workspace"),
			current_version: Some(Version::parse("1.0.0").expect("version")),
			publish_state: PublishState::Public,
			version_group_id: None,
			metadata: BTreeMap::from([("config_id".to_string(), "disabled".to_string())]),
			declared_dependencies: Vec::new(),
		},
		PackageRecord {
			id: "cargo:crates/private/Cargo.toml".to_string(),
			name: "private".to_string(),
			ecosystem: Ecosystem::Cargo,
			manifest_path: PathBuf::from("/workspace/crates/private/Cargo.toml"),
			workspace_root: PathBuf::from("/workspace"),
			current_version: Some(Version::parse("1.0.0").expect("version")),
			publish_state: PublishState::Private,
			version_group_id: None,
			metadata: BTreeMap::from([("config_id".to_string(), "private".to_string())]),
			declared_dependencies: Vec::new(),
		},
	];
	let publications = vec![
		PackagePublicationTarget {
			package: "public".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.0.1".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
		PackagePublicationTarget {
			package: "disabled".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.0.1".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
		PackagePublicationTarget {
			package: "private".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			version: "1.0.1".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
	];

	let requests =
		build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
			.expect("requests");

	assert_eq!(requests.len(), 1);
	assert_eq!(requests[0].package_id, "public");
}

#[test]
fn write_placeholder_directory_builds_npm_jsr_dart_and_python_scaffolds() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let npm = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::Npm),
		Some(&sample_source()),
	)
	.expect("npm placeholder:");
	assert!(npm.path().join("package.json").is_file());

	let dart = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::PubDev),
		Some(&sample_source()),
	)
	.expect("dart placeholder:");
	assert!(dart.path().join("pubspec.yaml").is_file());

	let jsr = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::Jsr),
		Some(&sample_source()),
	)
	.expect("jsr placeholder:");
	assert!(jsr.path().join("deno.json").is_file());

	let python_request = PublishRequest {
		package_name: "Example-Pkg.Name".to_string(),
		..sample_request(RegistryKind::Pypi)
	};
	let python =
		build_placeholder_directory(tempdir.path(), &python_request, Some(&sample_source()))
			.expect("Python placeholder:");
	let pyproject =
		fs::read_to_string(python.path().join("pyproject.toml")).expect("read pyproject.toml");
	assert!(pyproject.contains("name = \"Example-Pkg.Name\""));
	assert!(pyproject.contains("packages = [\"src/example_pkg_name\"]"));
	assert!(
		python
			.path()
			.join("src")
			.join("example_pkg_name")
			.join("__init__.py")
			.is_file()
	);

	let digit_request = PublishRequest {
		package_name: "123-pkg".to_string(),
		..sample_request(RegistryKind::Pypi)
	};
	let digit_python = build_placeholder_directory(tempdir.path(), &digit_request, None)
		.expect("digit Python placeholder:");
	assert!(
		digit_python
			.path()
			.join("src")
			.join("placeholder_123_pkg")
			.join("__init__.py")
			.is_file()
	);
}

#[test]
fn python_placeholder_manifest_writers_report_io_errors() {
	let request = sample_request(RegistryKind::Pypi);
	let tempdir = tempfile::tempdir().expect("tempdir:");
	fs::create_dir(tempdir.path().join("pyproject.toml")).expect("create pyproject dir");
	let error = write_python_placeholder_manifest(tempdir.path(), &request, None)
		.expect_err("pyproject write should fail");
	assert!(
		error
			.to_string()
			.contains("failed to write placeholder pyproject.toml")
	);

	let tempdir = tempfile::tempdir().expect("tempdir:");
	fs::write(tempdir.path().join("src"), "not a directory").expect("write src file");
	let error = write_python_placeholder_manifest(tempdir.path(), &request, None)
		.expect_err("package directory create should fail");
	assert!(
		error
			.to_string()
			.contains("failed to create placeholder Python package")
	);

	let tempdir = tempfile::tempdir().expect("tempdir:");
	let module_dir = tempdir.path().join("src").join("pkg");
	fs::create_dir_all(&module_dir).expect("create module dir");
	fs::create_dir(module_dir.join("__init__.py")).expect("create init dir");
	let error = write_python_placeholder_manifest(tempdir.path(), &request, None)
		.expect_err("module write should fail");
	assert!(
		error
			.to_string()
			.contains("failed to write placeholder Python package module")
	);
}

#[test]
fn placeholder_tempdir_error_renders_stable_message() {
	let error = placeholder_tempdir_error(&std::io::Error::other("disk full"));
	assert_eq!(
		error.to_string(),
		"io error: failed to create placeholder tempdir: disk full"
	);
}

#[test]
fn placeholder_directory_manifests_include_expected_repository_metadata() {
	let tempdir = tempfile::tempdir().expect("tempdir:");

	let npm = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::Npm),
		Some(&sample_source()),
	)
	.expect("npm placeholder:");
	let npm_manifest =
		fs::read_to_string(npm.path().join("package.json")).expect("read package.json:");
	let npm_manifest_json =
		serde_json::from_str::<JsonValue>(&npm_manifest).expect("parse package.json");
	let npm_repository = npm_manifest_json
		.get("repository")
		.and_then(JsonValue::as_str);
	assert_eq!(
		npm_repository,
		Some("https://github.com/monochange/monochange")
	);

	let dart = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::PubDev),
		Some(&sample_source()),
	)
	.expect("dart placeholder:");
	let pubspec = fs::read_to_string(dart.path().join("pubspec.yaml")).expect("read pubspec.yaml:");
	assert!(pubspec.contains("repository: https://github.com/monochange/monochange"));

	let jsr = build_placeholder_directory(
		tempdir.path(),
		&sample_request(RegistryKind::Jsr),
		Some(&sample_source()),
	)
	.expect("jsr placeholder:");
	let deno_manifest = fs::read_to_string(jsr.path().join("deno.json")).expect("read deno.json:");
	let deno_manifest_json =
		serde_json::from_str::<JsonValue>(&deno_manifest).expect("parse deno.json");
	let deno_repository = deno_manifest_json
		.get("repository")
		.and_then(JsonValue::as_str);
	assert_eq!(
		deno_repository,
		Some("https://github.com/monochange/monochange")
	);
}

#[test]
fn planned_and_skip_trust_outcomes_cover_npm_and_manual_flows() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let planned = planned_trust_outcome(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(planned.status, TrustedPublishingStatus::Planned);
	assert_eq!(planned.environment, Some("publisher".to_string()));
	assert_eq!(
		planned.setup_url.as_deref(),
		Some("https://www.npmjs.com/package/pkg/access")
	);

	let skipped = trust_outcome_for_skip(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(skipped.status, TrustedPublishingStatus::Configured);
	assert_eq!(
		skipped.setup_url.as_deref(),
		Some("https://www.npmjs.com/package/pkg/access")
	);

	let manual = planned_trust_outcome(
		&trusted_request(RegistryKind::CratesIo),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(manual.status, TrustedPublishingStatus::ManualActionRequired);
	assert_eq!(manual.repository.as_deref(), Some("monochange/monochange"));
	assert_eq!(manual.workflow.as_deref(), Some("publish.yml"));
	assert_eq!(manual.environment.as_deref(), Some("publisher"));
	assert!(
		manual
			.setup_url
			.expect("expected setup url")
			.contains("crates.io/crates/pkg")
	);
}

#[test]
fn configure_npm_trusted_publishing_skips_when_trust_list_matches_context() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let request = trusted_request(RegistryKind::Npm);
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: "repository monochange/monochange workflow publish.yml environment publisher"
			.to_string(),
		stderr: String::new(),
	}]);

	let outcome = configure_npm_trusted_publishing(
		&request,
		Some(&sample_source()),
		root.path(),
		&env_map,
		&mut executor,
	)
	.expect("configure npm trusted publishing");

	assert_eq!(outcome.status, TrustedPublishingStatus::Configured);
	assert_eq!(
		outcome.message,
		"npm trusted publishing already matches the current GitHub workflow"
	);
	assert_eq!(executor.commands.len(), 1);
	assert_eq!(executor.commands[0].program, "npm");
	assert_eq!(executor.commands[0].args, ["trust", "ls", "pkg"]);
	assert!(executor.commands[0].cwd.as_os_str().is_empty());
}

#[test]
fn configure_npm_trusted_publishing_runs_trust_command_and_verifies_result() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let request = trusted_request(RegistryKind::Npm);
	let mut executor = FakeExecutor::new(vec![
		CommandOutput {
			success: true,
			stdout: String::new(),
			stderr: String::new(),
		},
		CommandOutput {
			success: true,
			stdout: String::new(),
			stderr: String::new(),
		},
		CommandOutput {
			success: true,
			stdout: "repository monochange/monochange workflow publish.yml environment publisher"
				.to_string(),
			stderr: String::new(),
		},
	]);

	let outcome = configure_npm_trusted_publishing(
		&request,
		Some(&sample_source()),
		root.path(),
		&env_map,
		&mut executor,
	)
	.expect("configure npm trusted publishing");

	assert_eq!(outcome.status, TrustedPublishingStatus::Configured);
	assert_eq!(
		outcome.message,
		"configured npm trusted publishing for the current GitHub workflow"
	);
	assert_eq!(executor.commands.len(), 3);
	assert_eq!(executor.commands[0].args, ["trust", "ls", "pkg"]);
	assert_eq!(
		executor.commands[1].args,
		[
			"trust",
			"github",
			"pkg",
			"--file",
			"publish.yml",
			"--repo",
			"monochange/monochange",
			"--yes",
			"--env",
			"publisher",
		]
	);
	assert_eq!(executor.commands[2].args, ["trust", "ls", "pkg"]);
}

#[test]
fn configure_npm_trusted_publishing_reports_trust_command_failure() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let request = trusted_request(RegistryKind::Npm);
	let mut executor = FakeExecutor::new(vec![
		CommandOutput {
			success: true,
			stdout: String::new(),
			stderr: String::new(),
		},
		CommandOutput {
			success: false,
			stdout: String::new(),
			stderr: "permission denied".to_string(),
		},
	]);

	let error = configure_npm_trusted_publishing(
		&request,
		Some(&sample_source()),
		root.path(),
		&env_map,
		&mut executor,
	)
	.expect_err("expected trust command failure");

	let message = error.to_string();
	assert!(message.contains("`npm trust github pkg"));
	assert!(message.contains("permission denied"));
	assert_eq!(executor.commands.len(), 2);
}

#[test]
fn configure_npm_trusted_publishing_reports_failed_verification() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let request = trusted_request(RegistryKind::Npm);
	let mut executor = FakeExecutor::new(vec![
		CommandOutput {
			success: true,
			stdout: String::new(),
			stderr: String::new(),
		},
		CommandOutput {
			success: true,
			stdout: String::new(),
			stderr: String::new(),
		},
		CommandOutput {
			success: true,
			stdout: "repository other/repo workflow publish.yml".to_string(),
			stderr: String::new(),
		},
	]);

	let error = configure_npm_trusted_publishing(
		&request,
		Some(&sample_source()),
		root.path(),
		&env_map,
		&mut executor,
	)
	.expect_err("expected failed verification");

	let message = error.to_string();
	assert!(message.contains("npm trusted publishing could not be verified for `pkg`"));
	assert!(message.contains("npm trust github pkg"));
	assert_eq!(executor.commands.len(), 3);
}

#[test]
fn trust_outcome_for_skip_uses_manual_action_for_non_npm_packages() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let outcome = trust_outcome_for_skip(
		&trusted_request(RegistryKind::CratesIo),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);
	assert_eq!(
		outcome.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert_eq!(outcome.repository.as_deref(), Some("monochange/monochange"));
	assert_eq!(outcome.workflow.as_deref(), Some("publish.yml"));
	assert_eq!(outcome.environment.as_deref(), Some("publisher"));
}

#[test]
fn manual_trust_outcome_preserves_explicit_context_and_registry_setup_url() {
	let mut request = trusted_request(RegistryKind::PubDev);
	request.trusted_publishing.repository = Some("monochange/monochange".to_string());
	request.trusted_publishing.workflow = Some("publish.yml".to_string());
	request.trusted_publishing.environment = Some("pub.dev".to_string());

	let outcome = manual_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());

	assert_eq!(
		outcome.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert_eq!(outcome.repository.as_deref(), Some("monochange/monochange"));
	assert_eq!(outcome.workflow.as_deref(), Some("publish.yml"));
	assert_eq!(outcome.environment.as_deref(), Some("pub.dev"));
	assert_eq!(
		outcome.setup_url.as_deref(),
		Some("https://pub.dev/packages/pkg/admin")
	);
	assert!(
		outcome
			.message
			.contains("configure trusted publishing manually for `pkg`")
	);
	assert!(outcome.message.contains(
		"register repository `monochange/monochange`, workflow `publish.yml`, environment `pub.dev`"
	));
}

#[test]
fn manual_trust_outcome_includes_copyable_npm_trust_command_when_context_is_known() {
	let mut request = trusted_request(RegistryKind::Npm);
	request.trusted_publishing.repository = Some("monochange/monochange".to_string());
	request.trusted_publishing.workflow = Some("publish.yml".to_string());
	request.trusted_publishing.environment = Some("publisher".to_string());

	let outcome = manual_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());

	assert_eq!(
		outcome.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert!(outcome.message.contains(
		"npm trust github pkg --file publish.yml --repo monochange/monochange --yes --env publisher"
	));
}

#[test]
fn planned_trust_outcome_includes_copyable_npm_trust_command_when_context_is_known() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
	]);
	let outcome = planned_trust_outcome(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	);

	assert_eq!(outcome.status, TrustedPublishingStatus::Planned);
	assert!(outcome.message.contains(
		"would configure npm trusted publishing with `npm trust github pkg --file publish.yml --repo monochange/monochange --yes --env publisher`"
	));
}

#[test]
fn manual_trust_outcome_reports_missing_github_context_configuration() {
	let mut request = trusted_request(RegistryKind::Jsr);
	request.trusted_publishing.repository = Some("monochange/monochange".to_string());

	let outcome = manual_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());

	assert_eq!(
		outcome.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert_eq!(outcome.repository.as_deref(), Some("monochange/monochange"));
	assert_eq!(outcome.workflow, None);
	assert!(
		outcome
			.message
			.contains("finish the GitHub context setup first")
	);
	assert!(
		outcome
			.message
			.contains("set `publish.trusted_publishing.workflow`")
	);
}

#[test]
fn release_trust_prerequisites_include_provider_capability_diagnostics() {
	let request = trusted_request(RegistryKind::Npm);
	let error = enforce_release_trust_prerequisites(
		&request,
		Some(&sample_source()),
		Path::new("."),
		&BTreeMap::new(),
	)
	.expect_err("missing GitHub context should block trusted npm release publishing");

	let message = error.to_string();
	assert!(message.contains("local/manual publishing is not allowed"));
	assert!(message.contains("No supported CI provider identity was detected"));
	assert!(message.contains("supported providers: GitHub Actions, GitLab CI/CD"));
}

#[test]
fn manual_trust_outcome_reports_unsupported_ci_provider_capability() {
	let request = trusted_request(RegistryKind::Npm);
	let env_map = BTreeMap::from([
		("CIRCLECI".to_string(), "true".to_string()),
		(
			"CIRCLE_PROJECT_USERNAME".to_string(),
			"monochange".to_string(),
		),
		(
			"CIRCLE_PROJECT_REPONAME".to_string(),
			"monochange".to_string(),
		),
		("CIRCLE_WORKFLOW_ID".to_string(), "workflow".to_string()),
	]);

	let outcome = manual_trust_outcome(&request, None, Path::new("."), &env_map);

	assert_eq!(
		outcome.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert!(
		outcome
			.message
			.contains("CircleCI is not supported for npm trusted publishing")
	);
	assert!(
		outcome
			.message
			.contains("supported providers: GitHub Actions, GitLab CI/CD")
	);
}

#[test]
fn planned_trust_outcome_returns_disabled_when_trust_is_off() {
	let outcome = planned_trust_outcome(
		&sample_request(RegistryKind::Npm),
		None,
		Path::new("."),
		&BTreeMap::new(),
	);
	assert_eq!(outcome.status, TrustedPublishingStatus::Disabled);
}

#[test]
fn planned_and_skip_trust_outcomes_fall_back_to_manual_setup_when_context_missing() {
	let request = trusted_request(RegistryKind::Npm);
	let outcome = planned_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());
	assert_eq!(
		outcome.status,
		TrustedPublishingStatus::ManualActionRequired
	);

	let skipped = trust_outcome_for_skip(&request, None, Path::new("."), &BTreeMap::new());
	assert_eq!(
		skipped.status,
		TrustedPublishingStatus::ManualActionRequired
	);
}

#[test]
fn enforce_release_trust_prerequisites_accepts_configured_github_oidc_contexts() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL.to_string(),
			"https://token.actions.githubusercontent.com".to_string(),
		),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN.to_string(),
			"request-token".to_string(),
		),
	]);

	enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect("expected npm trust prereq success:");

	enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::CratesIo),
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect("expected crates.io trust prereq success:");

	enforce_release_trust_prerequisites(
		&sample_request(RegistryKind::Npm),
		None,
		root.path(),
		&BTreeMap::new(),
	)
	.expect("expected disabled trust success:");

	let mut mismatched_workflow_request = trusted_request(RegistryKind::PubDev);
	mismatched_workflow_request.trusted_publishing.workflow = Some("release.yml".to_string());
	let mismatched_context_error = enforce_release_trust_prerequisites(
		&mismatched_workflow_request,
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect_err("expected mismatched context error");
	assert!(
		mismatched_context_error
			.to_string()
			.contains("expected GitHub workflow `release.yml`, but detected `publish.yml`")
	);
}

#[test]
fn trusted_publishing_without_attestation_policy_does_not_request_npm_provenance() {
	let mut request = trusted_request(RegistryKind::Npm);

	let command = build_npm_release_publish_command(&request);
	assert!(!command.args.contains(&"--provenance".to_string()));

	request.attestations.require_registry_provenance = true;

	let command = build_npm_release_publish_command(&request);
	assert!(command.args.contains(&"--provenance".to_string()));
}

#[test]
fn enforce_release_attestation_prerequisites_accepts_supported_registry_provenance() {
	let env_map = github_oidc_env();

	enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Npm),
		&env_map,
	)
	.expect("expected npm provenance policy success");

	enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Jsr),
		&env_map,
	)
	.expect("expected JSR provenance policy success");
}

#[test]
fn enforce_release_attestation_prerequisites_rejects_disabled_trusted_publishing() {
	let mut request = sample_request(RegistryKind::Npm);
	request.attestations.require_registry_provenance = true;

	let error = enforce_release_attestation_prerequisites(&request, &github_oidc_env())
		.expect_err("disabled trusted publishing should reject provenance policy");

	let message = error.to_string();
	assert!(message.contains("requires registry-native package provenance"));
	assert!(message.contains("trusted publishing is disabled"));
}

#[test]
fn enforce_release_attestation_prerequisites_rejects_local_contexts() {
	let error = enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Npm),
		&BTreeMap::new(),
	)
	.expect_err("local trusted publishing should reject provenance policy");

	let message = error.to_string();
	assert!(message.contains("local or unverifiable"));
	assert!(message.contains("No supported CI provider identity was detected"));
}

#[test]
fn enforce_release_attestation_prerequisites_rejects_unsupported_registry_provenance() {
	let error = enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::CratesIo),
		&github_oidc_env(),
	)
	.expect_err("crates.io should reject registry provenance policy");

	let message = error.to_string();
	assert!(message.contains("cannot require registry-native package provenance"));
	assert!(message.contains("registry-native provenance is not available"));

	let error = enforce_release_attestation_prerequisites(
		&trusted_provenance_request(RegistryKind::Pypi),
		&github_oidc_env(),
	)
	.expect_err("PyPI should reject until the built-in publisher can require attestations");

	let message = error.to_string();
	assert!(message.contains("registry supports provenance"));
	assert!(message.contains("built-in publisher"));
}

#[test]
fn enforce_release_trust_prerequisites_rejects_long_lived_npm_tokens() {
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL.to_string(),
			"https://token.actions.githubusercontent.com".to_string(),
		),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN.to_string(),
			"request-token".to_string(),
		),
		("NPM_TOKEN".to_string(), "secret-token".to_string()),
	]);

	let error = enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&env_map,
	)
	.expect_err("long-lived npm tokens should be rejected");
	let message = error.to_string();
	assert!(message.contains("long-lived npm token environment variables"));
	assert!(message.contains("NPM_TOKEN"));
}

#[test]
fn enforce_release_trust_prerequisites_rejects_unsupported_provider_registry_pairs() {
	let root = workflow_root();
	let circle_env = BTreeMap::from([
		("CIRCLECI".to_string(), "true".to_string()),
		(
			"CIRCLE_PROJECT_USERNAME".to_string(),
			"monochange".to_string(),
		),
		(
			"CIRCLE_PROJECT_REPONAME".to_string(),
			"monochange".to_string(),
		),
		("CIRCLE_WORKFLOW_ID".to_string(), "workflow".to_string()),
	]);
	let error = enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&circle_env,
	)
	.expect_err("CircleCI npm trusted publishing should be rejected");
	let message = error.to_string();
	assert!(message.contains("cannot enforce trusted publishing"));
	assert!(message.contains("CircleCI"));

	let gitlab_env = BTreeMap::from([
		("GITLAB_CI".to_string(), "true".to_string()),
		(
			"CI_PROJECT_PATH".to_string(),
			"monochange/monochange".to_string(),
		),
		("CI_JOB_ID".to_string(), "42".to_string()),
	]);
	enforce_release_trust_prerequisites(
		&trusted_request(RegistryKind::Npm),
		Some(&sample_source()),
		root.path(),
		&gitlab_env,
	)
	.expect("supported non-GitHub trusted publishing identities should pass capability checks");
}

#[test]
fn forbidden_npm_token_env_keys_detects_config_auth_tokens() {
	let env_map = BTreeMap::from([
		(
			"npm_config_registry_auth_token".to_string(),
			"secret".to_string(),
		),
		("NPM_CONFIG_USERCONFIG".to_string(), ".npmrc".to_string()),
	]);
	assert_eq!(
		forbidden_npm_token_env_keys(&env_map),
		vec!["npm_config_registry_auth_token".to_string()]
	);
}

#[test]
fn verify_github_trust_context_reports_identity_mismatches() {
	let root = workflow_root();
	let request = trusted_request(RegistryKind::Npm);
	let expected = GitHubTrustContext {
		repository: "monochange/monochange".to_string(),
		workflow: "publish.yml".to_string(),
		environment: Some("publisher".to_string()),
	};

	let missing_repository = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		None,
		Some("publish.yml"),
		Some("publisher"),
	)
	.expect_err("missing GitHub repository should fail");
	assert!(
		missing_repository
			.to_string()
			.contains("GitHub Actions did not expose `GITHUB_REPOSITORY`")
	);

	let repository_mismatch = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		Some("other/repo"),
		Some("publish.yml"),
		Some("publisher"),
	)
	.expect_err("mismatched GitHub repository should fail");
	assert!(
		repository_mismatch.to_string().contains(
			"expected GitHub repository `monochange/monochange`, but detected `other/repo`"
		)
	);

	let missing_workflow = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		Some("monochange/monochange"),
		None,
		Some("publisher"),
	)
	.expect_err("missing GitHub workflow should fail");
	assert!(
		missing_workflow
			.to_string()
			.contains("GitHub Actions did not expose `GITHUB_WORKFLOW_REF`")
	);

	let environment_mismatch = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&expected,
		Some("monochange/monochange"),
		Some("publish.yml"),
		None,
	)
	.expect_err("missing GitHub environment should fail");
	assert!(
		environment_mismatch
			.to_string()
			.contains("expected GitHub environment `publisher`, but detected `none`")
	);

	let missing_oidc = verify_github_trust_context(
		&request,
		root.path(),
		&BTreeMap::new(),
		&GitHubTrustContext {
			environment: None,
			..expected
		},
		Some("monochange/monochange"),
		Some("publish.yml"),
		None,
	)
	.expect_err("missing GitHub OIDC token request variables should fail");
	assert!(missing_oidc.to_string().contains("grant `id-token: write`"));
}

#[test]
fn execute_publish_requests_blocks_trusted_publish_before_external_command() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut executor = FakeExecutor::new(Vec::new());
	let error = execute_publish_requests(
		Path::new("."),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		false,
		&[trusted_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect_err("trusted publishing should block local release publish");

	assert!(
		error
			.to_string()
			.contains("local/manual publishing is not allowed")
	);
	assert!(executor.commands.is_empty());
}

#[test]
fn ensure_publish_report_succeeded_reports_failed_outcomes() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_publish_outcome(
			"failed-pkg",
			PackagePublishStatus::Failed,
		)],
	};
	let error = ensure_publish_report_succeeded(&report)
		.expect_err("failed publish outcome should fail command");
	assert!(error.to_string().contains("failed-pkg 1.2.3"));

	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_publish_outcome(
			"done",
			PackagePublishStatus::SkippedExisting,
		)],
	};
	ensure_publish_report_succeeded(&report)
		.unwrap_or_else(|error| panic!("successful publish report: {error}"));
}

#[test]
fn resume_publish_requests_skips_completed_versions_and_retries_failed_work() {
	let mut completed = sample_request(RegistryKind::Npm);
	completed.package_id = "done".to_string();
	let mut failed = sample_request(RegistryKind::Npm);
	failed.package_id = "retry".to_string();
	let previous = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![
			sample_publish_outcome("done", PackagePublishStatus::Published),
			sample_publish_outcome("retry", PackagePublishStatus::Failed),
		],
	};

	let (pending, resumed) = resume_publish_requests(&[completed, failed], Some(&previous))
		.unwrap_or_else(|error| panic!("resume requests: {error}"));

	assert_eq!(resumed.len(), 1);
	assert_eq!(resumed[0].package, "done");
	assert_eq!(pending.len(), 1);
	assert_eq!(pending[0].package_id, "retry");
}

#[test]
fn merge_publish_resume_report_preserves_current_or_prepends_resumed_outcomes() {
	let current = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_publish_outcome(
			"current",
			PackagePublishStatus::Published,
		)],
	};

	let unchanged = merge_publish_resume_report(
		PackagePublishRunMode::Release,
		false,
		Vec::new(),
		current.clone(),
	);
	assert_eq!(unchanged, current);

	let merged = merge_publish_resume_report(
		PackagePublishRunMode::Release,
		false,
		vec![sample_publish_outcome(
			"resumed",
			PackagePublishStatus::SkippedExisting,
		)],
		current,
	);
	assert_eq!(merged.packages.len(), 2);
	assert_eq!(merged.packages[0].package, "resumed");
	assert_eq!(merged.packages[1].package, "current");
}

#[test]
fn resume_publish_requests_rejects_dry_run_and_placeholder_reports() {
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: true,
		packages: Vec::new(),
	};
	let error =
		resume_publish_requests(&[], Some(&report)).expect_err("dry-run resume report should fail");
	assert!(error.to_string().contains("real publish run"));

	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Placeholder,
		dry_run: false,
		packages: Vec::new(),
	};
	let error = resume_publish_requests(&[], Some(&report))
		.expect_err("placeholder resume report should fail");
	assert!(error.to_string().contains("mc publish"));
}

#[test]
fn publish_report_artifact_round_trips_and_reports_io_errors() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let report = PackagePublishReport {
		mode: PackagePublishRunMode::Release,
		dry_run: false,
		packages: vec![sample_publish_outcome(
			"done",
			PackagePublishStatus::SkippedExisting,
		)],
	};
	let output = tempdir.path().join("nested/publish-result.json");

	write_publish_report_artifact(&output, &report)
		.unwrap_or_else(|error| panic!("write report: {error}"));
	let read_report = read_publish_report_artifact(&output)
		.unwrap_or_else(|error| panic!("read report: {error}"));
	assert_eq!(read_report, report);

	let missing_error = read_publish_report_artifact(&tempdir.path().join("missing.json"))
		.expect_err("missing artifact should fail");
	assert!(missing_error.to_string().contains("failed to read"));

	let invalid_json_path = tempdir.path().join("invalid.json");
	fs::write(&invalid_json_path, "not json")
		.unwrap_or_else(|error| panic!("write invalid json: {error}"));
	let parse_error =
		read_publish_report_artifact(&invalid_json_path).expect_err("invalid artifact should fail");
	assert!(parse_error.to_string().contains("failed to parse"));

	let write_error = write_publish_report_artifact(tempdir.path(), &report)
		.expect_err("directory output path should fail");
	assert!(write_error.to_string().contains("failed to write"));

	let parent_file = tempdir.path().join("file-parent");
	fs::write(&parent_file, "not a directory")
		.unwrap_or_else(|error| panic!("write file parent: {error}"));
	let create_error = write_publish_report_artifact(&parent_file.join("result.json"), &report)
		.expect_err("file parent should fail directory creation");
	assert!(create_error.to_string().contains("failed to create"));
	assert!(
		publish_report_json_error("bad json")
			.to_string()
			.contains("failed to serialize package publish report")
	);
}

fn write_cargo_manifest(root: &Path, contents: &str) -> PathBuf {
	let package_root = root.join("pkg");
	fs::create_dir_all(&package_root).expect("package dir");
	let manifest_path = package_root.join("Cargo.toml");
	fs::write(&manifest_path, contents).expect("write Cargo manifest");
	manifest_path
}

fn sample_cargo_request(root: &Path, manifest_path: &Path) -> PublishRequest {
	PublishRequest {
		manifest_path: manifest_path.to_path_buf(),
		package_root: root.join("pkg"),
		..sample_request(RegistryKind::CratesIo)
	}
}

#[test]
fn cargo_publish_readiness_blockers_require_crates_io_metadata_and_publish_access() {
	let root = tempfile::tempdir().expect("tempdir");
	let manifest_path = write_cargo_manifest(
		root.path(),
		r#"
[package]
name = "pkg"
version = "1.2.3"
publish = ["internal"]
"#,
	);
	let request = sample_cargo_request(root.path(), &manifest_path);

	let blockers = cargo_publish_readiness_blockers(root.path(), &request).expect("blockers");

	assert!(blockers.contains(&"package.publish does not include crates-io".to_string()));
	assert!(blockers.contains(&"package.description is required for crates.io".to_string()));
	assert!(blockers.contains(
		&"package.license or package.license-file is required for crates.io".to_string()
	));
}

#[test]
fn cargo_publish_readiness_blockers_ignore_non_cargo_requests() {
	let blockers =
		cargo_publish_readiness_blockers(Path::new("."), &sample_request(RegistryKind::Npm))
			.expect("blockers");

	assert!(blockers.is_empty());
}

#[test]
fn cargo_publish_readiness_blockers_report_manifest_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	let missing = root.path().join("pkg/Cargo.toml");
	let missing_request = sample_cargo_request(root.path(), &missing);
	let missing_error = cargo_publish_readiness_blockers(root.path(), &missing_request)
		.expect_err("expected read error");
	assert!(
		missing_error
			.to_string()
			.contains("failed to read Cargo manifest")
	);

	let invalid = write_cargo_manifest(root.path(), "not valid toml");
	let invalid_request = sample_cargo_request(root.path(), &invalid);
	let invalid_error = cargo_publish_readiness_blockers(root.path(), &invalid_request)
		.expect_err("expected parse error");
	assert!(invalid_error.to_string().contains("failed to parse"));
}

#[test]
fn cargo_publish_readiness_blockers_report_missing_package_table() {
	let root = tempfile::tempdir().expect("tempdir");
	let manifest_path = write_cargo_manifest(root.path(), "[workspace]\nmembers = []\n");
	let request = sample_cargo_request(root.path(), &manifest_path);

	let blockers = cargo_publish_readiness_blockers(root.path(), &request).expect("blockers");

	assert_eq!(
		blockers,
		vec!["Cargo manifest is missing [package]".to_string()]
	);
}

#[test]
fn cargo_publish_readiness_blockers_reject_publish_false() {
	let root = tempfile::tempdir().expect("tempdir");
	let manifest_path = write_cargo_manifest(
		root.path(),
		r#"
[package]
name = "pkg"
version = "1.2.3"
description = "A package"
license = "MIT"
publish = false
"#,
	);
	let request = sample_cargo_request(root.path(), &manifest_path);

	let blockers = cargo_publish_readiness_blockers(root.path(), &request).expect("blockers");

	assert_eq!(blockers, vec!["package.publish is false".to_string()]);
}

#[test]
fn cargo_publish_readiness_blockers_accept_workspace_inherited_metadata() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("Cargo.toml"),
		r#"
[workspace.package]
description = "Workspace description"
license = "MIT"
"#,
	)
	.expect("write workspace manifest");
	let manifest_path = write_cargo_manifest(
		root.path(),
		r#"
[package]
name = "pkg"
version = "1.2.3"
description = { workspace = true }
license = { workspace = true }
publish = ["crates-io"]
"#,
	);
	let request = sample_cargo_request(root.path(), &manifest_path);

	let blockers = cargo_publish_readiness_blockers(root.path(), &request).expect("blockers");

	assert!(blockers.is_empty());
}

#[test]
fn execute_publish_requests_marks_dry_run_cargo_metadata_blockers() {
	let root = tempfile::tempdir().expect("tempdir");
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(404);
	});
	let manifest_path = write_cargo_manifest(
		root.path(),
		r#"
[package]
name = "pkg"
version = "1.2.3"
"#,
	);
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let request = sample_cargo_request(root.path(), &manifest_path);
	let mut executor = FakeExecutor::new(Vec::new());

	let report = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		true,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("report");

	assert_eq!(report.packages[0].status, PackagePublishStatus::Blocked);
	assert!(
		report.packages[0]
			.message
			.contains("package.description is required for crates.io")
	);
	assert!(executor.commands.is_empty());
}

#[test]
fn execute_publish_requests_rejects_real_cargo_metadata_blockers() {
	let root = tempfile::tempdir().expect("tempdir");
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(404);
	});
	let manifest_path = write_cargo_manifest(
		root.path(),
		r#"
[package]
name = "pkg"
version = "1.2.3"
"#,
	);
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let request = sample_cargo_request(root.path(), &manifest_path);
	let mut executor = FakeExecutor::new(Vec::new());

	let error = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		false,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect_err("expected readiness blocker");

	assert!(
		error
			.to_string()
			.contains("pkg 1.2.3 is not ready to publish to crates_io")
	);
	assert!(executor.commands.is_empty());
}

#[test]
fn execute_publish_requests_skips_external_and_existing_versions() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": { "1.2.3": {} }
		}));
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};
	let request = PublishRequest {
		mode: PublishMode::External,
		..sample_request(RegistryKind::Npm)
	};
	let existing = sample_request(RegistryKind::Npm);
	let mut executor = FakeExecutor::new(Vec::new());
	let report = execute_publish_requests(
		Path::new("."),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		true,
		&[request, existing],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("report:");
	assert_eq!(report.packages.len(), 2);
	assert_eq!(
		report.packages[0].status,
		PackagePublishStatus::SkippedExternal
	);
	assert_eq!(
		report.packages[1].status,
		PackagePublishStatus::SkippedExisting
	);
}

#[test]
fn filter_pending_publish_requests_skips_external_and_existing_versions() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": { "1.2.3": {} }
		}));
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let request = PublishRequest {
		mode: PublishMode::External,
		..sample_request(RegistryKind::Npm)
	};
	let existing = sample_request(RegistryKind::Npm);
	let pending = PublishRequest {
		package_id: "pkg-next".to_string(),
		package_name: "pkg-next".to_string(),
		..sample_request(RegistryKind::Npm)
	};
	server.mock(|when, then| {
		when.method(GET).path("/pkg-next");
		then.status(404);
	});

	let filtered = filter_pending_publish_requests_with_transport(
		&[request, existing, pending],
		&client,
		&endpoints,
	)
	.expect("filtered requests:");

	assert_eq!(filtered.len(), 1);
	assert_eq!(filtered[0].package_id, "pkg-next");
}

#[test]
fn execute_publish_requests_publishes_release_with_trust_outcome() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let root = workflow_root();
	let env_map = BTreeMap::from([
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_JOB".to_string(), "release".to_string()),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_URL.to_string(),
			"https://token.actions.githubusercontent.com".to_string(),
		),
		(
			GITHUB_ACTIONS_ID_TOKEN_REQUEST_TOKEN.to_string(),
			"request-token".to_string(),
		),
	]);
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: String::new(),
		stderr: String::new(),
	}]);

	let report = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Release,
		false,
		&[trusted_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&env_map,
		&mut executor,
	)
	.expect("report:");

	assert_eq!(report.packages.len(), 1);
	assert_eq!(report.packages[0].status, PackagePublishStatus::Published);
	assert_eq!(
		report.packages[0].trusted_publishing.status,
		TrustedPublishingStatus::Configured
	);
	assert!(
		report.packages[0]
			.trusted_publishing
			.message
			.contains("npm trusted publishing")
	);
	// No trust commands are executed — trust configuration is manual
	assert_eq!(executor.commands.len(), 1);
}

#[test]
fn release_dry_run_orders_cargo_dev_and_build_dependencies_before_dependents() {
	let expected_order = vec![
		"zephyr-build",
		"lima-dev",
		"quartz-build",
		"ember-dev",
		"nyx-build",
		"cedar-dev",
		"violet-build",
		"osprey-build",
		"delta-dev",
		"amber-dev",
		"mango-build",
		"sable-dev",
		"orbit-build",
		"binary-build",
		"atlas-dev",
		"config",
	];
	let packages = [
		("config", Some(("build-dependencies", "atlas-dev"))),
		("atlas-dev", Some(("build-dependencies", "binary-build"))),
		("binary-build", Some(("dev-dependencies", "orbit-build"))),
		("orbit-build", Some(("build-dependencies", "sable-dev"))),
		("sable-dev", Some(("dev-dependencies", "mango-build"))),
		("mango-build", Some(("build-dependencies", "amber-dev"))),
		("amber-dev", Some(("dev-dependencies", "delta-dev"))),
		("delta-dev", Some(("build-dependencies", "osprey-build"))),
		("osprey-build", Some(("dev-dependencies", "violet-build"))),
		("violet-build", Some(("build-dependencies", "cedar-dev"))),
		("cedar-dev", Some(("dev-dependencies", "nyx-build"))),
		("nyx-build", Some(("build-dependencies", "ember-dev"))),
		("ember-dev", Some(("dev-dependencies", "quartz-build"))),
		("quartz-build", Some(("build-dependencies", "lima-dev"))),
		("lima-dev", Some(("dev-dependencies", "zephyr-build"))),
		("zephyr-build", None),
	];
	let server = MockServer::start();
	for (package, _) in packages {
		server.mock(|when, then| {
			when.method(GET).path(format!("/crates/{package}"));
			then.status(404);
		});
	}

	let root = tempfile::tempdir().expect("tempdir:");
	let members = packages
		.iter()
		.map(|(package, _)| format!("\"{package}\""))
		.collect::<Vec<_>>()
		.join(", ");
	fs::write(
		root.path().join("Cargo.toml"),
		format!("[workspace]\nmembers = [{members}]\nresolver = \"2\"\n"),
	)
	.expect("write workspace manifest:");

	for (package, dependency) in packages {
		let package_dir = root.path().join(package);
		fs::create_dir_all(package_dir.join("src")).expect("mkdir package:");
		fs::write(package_dir.join("src/lib.rs"), "").expect("write package lib:");

		let dependency_section = dependency.map_or_else(String::new, |(kind, dependency)| {
			format!(
				"\n[{kind}]\n{dependency} = {{ path = \"../{dependency}\", version = \"1.0.0\" }}\n"
			)
		});
		fs::write(
			package_dir.join("Cargo.toml"),
			format!(
				concat!(
					"[package]\n",
					"name = \"{package}\"\n",
					"version = \"1.0.0\"\n",
					"edition = \"2021\"\n",
					"license = \"MIT\"\n",
					"description = \"test package\"\n",
					"{dependency_section}"
				),
				package = package,
				dependency_section = dependency_section,
			),
		)
		.expect("write package manifest:");
	}

	let configuration_packages = packages
		.iter()
		.map(|(package, _)| (*package, monochange_core::PackageType::Cargo, true))
		.collect::<Vec<_>>();
	let configuration = sample_configuration(&configuration_packages);
	let publications = packages
		.into_iter()
		.map(|(package, _)| {
			PackagePublicationTarget {
				package: package.to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: "1.0.0".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			}
		})
		.collect::<Vec<_>>();

	with_vars(
		[("MONOCHANGE_CRATES_IO_API_URL", Some(server.base_url()))],
		|| {
			let report = run_publish_packages_with_publications(
				root.path(),
				&configuration,
				&publications,
				&BTreeSet::new(),
				true,
			)
			.expect("publish report:");
			let order = report
				.packages
				.iter()
				.map(|package| package.package.as_str())
				.collect::<Vec<_>>();

			assert_eq!(order, expected_order);
		},
	);
}

#[test]
fn execute_publish_requests_placeholder_dry_run_validates_publish_command() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: String::new(),
		stderr: String::new(),
	}]);

	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		true,
		&[sample_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("report:");

	assert_eq!(report.packages[0].status, PackagePublishStatus::Planned);
	assert!(executor.commands.is_empty());
}

#[test]
fn execute_publish_requests_placeholder_dry_run_surfaces_manifest_prerequisites() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	let package_root = root.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	fs::write(
		package_root.join("Cargo.toml"),
		concat!("[package]\n", "name = \"pkg\"\n", "version = \"1.0.0\"\n",),
	)
	.expect("write manifest:");

	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut request = sample_request(RegistryKind::CratesIo);
	request.manifest_path = package_root.join("Cargo.toml");
	request.package_root = package_root;
	request.placeholder = true;
	let mut executor = FakeExecutor::new(Vec::new());

	let error = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Placeholder,
		true,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect_err("expected placeholder manifest error");
	assert!(
		error.to_string().contains(
			"placeholder publishing requires `package.license` or `package.license-file`"
		)
	);
	assert!(executor.commands.is_empty());
}

#[test]
fn execute_publish_requests_publishes_placeholder_and_flags_manual_trust() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	let package_root = root.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir:");
	fs::write(
		package_root.join("Cargo.toml"),
		concat!(
			"[package]\n",
			"name = \"pkg\"\n",
			"version = \"1.0.0\"\n",
			"license = \"MIT\"\n",
		),
	)
	.expect("write manifest:");

	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut request = trusted_request(RegistryKind::CratesIo);
	request.manifest_path = package_root.join("Cargo.toml");
	request.package_root = package_root;
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: String::new(),
		stderr: String::new(),
	}]);

	let report = execute_publish_requests(
		root.path(),
		Some(&sample_source()),
		PackagePublishRunMode::Placeholder,
		false,
		&[request],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("report:");

	assert_eq!(report.packages[0].status, PackagePublishStatus::Published);
	assert_eq!(
		report.packages[0].trusted_publishing.status,
		TrustedPublishingStatus::ManualActionRequired
	);
	assert_eq!(executor.commands[0].program, "cargo");
}

#[test]
fn execute_publish_requests_surfaces_publish_command_failures() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client:");
	let endpoints = sample_endpoints(&server.base_url());
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: false,
		stdout: String::new(),
		stderr: "boom".to_string(),
	}]);

	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[sample_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("publish report");

	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(report.packages[0].message.contains("npm publish"));
	assert!(report.packages[0].message.contains("boom"));

	let mut executor = FakeExecutor::new(Vec::new());
	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[sample_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("publish report");
	assert_eq!(report.packages[0].status, PackagePublishStatus::Failed);
	assert!(
		report.packages[0]
			.message
			.contains("missing fake command output")
	);
}

#[test]
fn execute_publish_requests_uses_disabled_trust_outcome_for_successful_builtin_publish() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let client = Client::builder().build().expect("http client");
	let endpoints = sample_endpoints(&server.base_url());
	let mut executor = FakeExecutor::new(vec![CommandOutput {
		success: true,
		stdout: String::new(),
		stderr: String::new(),
	}]);

	let report = execute_publish_requests(
		Path::new("."),
		None,
		PackagePublishRunMode::Release,
		false,
		&[sample_request(RegistryKind::Npm)],
		&client,
		&endpoints,
		&BTreeMap::new(),
		&mut executor,
	)
	.expect("report");
	assert_eq!(
		report.packages[0].trusted_publishing.status,
		TrustedPublishingStatus::Disabled
	);
}

#[test]
fn run_placeholder_publish_uses_env_overrides_for_registry_endpoints() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});

	let root = tempfile::tempdir().expect("tempdir:");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("config:");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir:");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("manifest:");
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration:");

	with_locked_env_vars(|| {
		with_vars(
			vec![(
				"MONOCHANGE_NPM_REGISTRY_URL",
				Some(server.base_url().as_str()),
			)],
			|| {
				let report =
					run_placeholder_publish(root.path(), &configuration, &BTreeSet::new(), true)
						.expect("placeholder report:");
				assert_eq!(report.mode, PackagePublishRunMode::Placeholder);
				assert_eq!(report.packages.len(), 1);
				assert_eq!(report.packages[0].status, PackagePublishStatus::Planned);
			},
		);
	});
}

#[test]
fn run_publish_packages_uses_prepared_release_publications() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("config:");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir:");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("manifest:");
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration:");
	let prepared_release = sample_prepared_release(
		root.path(),
		vec![PackagePublicationTarget {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		}],
	);

	with_locked_env_vars(|| {
		with_vars(
			vec![(
				"MONOCHANGE_NPM_REGISTRY_URL",
				Some(server.base_url().as_str()),
			)],
			|| {
				let report = run_publish_packages(
					root.path(),
					&configuration,
					Some(&prepared_release),
					&BTreeSet::new(),
					true,
				)
				.expect("publish report:");
				assert_eq!(report.mode, PackagePublishRunMode::Release);
				assert_eq!(report.packages.len(), 1);
				assert_eq!(report.packages[0].status, PackagePublishStatus::Planned);
				assert_eq!(report.packages[0].version, "1.2.3");
			},
		);
	});
}

#[test]
fn run_publish_packages_discovers_release_record_publications_from_head() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n",
	)
	.expect("config:");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir:");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("manifest:");
	fs::write(root.path().join("tracked.txt"), "initial\n").expect("tracked:");
	git(root.path(), &["init"]);
	git(root.path(), &["config", "user.name", "monochange Tests"]);
	git(
		root.path(),
		&["config", "user.email", "monochange@example.com"],
	);
	git(root.path(), &["add", "."]);
	git(root.path(), &["commit", "-m", "initial"]);
	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration:");
	commit_release_record(
		root.path(),
		vec![PackagePublicationTarget {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		}],
	);
	let discovered = release_record_package_publications_from_prepared_or_head(root.path(), None)
		.expect("release record publications");
	assert_eq!(discovered.len(), 1);

	with_locked_env_vars(|| {
		with_vars(
			vec![(
				"MONOCHANGE_NPM_REGISTRY_URL",
				Some(server.base_url().as_str()),
			)],
			|| {
				let report =
					run_publish_packages(root.path(), &configuration, None, &BTreeSet::new(), true)
						.expect("publish report:");
				assert_eq!(report.mode, PackagePublishRunMode::Release);
				assert_eq!(report.packages.len(), 1);
				assert_eq!(report.packages[0].status, PackagePublishStatus::Planned);
				assert_eq!(report.packages[0].version, "1.2.3");
			},
		);
	});
}

#[test]
fn process_command_executor_runs_commands_and_reports_spawn_failures() {
	let tempdir = tempfile::tempdir().expect("tempdir:");
	let mut executor = ProcessCommandExecutor;
	let success = executor
		.run(&CommandSpec {
			program: "sh".to_string(),
			args: vec![
				"-c".to_string(),
				"printf stdout; printf stderr >&2".to_string(),
			],
			cwd: tempdir.path().to_path_buf(),
		})
		.expect("expected command success:");
	assert!(success.success);
	assert_eq!(success.stdout, "stdout");
	assert_eq!(success.stderr, "stderr");

	let error = executor
		.run(&CommandSpec {
			program: "definitely-not-a-real-command".to_string(),
			args: Vec::new(),
			cwd: tempdir.path().to_path_buf(),
		})
		.expect_err("expected command failure");
	assert!(
		error
			.to_string()
			.contains("failed to run `definitely-not-a-real-command`")
	);
}

#[test]
fn fake_executor_reports_missing_outputs_and_render_helpers_match() {
	let mut executor = FakeExecutor::new(Vec::new());
	let spec = CommandSpec {
		program: "npm".to_string(),
		args: vec![
			"publish".to_string(),
			"--access".to_string(),
			"public".to_string(),
		],
		cwd: PathBuf::from("."),
	};
	let error = executor
		.run(&spec)
		.expect_err("expected fake executor error");
	assert!(error.to_string().contains("missing fake command output"));
	assert_eq!(render_command(&spec), "npm publish --access public");
	assert_eq!(
		render_command_error(&CommandOutput {
			success: false,
			stdout: String::new(),
			stderr: String::new(),
		}),
		"command failed"
	);
}

#[test]
fn append_publish_dry_run_args_replaces_force_with_dry_run_for_pubdev() {
	let mut args = vec![
		"pub".to_string(),
		"publish".to_string(),
		"--force".to_string(),
	];
	append_publish_dry_run_args(&mut args, RegistryKind::PubDev, true);
	assert!(!args.contains(&"--force".to_string()));
	assert!(args.contains(&"--dry-run".to_string()));
}

#[test]
fn append_publish_dry_run_args_appends_standard_flag_for_non_pubdev_registries() {
	for registry in [RegistryKind::Npm, RegistryKind::CratesIo, RegistryKind::Jsr] {
		let mut args = vec!["publish".to_string()];
		append_publish_dry_run_args(&mut args, registry, true);
		assert_eq!(args.last(), Some(&"--dry-run".to_string()));
	}
}

#[test]
fn build_npm_placeholder_publish_command_uses_package_root_as_cwd() {
	let command = build_npm_placeholder_publish_command(
		&sample_request(RegistryKind::Npm),
		Path::new("/tmp/placeholder"),
	);
	assert_eq!(command.program, "npm");
	assert_eq!(command.cwd, PathBuf::from("/workspace/pkg"));
	assert_eq!(command.args[0], "publish");
}

#[test]
fn write_cargo_placeholder_manifest_reads_workspace_license_file_from_root() {
	let root = tempfile::tempdir().expect("tempdir");
	let package_root = root.path().join("pkg");
	fs::create_dir_all(&package_root).expect("mkdir");
	fs::write(
		root.path().join("Cargo.toml"),
		concat!(
			"[workspace]\n",
			"members = [\"pkg\"]\n",
			"[workspace.package]\n",
			"license-file = \"LICENSE\"\n",
		),
	)
	.expect("write workspace manifest");
	fs::write(
		package_root.join("Cargo.toml"),
		concat!("[package]\n", "name = \"pkg\"\n", "version = \"1.0.0\"\n"),
	)
	.expect("write package manifest");
	fs::write(root.path().join("LICENSE"), "MIT").expect("write license");
	let request = PublishRequest {
		manifest_path: package_root.join("Cargo.toml"),
		package_root,
		..sample_request(RegistryKind::CratesIo)
	};
	let placeholder_dir = tempfile::tempdir().expect("tempdir");
	write_cargo_placeholder_manifest(placeholder_dir.path(), &request, root.path(), None)
		.expect("cargo placeholder");
	let placeholder_manifest = fs::read_to_string(placeholder_dir.path().join("Cargo.toml"))
		.expect("read placeholder manifest");
	assert!(placeholder_manifest.contains("license-file = \"LICENSE\""));
	assert_eq!(
		fs::read_to_string(placeholder_dir.path().join("LICENSE"))
			.expect("read placeholder license"),
		"MIT"
	);
}

#[test]
fn extract_workspace_package_table_returns_workspace_package_table() {
	let parsed = toml::from_str::<TomlValue>(concat!(
		"[workspace]\n",
		"members = [\"pkg\"]\n",
		"[workspace.package]\n",
		"license = \"MIT\"\n",
	))
	.expect("parse manifest");
	let workspace_package = extract_workspace_package_table(&parsed).expect("package table");
	assert_eq!(
		workspace_package.get("license").and_then(TomlValue::as_str),
		Some("MIT")
	);
}

#[test]
fn read_workspace_package_table_returns_workspace_package_table() {
	let root = tempfile::tempdir().expect("tempdir");
	fs::write(
		root.path().join("Cargo.toml"),
		concat!(
			"[workspace]\n",
			"members = [\"pkg\"]\n",
			"[workspace.package]\n",
			"license = \"MIT\"\n",
		),
	)
	.expect("write manifest");
	let workspace_package = read_workspace_package_table(root.path())
		.expect("workspace package")
		.expect("package table");
	assert_eq!(
		workspace_package.get("license").and_then(TomlValue::as_str),
		Some("MIT")
	);
}

#[test]
fn read_workspace_package_table_reports_io_and_parse_errors() {
	let root = tempfile::tempdir().expect("tempdir");
	let read_result = read_workspace_package_table(root.path());
	assert!(read_result.is_ok());
	assert!(read_result.expect("read").is_none());

	let manifest_path = root.path().join("Cargo.toml");
	fs::write(&manifest_path, "[workspace]\nmembers = []\n").expect("write manifest");
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		let mut permissions = fs::metadata(&manifest_path)
			.expect("metadata")
			.permissions();
		permissions.set_mode(0o000);
		fs::set_permissions(&manifest_path, permissions).expect("chmod");
		let read_error =
			read_workspace_package_table(root.path()).expect_err("expected read error");
		assert!(
			read_error
				.to_string()
				.contains("failed to read Cargo manifest")
		);
		let mut restore = fs::metadata(&manifest_path)
			.expect("metadata")
			.permissions();
		restore.set_mode(0o644);
		fs::set_permissions(&manifest_path, restore).expect("restore chmod");
	}

	fs::write(&manifest_path, "not valid toml").expect("write invalid");
	let parse_result = read_workspace_package_table(root.path());
	let error = parse_result.expect_err("expected parse error");
	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn build_release_requests_uses_publication_targets_and_package_metadata() {
	let package = PackageRecord {
		id: "npm:packages/pkg/package.json".to_string(),
		name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from("/workspace/packages/pkg/package.json"),
		workspace_root: PathBuf::from("/workspace"),
		current_version: Some(Version::parse("1.0.0").expect("version:")),
		publish_state: PublishState::Public,
		version_group_id: None,
		metadata: BTreeMap::from([
			("config_id".to_string(), "pkg".to_string()),
			("manager".to_string(), "pnpm".to_string()),
		]),
		declared_dependencies: Vec::new(),
	};
	let publication = PackagePublicationTarget {
		package: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
		version: "1.2.3".to_string(),
		mode: PublishMode::Builtin,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
	};
	let configuration = sample_configuration(&[("pkg", monochange_core::PackageType::Npm, true)]);
	let requests =
		build_release_requests(&configuration, &[package], &[publication], &BTreeSet::new())
			.expect("requests:");
	assert_eq!(requests.len(), 1);
	let request = requests.first().expect("request");
	assert_eq!(request.version, "1.2.3");
	assert_eq!(request.package_name, "pkg");
	assert_eq!(request.package_manager.as_deref(), Some("pnpm"));
	assert_eq!(
		request.package_metadata.get("manager").map(String::as_str),
		Some("pnpm")
	);
}

#[test]
fn run_publish_packages_with_resume_filters_by_group_and_ecosystem() {
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/pkg");
		then.status(404);
	});
	let root = tempfile::tempdir().expect("tempdir:");
	fs::write(
		root.path().join("monochange.toml"),
		"[package.pkg]\npath = \"packages/pkg\"\ntype = \"npm\"\n\n[group.my-group]\npackages = [\"pkg\"]\n",
	)
	.expect("config:");
	fs::create_dir_all(root.path().join("packages/pkg")).expect("mkdir:");
	fs::write(
		root.path().join("packages/pkg/package.json"),
		r#"{ "name": "pkg", "version": "1.0.0" }"#,
	)
	.expect("manifest:");

	let configuration = crate::load_workspace_configuration(root.path()).expect("configuration:");
	let prepared_release = sample_prepared_release(
		root.path(),
		vec![PackagePublicationTarget {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		}],
	);

	let mut selected_groups = BTreeSet::new();
	selected_groups.insert("my-group".to_string());

	let mut selected_ecosystems = BTreeSet::new();
	selected_ecosystems.insert(Ecosystem::Npm);

	with_locked_env_vars(|| {
		with_vars(
			vec![(
				"MONOCHANGE_NPM_REGISTRY_URL",
				Some(server.base_url().as_str()),
			)],
			|| {
				let report = run_publish_packages_with_resume(
					root.path(),
					&configuration,
					Some(&prepared_release),
					&BTreeSet::new(),
					&selected_groups,
					&selected_ecosystems,
					true,
					None,
				)
				.expect("publish report:");

				assert_eq!(report.packages.len(), 1);
				assert_eq!(report.packages[0].package, "pkg");
			},
		);
	});
}
