use monochange_core::DependencyKind;
use monochange_core::GroupChangelogInclude;
use monochange_core::PackageDependency;
use monochange_core::VersionFormat;

use super::*;

fn builtin_provider_registry_trust_capability(
	registry: RegistryKind,
	provider: CiProviderKind,
) -> ProviderRegistryTrustCapability {
	provider_registry_trust_capability(&PublishRegistry::Builtin(registry), provider)
}

#[test]
fn detects_github_actions_identity_from_workflow_ref() {
	let env_map = BTreeMap::from([
		("GITHUB_ACTIONS".to_string(), "true".to_string()),
		(
			"GITHUB_REPOSITORY".to_string(),
			"monochange/monochange".to_string(),
		),
		(
			"GITHUB_WORKFLOW_REF".to_string(),
			"monochange/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
		),
		("GITHUB_RUN_ID".to_string(), "123".to_string()),
	]);

	let identity = detect_trusted_publishing_identity(&env_map);

	assert_eq!(identity.provider(), CiProviderKind::GitHubActions);
	assert!(identity.is_verifiable_by_env());
	assert!(matches!(
		identity,
		TrustedPublishingIdentity::GitHubActions {
			workflow: Some(workflow),
			..
		} if workflow == "publish.yml"
	));
}

#[test]
fn detects_gitlab_circleci_and_google_cloud_build_identities() {
	let gitlab = detect_trusted_publishing_identity(&BTreeMap::from([
		("GITLAB_CI".to_string(), "true".to_string()),
		(
			"CI_PROJECT_PATH".to_string(),
			"monochange/monochange".to_string(),
		),
		("CI_JOB_ID".to_string(), "42".to_string()),
	]));
	assert_eq!(gitlab.provider(), CiProviderKind::GitLabCi);
	assert!(gitlab.is_verifiable_by_env());

	let circle = detect_trusted_publishing_identity(&BTreeMap::from([
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
	]));
	assert_eq!(circle.provider(), CiProviderKind::CircleCi);
	assert!(circle.is_verifiable_by_env());

	let google = detect_trusted_publishing_identity(&BTreeMap::from([
		("BUILD_ID".to_string(), "build".to_string()),
		("PROJECT_ID".to_string(), "project".to_string()),
		("REPO_NAME".to_string(), "monochange".to_string()),
	]));
	assert_eq!(google.provider(), CiProviderKind::GoogleCloudBuild);
	assert!(google.is_verifiable_by_env());
}

#[test]
fn builtin_registry_matrix_lists_supported_providers_without_overstating_setup_verification() {
	let npm = builtin_registry_trust_capabilities(RegistryKind::Npm);
	assert!(npm.trusted_publishing);
	assert_eq!(
		npm.supported_providers,
		vec![CiProviderKind::GitHubActions, CiProviderKind::GitLabCi]
	);
	assert!(npm.registry_setup_verifiable);
	assert!(npm.registry_setup_automation);
	assert!(npm.registry_native_provenance);

	let crates = builtin_registry_trust_capabilities(RegistryKind::CratesIo);
	assert_eq!(
		crates.supported_providers,
		vec![CiProviderKind::GitHubActions]
	);
	assert!(!crates.registry_setup_verifiable);
	assert!(!crates.registry_native_provenance);

	let pypi = builtin_registry_trust_capabilities(RegistryKind::Pypi);
	assert_eq!(
		pypi.supported_providers,
		vec![
			CiProviderKind::GitHubActions,
			CiProviderKind::GitLabCi,
			CiProviderKind::GoogleCloudBuild,
		]
	);
	assert!(!pypi.registry_setup_verifiable);
	assert!(pypi.registry_native_provenance);
}

