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
