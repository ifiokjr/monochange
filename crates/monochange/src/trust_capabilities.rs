use std::collections::BTreeMap;

use monochange_core::PublishRegistry;
use monochange_core::RegistryKind;
use serde::Deserialize;
use serde::Serialize;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CiProviderKind {
	GitHubActions,
	GitLabCi,
	CircleCi,
	GoogleCloudBuild,
	Unknown,
}

impl CiProviderKind {
	pub(crate) fn label(self) -> &'static str {
		match self {
			Self::GitHubActions => "GitHub Actions",
			Self::GitLabCi => "GitLab CI/CD",
			Self::CircleCi => "CircleCI",
			Self::GoogleCloudBuild => "Google Cloud Build",
			Self::Unknown => "unknown CI provider",
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "provider", rename_all = "snake_case")]
pub(crate) enum TrustedPublishingIdentity {
	GitHubActions {
		repository: Option<String>,
		workflow: Option<String>,
		workflow_ref: Option<String>,
		environment: Option<String>,
		ref_name: Option<String>,
		run_id: Option<String>,
	},
	GitLabCi {
		project_path: Option<String>,
		ref_name: Option<String>,
		pipeline_source: Option<String>,
		job_id: Option<String>,
	},
	CircleCi {
		project_slug: Option<String>,
		workflow_id: Option<String>,
		job_name: Option<String>,
	},
	GoogleCloudBuild {
		project_id: Option<String>,
		build_id: Option<String>,
		trigger_name: Option<String>,
		repository: Option<String>,
		ref_name: Option<String>,
	},
	Unknown {
		reason: String,
	},
}

impl TrustedPublishingIdentity {
	pub(crate) fn provider(&self) -> CiProviderKind {
		match self {
			Self::GitHubActions { .. } => CiProviderKind::GitHubActions,
			Self::GitLabCi { .. } => CiProviderKind::GitLabCi,
			Self::CircleCi { .. } => CiProviderKind::CircleCi,
			Self::GoogleCloudBuild { .. } => CiProviderKind::GoogleCloudBuild,
			Self::Unknown { .. } => CiProviderKind::Unknown,
		}
	}

	pub(crate) fn is_verifiable_by_env(&self) -> bool {
		match self {
			Self::GitHubActions {
				repository,
				workflow,
				workflow_ref,
				..
			} => repository.is_some() && (workflow.is_some() || workflow_ref.is_some()),
			Self::GitLabCi {
				project_path,
				job_id,
				..
			} => project_path.is_some() && job_id.is_some(),
			Self::CircleCi {
				project_slug,
				workflow_id,
				..
			} => project_slug.is_some() && workflow_id.is_some(),
			Self::GoogleCloudBuild {
				project_id,
				build_id,
				..
			} => project_id.is_some() && build_id.is_some(),
			Self::Unknown { .. } => false,
		}
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
	clippy::struct_excessive_bools,
	reason = "capability matrix reports independent registry booleans"
)]
pub(crate) struct RegistryTrustCapabilities {
	pub(crate) registry: String,
	pub(crate) trusted_publishing: bool,
	pub(crate) supported_providers: Vec<CiProviderKind>,
	pub(crate) registry_setup_verifiable: bool,
	pub(crate) registry_setup_automation: bool,
	pub(crate) registry_native_provenance: bool,
	pub(crate) setup_url: Option<String>,
	pub(crate) notes: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
	clippy::struct_excessive_bools,
	reason = "capability matrix reports independent provider/registry booleans"
)]
pub(crate) struct ProviderRegistryTrustCapability {
	pub(crate) registry: String,
	pub(crate) provider: CiProviderKind,
	pub(crate) trusted_publishing: bool,
	pub(crate) ci_identity_verifiable: bool,
	pub(crate) registry_setup_verifiable: bool,
	pub(crate) registry_setup_automation: bool,
	pub(crate) registry_native_provenance: bool,
	pub(crate) notes: Vec<String>,
}