#[test]
fn supported_provider_registry_combinations_are_claimed_explicitly() {
	let expected = [
		(RegistryKind::Npm, CiProviderKind::GitHubActions, true, true),
		(RegistryKind::Npm, CiProviderKind::GitLabCi, false, true),
		(
			RegistryKind::CratesIo,
			CiProviderKind::GitHubActions,
			false,
			false,
		),
		(
			RegistryKind::Jsr,
			CiProviderKind::GitHubActions,
			false,
			true,
		),
		(
			RegistryKind::PubDev,
			CiProviderKind::GitHubActions,
			false,
			false,
		),
		(
			RegistryKind::PubDev,
			CiProviderKind::GoogleCloudBuild,
			false,
			false,
		),
		(
			RegistryKind::Pypi,
			CiProviderKind::GitHubActions,
			false,
			true,
		),
		(RegistryKind::Pypi, CiProviderKind::GitLabCi, false, true),
		(
			RegistryKind::Pypi,
			CiProviderKind::GoogleCloudBuild,
			false,
			true,
		),
	];

	for (registry, provider, setup_verifiable, provenance) in expected {
		let capability = builtin_provider_registry_trust_capability(registry, provider);
		assert!(
			capability.trusted_publishing,
			"expected {provider:?} to be supported for {registry}"
		);
		assert!(capability.ci_identity_verifiable);
		assert_eq!(capability.registry_setup_verifiable, setup_verifiable);
		assert_eq!(capability.registry_setup_automation, setup_verifiable);
		assert_eq!(capability.registry_native_provenance, provenance);
	}
}

#[test]
fn provider_registry_capability_distinguishes_trust_from_provenance() {
	let npm_github = builtin_provider_registry_trust_capability(
		RegistryKind::Npm,
		CiProviderKind::GitHubActions,
	);
	assert!(npm_github.trusted_publishing);
	assert!(npm_github.registry_setup_verifiable);
	assert!(npm_github.registry_native_provenance);

	let crates_github = builtin_provider_registry_trust_capability(
		RegistryKind::CratesIo,
		CiProviderKind::GitHubActions,
	);
	assert!(crates_github.trusted_publishing);
	assert!(!crates_github.registry_native_provenance);

	let jsr_circle =
		builtin_provider_registry_trust_capability(RegistryKind::Jsr, CiProviderKind::CircleCi);
	assert!(!jsr_circle.trusted_publishing);
	assert!(!jsr_circle.registry_native_provenance);
}

#[test]
fn custom_registry_is_not_treated_as_trusted_by_default() {
	let custom = provider_registry_trust_capability(
		&PublishRegistry::Custom("https://registry.example.com".to_string()),
		CiProviderKind::GitHubActions,
	);

	assert!(!custom.trusted_publishing);
	assert!(!custom.ci_identity_verifiable);
	assert!(!custom.registry_setup_verifiable);
	assert!(
		custom
			.notes
			.iter()
			.any(|note| note.contains("custom/private"))
	);
}

#[test]
fn diagnostics_report_unsupported_and_unknown_contexts() {
	let circle_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Npm),
		&TrustedPublishingIdentity::CircleCi {
			project_slug: Some("gh/monochange/monochange".to_string()),
			workflow_id: Some("workflow".to_string()),
			job_name: Some("publish".to_string()),
		},
	);
	assert!(circle_message.contains("CircleCI is not supported for npm trusted publishing"));
	assert!(circle_message.contains("GitHub Actions, GitLab CI/CD"));

	let unknown_identity = TrustedPublishingIdentity::Unknown {
		reason: "local shell".to_string(),
	};
	assert!(!unknown_identity.is_verifiable_by_env());
	let unknown_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Pypi),
		&unknown_identity,
	);
	assert!(unknown_message.contains("No supported CI provider identity"));
	assert!(unknown_message.contains("Google Cloud Build"));
}

#[test]
fn diagnostics_report_incomplete_and_supported_context_capabilities() {
	let incomplete_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Npm),
		&TrustedPublishingIdentity::GitHubActions {
			repository: None,
			workflow: Some("publish.yml".to_string()),
			workflow_ref: None,
			environment: None,
			ref_name: None,
			run_id: None,
		},
	);
	assert!(incomplete_message.contains("publish-time environment variables are incomplete"));

	let complete_github_identity = TrustedPublishingIdentity::GitHubActions {
		repository: Some("monochange/monochange".to_string()),
		workflow: Some("publish.yml".to_string()),
		workflow_ref: None,
		environment: Some("publisher".to_string()),
		ref_name: Some("main".to_string()),
		run_id: Some("123".to_string()),
	};
	let npm_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::Npm),
		&complete_github_identity,
	);
	assert!(npm_message.contains("monochange can verify registry-side setup"));
	assert!(npm_message.contains("registry-native provenance is available"));

	let crates_message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::CratesIo),
		&complete_github_identity,
	);
	assert!(crates_message.contains("registry-side setup verification is manual"));
	assert!(crates_message.contains("registry-native provenance is not available"));
}

#[test]
fn unsupported_builtin_registries_have_no_trusted_publishing_capabilities() {
	let goproxy = builtin_registry_trust_capabilities(RegistryKind::GoProxy);

	assert_eq!(goproxy.registry, "go_proxy");
	assert!(!goproxy.trusted_publishing);
	assert!(goproxy.supported_providers.is_empty());
	assert_eq!(goproxy.setup_url, None);
	assert_eq!(
		goproxy.notes,
		vec!["unknown registry capabilities are treated as unsupported".to_string()]
	);

	let message = trusted_publishing_capability_message(
		&PublishRegistry::Builtin(RegistryKind::GoProxy),
		&TrustedPublishingIdentity::Unknown {
			reason: "local shell".to_string(),
		},
	);
	assert!(message.contains("supported providers: none"));
}

fn publication_target(package: &str, ecosystem: Ecosystem) -> PackagePublicationTarget {
	PackagePublicationTarget {
		package: package.to_string(),
		ecosystem,
		registry: None,
		version: "1.0.0".to_string(),
		mode: PublishMode::default(),
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
	}
}

fn group_definition(id: &str, packages: &[&str]) -> GroupDefinition {
	GroupDefinition {
		id: id.to_string(),
		packages: packages
			.iter()
			.map(|package| (*package).to_string())
			.collect(),
		changelog: None,
		changelog_include: GroupChangelogInclude::default(),
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: VersionFormat::default(),
	}
}

#[test]
fn select_release_publication_targets_filters_ecosystems_and_expands_groups() {
	let groups = vec![
		group_definition("frontend", &["web", "ui"]),
		group_definition("docs", &["site"]),
	];
	let publication_targets = vec![
		publication_target("web", Ecosystem::Npm),
		publication_target("cli", Ecosystem::Cargo),
		publication_target("docs", Ecosystem::Python),
	];
	let selected_packages = BTreeSet::from(["manual".to_string()]);
	let selected_groups = BTreeSet::from(["frontend".to_string(), "missing".to_string()]);
	let selected_ecosystems = BTreeSet::from([Ecosystem::Npm, Ecosystem::Cargo]);

	let selected = select_release_publication_targets(
		&groups,
		&publication_targets,
		&selected_packages,
		&selected_groups,
		&selected_ecosystems,
	);

	assert_eq!(selected.publication_targets.len(), 2);
	assert_eq!(selected.publication_targets[0].package, "web");
	assert_eq!(selected.publication_targets[1].package, "cli");
	assert_eq!(
		selected.selected_packages,
		BTreeSet::from(["manual".to_string(), "web".to_string(), "ui".to_string()])
	);
}

fn sample_publish_request_for_registry(registry: RegistryKind) -> PublishRequest {
	PublishRequest {
		package_id: "pkg".to_string(),
		package_name: "pkg".to_string(),
		ecosystem: Ecosystem::Npm,
		manifest_path: PathBuf::from("package.json"),
		package_root: PathBuf::from("."),
		registry,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		placeholder_readme: "placeholder".to_string(),
	}
}

#[test]
fn publish_readiness_registry_push_checker_and_missing_checker_paths() {
	let request = sample_publish_request_for_registry(RegistryKind::Npm);
	let root = Path::new(".");
	let mut registry = PublishReadinessRegistry::new();

	assert_eq!(registry.blocked_message(root, &request).unwrap(), None);

	registry.push_checker(
		RegistryKind::Npm,
		Box::new(|_, request| Ok(Some(format!("{} blocked", request.package_name)))),
	);

	assert_eq!(
		registry.blocked_message(root, &request).unwrap().as_deref(),
		Some("pkg blocked")
	);
}

#[test]
fn placeholder_manifest_registry_push_writer_and_directory_builder_write_files() {
	let request = sample_publish_request_for_registry(RegistryKind::Npm);
	let root = Path::new(".");
	let mut registry = PlaceholderManifestWriterRegistry::new();

	registry.push_writer(
		RegistryKind::Npm,
		Box::new(|placeholder_dir, request, _, _| {
			fs::write(
				placeholder_dir.join("package.json"),
				format!("{{\"name\":\"{}\"}}", request.package_name),
			)
			.map_err(|error| MonochangeError::Io(error.to_string()))
		}),
	);

	let tempdir = build_placeholder_directory(root, &request, None, &registry).unwrap();

	assert_eq!(
		fs::read_to_string(tempdir.path().join("README.md")).unwrap(),
		"placeholder"
	);
	assert_eq!(
		fs::read_to_string(tempdir.path().join("package.json")).unwrap(),
		"{\"name\":\"pkg\"}"
	);
}