pub(crate) fn detect_trusted_publishing_identity(
	env_map: &BTreeMap<String, String>,
) -> TrustedPublishingIdentity {
	if env_is_true(env_map, "GITHUB_ACTIONS") || env_map.contains_key("GITHUB_WORKFLOW_REF") {
		return TrustedPublishingIdentity::GitHubActions {
			repository: env_map.get("GITHUB_REPOSITORY").cloned(),
			workflow: env_map
				.get("GITHUB_WORKFLOW_REF")
				.and_then(|value| parse_github_workflow_ref(value))
				.or_else(|| env_map.get("GITHUB_WORKFLOW").cloned()),
			workflow_ref: env_map.get("GITHUB_WORKFLOW_REF").cloned(),
			environment: env_map
				.get("GITHUB_ENVIRONMENT")
				.or_else(|| env_map.get("MONOCHANGE_TRUSTED_PUBLISHING_ENVIRONMENT"))
				.cloned(),
			ref_name: env_map.get("GITHUB_REF_NAME").cloned(),
			run_id: env_map.get("GITHUB_RUN_ID").cloned(),
		};
	}

	if env_is_true(env_map, "GITLAB_CI") {
		return TrustedPublishingIdentity::GitLabCi {
			project_path: env_map.get("CI_PROJECT_PATH").cloned(),
			ref_name: env_map.get("CI_COMMIT_REF_NAME").cloned(),
			pipeline_source: env_map.get("CI_PIPELINE_SOURCE").cloned(),
			job_id: env_map.get("CI_JOB_ID").cloned(),
		};
	}

	if env_is_true(env_map, "CIRCLECI") {
		return TrustedPublishingIdentity::CircleCi {
			project_slug: circle_project_slug(env_map),
			workflow_id: env_map.get("CIRCLE_WORKFLOW_ID").cloned(),
			job_name: env_map.get("CIRCLE_JOB").cloned(),
		};
	}

	if env_map.contains_key("BUILD_ID")
		&& (env_map.contains_key("PROJECT_ID") || env_map.contains_key("GOOGLE_CLOUD_PROJECT"))
	{
		return TrustedPublishingIdentity::GoogleCloudBuild {
			project_id: env_map
				.get("PROJECT_ID")
				.or_else(|| env_map.get("GOOGLE_CLOUD_PROJECT"))
				.cloned(),
			build_id: env_map.get("BUILD_ID").cloned(),
			trigger_name: env_map.get("TRIGGER_NAME").cloned(),
			repository: env_map.get("REPO_NAME").cloned(),
			ref_name: env_map
				.get("BRANCH_NAME")
				.or_else(|| env_map.get("TAG_NAME"))
				.cloned(),
		};
	}

	TrustedPublishingIdentity::Unknown {
		reason: "no supported CI provider environment variables were detected".to_string(),
	}
}

pub(crate) fn registry_trust_capabilities(registry: &PublishRegistry) -> RegistryTrustCapabilities {
	match registry {
		PublishRegistry::Builtin(registry) => builtin_registry_trust_capabilities(*registry),
		PublishRegistry::Custom(name) => {
			RegistryTrustCapabilities {
				registry: name.clone(),
				trusted_publishing: false,
				supported_providers: Vec::new(),
				registry_setup_verifiable: false,
				registry_setup_automation: false,
				registry_native_provenance: false,
				setup_url: None,
				notes: vec![
				"custom/private registries have no built-in trusted-publishing contract in monochange"
					.to_string(),
			],
			}
		}
	}
}