#[test]
fn default_registry_kind_for_ecosystem_reports_unknown_and_known_ecosystems() {
	let unknown = default_registry_kind_for_ecosystem("unknown").unwrap_err();
	assert!(unknown.to_string().contains("ecosystem `unknown`"));

	assert_eq!(
		default_registry_kind_for_ecosystem("go").unwrap(),
		RegistryKind::GoProxy
	);
}

#[test]
fn placeholder_tempdir_error_includes_io_error() {
	let error = std::io::Error::other("no tempdir");

	assert!(
		placeholder_tempdir_error(&error)
			.to_string()
			.contains("failed to create placeholder tempdir: no tempdir")
	);
}

#[test]
fn publish_dependency_order_handles_realistic_cargo_dependency_graph() {
	let schema = publish_order_package("schema");

	let mut codegen = publish_order_package("codegen");
	codegen
		.declared_dependencies
		.push(publish_order_dependency("schema", DependencyKind::Runtime));

	let mut test_helpers = publish_order_package("test_helpers");
	test_helpers
		.declared_dependencies
		.push(publish_order_dependency("schema", DependencyKind::Runtime));

	let mut core = publish_order_package("core");
	core.declared_dependencies
		.push(publish_order_dependency("schema", DependencyKind::Build));
	core.declared_dependencies.push(publish_order_dependency(
		"test_helpers",
		DependencyKind::Development,
	));

	let mut cli = publish_order_package("cli");
	cli.declared_dependencies
		.push(publish_order_dependency("core", DependencyKind::Runtime));
	cli.declared_dependencies
		.push(publish_order_dependency("codegen", DependencyKind::Build));
	cli.declared_dependencies.push(publish_order_dependency(
		"test_helpers",
		DependencyKind::Development,
	));

	let ordered = order_release_requests_by_publish_dependencies(
		&[cli, core, test_helpers, codegen, schema],
		vec![
			publish_order_request("cli"),
			publish_order_request("core"),
			publish_order_request("test_helpers"),
			publish_order_request("codegen"),
			publish_order_request("schema"),
		],
	)
	.unwrap_or_else(|error| panic!("publish requests should be ordered: {error}"));
	let ordered_package_ids = ordered
		.iter()
		.map(|request| request.package_id.as_str())
		.collect::<Vec<_>>();

	assert_eq!(
		ordered_package_ids,
		vec!["schema", "codegen", "test_helpers", "core", "cli"]
	);
}

#[test]
fn publish_dependency_order_reports_development_dependency_cycles() {
	let mut app = publish_order_package("app");
	app.declared_dependencies.push(publish_order_dependency(
		"helper",
		DependencyKind::Development,
	));
	let mut helper = publish_order_package("helper");
	helper
		.declared_dependencies
		.push(publish_order_dependency("app", DependencyKind::Development));

	let error = order_release_requests_by_publish_dependencies(
		&[app, helper],
		vec![
			publish_order_request("app"),
			publish_order_request("helper"),
		],
	)
	.expect_err("development dependency cycle should be rejected");

	assert!(
		error
			.to_string()
			.contains("cyclic publish dependencies detected")
	);
}

fn publish_order_package(name: &str) -> PackageRecord {
	let root = PathBuf::from("/workspace");
	let mut package = PackageRecord::new(
		Ecosystem::Cargo,
		name,
		root.join(name).join("Cargo.toml"),
		root,
		None,
		PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), name.to_string());
	package
}

fn publish_order_dependency(name: &str, kind: DependencyKind) -> PackageDependency {
	PackageDependency {
		name: name.to_string(),
		kind,
		version_constraint: Some("1.0.0".to_string()),
		optional: false,
	}
}

fn publish_order_request(package: &str) -> PublishRequest {
	PublishRequest {
		package_id: package.to_string(),
		package_name: package.to_string(),
		ecosystem: Ecosystem::Cargo,
		manifest_path: PathBuf::from(format!("/workspace/{package}/Cargo.toml")),
		package_root: PathBuf::from(format!("/workspace/{package}")),
		registry: RegistryKind::CratesIo,
		package_manager: None,
		package_metadata: BTreeMap::new(),
		mode: PublishMode::Builtin,
		version: "1.0.0".to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		placeholder_readme: String::new(),
	}
}