pub(crate) fn builtin_registry_trust_capabilities(
	registry: RegistryKind,
) -> RegistryTrustCapabilities {
	let providers = supported_providers_for_registry(registry);
	RegistryTrustCapabilities {
		registry: registry.to_string(),
		trusted_publishing: !providers.is_empty(),
		supported_providers: providers,
		registry_setup_verifiable: registry == RegistryKind::Npm,
		registry_setup_automation: registry == RegistryKind::Npm,
		registry_native_provenance: matches!(
			registry,
			RegistryKind::Npm | RegistryKind::Jsr | RegistryKind::Pypi
		),
		setup_url: registry_setup_url(registry).map(str::to_string),
		notes: registry_notes(registry),
	}
}

pub(crate) fn provider_registry_trust_capability(
	registry: &PublishRegistry,
	provider: CiProviderKind,
) -> ProviderRegistryTrustCapability {
	let registry_capabilities = registry_trust_capabilities(registry);
	let supported = registry_capabilities
		.supported_providers
		.contains(&provider);
	let builtin = match registry {
		PublishRegistry::Builtin(registry) => Some(*registry),
		PublishRegistry::Custom(_) => None,
	};
	let registry_setup_verifiable = supported
		&& builtin == Some(RegistryKind::Npm)
		&& provider == CiProviderKind::GitHubActions;
	let registry_setup_automation = registry_setup_verifiable;
	let mut notes = registry_capabilities.notes.clone();

	if !supported {
		notes.push(format!(
			"{} is not a supported trusted-publishing provider for {}",
			provider.label(),
			registry_capabilities.registry
		));
	} else if !registry_setup_verifiable {
		notes.push(format!(
			"monochange can identify {} context for {}, but registry-side setup still requires manual review",
			provider.label(),
			registry_capabilities.registry
		));
	}

	ProviderRegistryTrustCapability {
		registry: registry_capabilities.registry,
		provider,
		trusted_publishing: supported,
		ci_identity_verifiable: supported && provider != CiProviderKind::Unknown,
		registry_setup_verifiable,
		registry_setup_automation,
		registry_native_provenance: supported && registry_capabilities.registry_native_provenance,
		notes,
	}
}

#[cfg(test)]
pub(crate) fn builtin_provider_registry_trust_capability(
	registry: RegistryKind,
	provider: CiProviderKind,
) -> ProviderRegistryTrustCapability {
	provider_registry_trust_capability(&PublishRegistry::Builtin(registry), provider)
}

pub(crate) fn trusted_publishing_capability_message_for_builtin(
	registry: RegistryKind,
	env_map: &BTreeMap<String, String>,
) -> String {
	let identity = detect_trusted_publishing_identity(env_map);
	trusted_publishing_capability_message(&PublishRegistry::Builtin(registry), &identity)
}

pub(crate) fn trusted_publishing_capability_message(
	registry: &PublishRegistry,
	identity: &TrustedPublishingIdentity,
) -> String {
	let provider = identity.provider();
	let capability = provider_registry_trust_capability(registry, provider);
	let registry_capabilities = registry_trust_capabilities(registry);
	let supported_providers = provider_list(&registry_capabilities.supported_providers);

	if provider == CiProviderKind::Unknown {
		return format!(
			"No supported CI provider identity was detected for {} trusted publishing; supported providers: {}.",
			registry_capabilities.registry, supported_providers
		);
	}

	if !capability.trusted_publishing {
		return format!(
			"Current CI provider {} is not supported for {} trusted publishing; supported providers: {}.",
			provider.label(),
			capability.registry,
			supported_providers
		);
	}

	if !identity.is_verifiable_by_env() {
		return format!(
			"Current CI provider {} is supported for {} trusted publishing, but publish-time environment variables are incomplete; verify the registry publisher configuration manually.",
			provider.label(),
			capability.registry
		);
	}

	let registry_setup = if capability.registry_setup_verifiable {
		"monochange can verify registry-side setup"
	} else {
		"registry-side setup verification is manual"
	};
	let provenance = if capability.registry_native_provenance {
		"registry-native provenance is available"
	} else {
		"registry-native provenance is not available"
	};

	format!(
		"Current CI provider {} is supported for {} trusted publishing; {registry_setup}; {provenance}.",
		provider.label(),
		capability.registry
	)
}

fn supported_providers_for_registry(registry: RegistryKind) -> Vec<CiProviderKind> {
	match registry {
		RegistryKind::Npm => vec![CiProviderKind::GitHubActions, CiProviderKind::GitLabCi],
		RegistryKind::CratesIo | RegistryKind::Jsr => vec![CiProviderKind::GitHubActions],
		RegistryKind::PubDev => {
			vec![
				CiProviderKind::GitHubActions,
				CiProviderKind::GoogleCloudBuild,
			]
		}
		RegistryKind::Pypi => {
			vec![
				CiProviderKind::GitHubActions,
				CiProviderKind::GitLabCi,
				CiProviderKind::GoogleCloudBuild,
			]
		}
		_ => Vec::new(),
	}
}

fn registry_setup_url(registry: RegistryKind) -> Option<&'static str> {
	match registry {
		RegistryKind::Npm => Some("https://docs.npmjs.com/trusted-publishers"),
		RegistryKind::CratesIo => Some("https://crates.io/docs/trusted-publishing"),
		RegistryKind::Jsr => Some("https://jsr.io/docs/publishing-packages"),
		RegistryKind::PubDev => Some("https://dart.dev/tools/pub/automated-publishing"),
		RegistryKind::Pypi => Some("https://docs.pypi.org/trusted-publishers/"),
		_ => None,
	}
}

fn registry_notes(registry: RegistryKind) -> Vec<String> {
	match registry {
		RegistryKind::Npm => vec![
			"npm trusted publishing supports GitHub Actions and GitLab CI/CD".to_string(),
			"monochange can verify and automate npm GitHub trusted-publisher setup with npm CLI trust commands".to_string(),
		],
		RegistryKind::CratesIo => vec![
			"crates.io trusted publishing uses OIDC short-lived tokens".to_string(),
			"monochange does not currently verify crates.io registry-side trusted-publisher setup".to_string(),
		],
		RegistryKind::Jsr => vec![
			"JSR can publish from supported CI without long-lived tokens".to_string(),
			"JSR package provenance is available, but monochange does not verify registry-side setup".to_string(),
		],
		RegistryKind::PubDev => vec![
			"pub.dev automated publishing uses configured OIDC publishers".to_string(),
			"pub.dev registry-side publisher setup requires manual review".to_string(),
		],
		RegistryKind::Pypi => vec![
			"PyPI Trusted Publishers support multiple CI identity providers".to_string(),
			"PEP 740 digital attestations are separate from trusted-publisher authorization".to_string(),
		],
		_ => vec!["unknown registry capabilities are treated as unsupported".to_string()],
	}
}

fn provider_list(providers: &[CiProviderKind]) -> String {
	if providers.is_empty() {
		return "none".to_string();
	}

	providers
		.iter()
		.map(|provider| provider.label())
		.collect::<Vec<_>>()
		.join(", ")
}

fn env_is_true(env_map: &BTreeMap<String, String>, key: &str) -> bool {
	env_map.get(key).is_some_and(|value| value == "true")
}

fn parse_github_workflow_ref(value: &str) -> Option<String> {
	let workflow_path = value.split('@').next()?;
	workflow_path
		.rsplit_once("/.github/workflows/")
		.map(|(_, workflow)| workflow.to_string())
}

fn circle_project_slug(env_map: &BTreeMap<String, String>) -> Option<String> {
	env_map
		.get("CIRCLE_PROJECT_USERNAME")
		.zip(env_map.get("CIRCLE_PROJECT_REPONAME"))
		.map(|(owner, repo)| format!("gh/{owner}/{repo}"))
		.or_else(|| env_map.get("CIRCLE_PROJECT_REPONAME").cloned())
}

#[cfg(test)]
mod tests {
	use super::*;

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
}
