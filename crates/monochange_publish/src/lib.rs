use std::collections::BTreeMap;
use std::env;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::RegistryKind;
use monochange_core::TrustedPublishingSettings;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use urlencoding::encode;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePublishRunMode {
	Placeholder,
	Release,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PackagePublishStatus {
	Planned,
	Published,
	SkippedExisting,
	SkippedExternal,
	Blocked,
	Failed,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustedPublishingStatus {
	Disabled,
	Planned,
	Configured,
	ManualActionRequired,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrustedPublishingOutcome {
	pub status: TrustedPublishingStatus,
	pub repository: Option<String>,
	pub workflow: Option<String>,
	pub environment: Option<String>,
	pub setup_url: Option<String>,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagePublishOutcome {
	pub package: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
	pub version: String,
	pub status: PackagePublishStatus,
	pub message: String,
	pub placeholder: bool,
	pub trusted_publishing: TrustedPublishingOutcome,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub command: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stdout: Option<String>,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub stderr: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PackagePublishReport {
	pub mode: PackagePublishRunMode,
	pub dry_run: bool,
	pub packages: Vec<PackagePublishOutcome>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct PublishRequest {
	pub package_id: String,
	pub package_name: String,
	pub ecosystem: Ecosystem,
	pub manifest_path: PathBuf,
	pub package_root: PathBuf,
	pub registry: RegistryKind,
	pub package_manager: Option<String>,
	pub package_metadata: BTreeMap<String, String>,
	pub mode: PublishMode,
	pub version: String,
	pub placeholder: bool,
	pub trusted_publishing: TrustedPublishingSettings,
	pub attestations: PublishAttestationSettings,
	pub placeholder_readme: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandSpec {
	pub program: String,
	pub args: Vec<String>,
	pub cwd: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CommandOutput {
	pub success: bool,
	pub stdout: String,
	pub stderr: String,
}

pub trait CommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput>;
}

pub struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		use std::process::Command;
		let mut command = Command::new(&spec.program);
		command.args(&spec.args).current_dir(&spec.cwd);
		let output = command.output().map_err(|error| {
			MonochangeError::Io(format!(
				"failed to run `{}` in {}: {error}",
				render_command(spec),
				spec.cwd.display()
			))
		})?;
		Ok(CommandOutput {
			success: output.status.success(),
			stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
			stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
		})
	}
}

pub fn render_command(spec: &CommandSpec) -> String {
	std::iter::once(spec.program.as_str())
		.chain(spec.args.iter().map(String::as_str))
		.collect::<Vec<_>>()
		.join(" ")
}

pub fn render_command_error(output: &CommandOutput) -> String {
	if output.stderr.is_empty() {
		"command failed".to_string()
	} else {
		output.stderr.clone()
	}
}

pub fn build_publish_command(
	request: &PublishRequest,
	mode: PackagePublishRunMode,
	placeholder_dir: Option<&Path>,
	dry_run: bool,
) -> CommandSpec {
	let mut command = None;
	let is_jsr_release =
		request.registry == RegistryKind::Jsr && mode == PackagePublishRunMode::Release;
	let placeholder_path = placeholder_dir;
	if request.registry == RegistryKind::Npm && mode == PackagePublishRunMode::Placeholder {
		command = Some(build_npm_placeholder_publish_command(
			request,
			placeholder_path.expect("placeholder directory must exist"),
		));
	} else if request.registry == RegistryKind::Npm && mode == PackagePublishRunMode::Release {
		command = Some(build_npm_release_publish_command(request));
	} else if request.registry == RegistryKind::CratesIo
		&& mode == PackagePublishRunMode::Placeholder
	{
		command = Some(build_cargo_placeholder_publish_command(
			request,
			placeholder_path.expect("placeholder directory must exist"),
		));
	} else if request.registry == RegistryKind::CratesIo && mode == PackagePublishRunMode::Release {
		command = Some(build_cargo_release_publish_command(request));
	} else if request.registry == RegistryKind::PubDev && mode == PackagePublishRunMode::Placeholder
	{
		command = Some(build_dart_publish_command(
			request,
			placeholder_path.expect("placeholder directory must exist"),
		));
	} else if request.registry == RegistryKind::PubDev && mode == PackagePublishRunMode::Release {
		command = Some(build_dart_publish_command(request, &request.package_root));
	} else if request.registry == RegistryKind::Jsr && mode == PackagePublishRunMode::Placeholder {
		command = Some(build_jsr_publish_command(
			placeholder_path.expect("placeholder directory must exist"),
		));
	} else if request.registry == RegistryKind::Pypi && mode == PackagePublishRunMode::Placeholder {
		command = Some(build_python_publish_command(
			request,
			placeholder_path.expect("placeholder directory must exist"),
		));
	} else if request.registry == RegistryKind::Pypi && mode == PackagePublishRunMode::Release {
		command = Some(build_python_publish_command(request, &request.package_root));
	} else if request.registry == RegistryKind::GoProxy {
		command = Some(build_go_publish_command(request));
	}
	if is_jsr_release {
		command = Some(build_jsr_publish_command(&request.package_root));
	}

	let mut command = command.expect("unsupported built-in publish registry");
	append_publish_dry_run_args(&mut command.args, request.registry, dry_run);
	command
}

pub fn append_publish_dry_run_args(args: &mut Vec<String>, registry: RegistryKind, dry_run: bool) {
	if !dry_run {
		return;
	}

	if registry == RegistryKind::Pypi || registry == RegistryKind::GoProxy {
		return;
	}

	if registry == RegistryKind::PubDev {
		args.retain(|arg| arg != "--force");
		args.push("--dry-run".to_string());
		return;
	}

	args.push("--dry-run".to_string());
}

pub fn build_npm_placeholder_publish_command(
	request: &PublishRequest,
	placeholder_path: &Path,
) -> CommandSpec {
	CommandSpec {
		program: npm_publish_program(request).to_string(),
		args: vec![
			"publish".to_string(),
			placeholder_path.display().to_string(),
			"--access".to_string(),
			"public".to_string(),
		],
		cwd: request.package_root.clone(),
	}
}

pub fn build_npm_release_publish_command(request: &PublishRequest) -> CommandSpec {
	let mut args = vec![
		"publish".to_string(),
		"--access".to_string(),
		"public".to_string(),
	];
	if request.attestations.require_registry_provenance {
		args.push("--provenance".to_string());
	}
	CommandSpec {
		program: npm_publish_program(request).to_string(),
		args,
		cwd: request.package_root.clone(),
	}
}

fn npm_publish_program(request: &PublishRequest) -> &'static str {
	if request.trusted_publishing.enabled {
		return "npm";
	}

	if uses_pnpm_publish_manager(request) {
		"pnpm"
	} else {
		"npm"
	}
}

pub fn uses_pnpm_publish_manager(request: &PublishRequest) -> bool {
	request.registry == RegistryKind::Npm && request.package_manager.as_deref() == Some("pnpm")
}

fn build_cargo_placeholder_publish_command(
	request: &PublishRequest,
	placeholder_path: &Path,
) -> CommandSpec {
	CommandSpec {
		program: "cargo".to_string(),
		args: vec![
			"publish".to_string(),
			"--allow-dirty".to_string(),
			"--manifest-path".to_string(),
			placeholder_path.join("Cargo.toml").display().to_string(),
		],
		cwd: request.package_root.clone(),
	}
}

fn build_cargo_release_publish_command(request: &PublishRequest) -> CommandSpec {
	CommandSpec {
		program: "cargo".to_string(),
		args: vec![
			"publish".to_string(),
			"--locked".to_string(),
			"--manifest-path".to_string(),
			request.manifest_path.display().to_string(),
		],
		cwd: request.package_root.clone(),
	}
}

fn build_dart_publish_command(request: &PublishRequest, cwd: &Path) -> CommandSpec {
	let program = if request.ecosystem == Ecosystem::Flutter {
		"flutter"
	} else {
		"dart"
	};
	CommandSpec {
		program: program.to_string(),
		args: vec![
			"pub".to_string(),
			"publish".to_string(),
			"--force".to_string(),
		],
		cwd: cwd.to_path_buf(),
	}
}

fn build_jsr_publish_command(cwd: &Path) -> CommandSpec {
	CommandSpec {
		program: "deno".to_string(),
		args: vec!["publish".to_string()],
		cwd: cwd.to_path_buf(),
	}
}

fn build_python_publish_command(request: &PublishRequest, cwd: &Path) -> CommandSpec {
	let trusted_publishing = if request.trusted_publishing.enabled {
		"always"
	} else {
		"never"
	};
	let script = format!(
		"uv build --out-dir dist && uv publish --trusted-publishing {trusted_publishing} dist/*"
	);
	CommandSpec {
		program: "sh".to_string(),
		args: vec!["-c".to_string(), script],
		cwd: cwd.to_path_buf(),
	}
}

fn build_go_publish_command(request: &PublishRequest) -> CommandSpec {
	CommandSpec {
		program: "git".to_string(),
		args: vec!["tag".to_string(), go_module_tag_name(request)],
		cwd: request.package_root.clone(),
	}
}

fn go_module_tag_name(request: &PublishRequest) -> String {
	let version = go_proxy_version(&request.version);
	let root = request
		.package_metadata
		.get("relative_path")
		.cloned()
		.unwrap_or_else(|| fallback_go_tag_prefix(request));
	let root = root.trim_matches('/');
	if root.is_empty() || root == "." {
		return version;
	}
	format!("{root}/{version}")
}

fn fallback_go_tag_prefix(request: &PublishRequest) -> String {
	env::current_dir()
		.ok()
		.and_then(|root| {
			request
				.package_root
				.strip_prefix(root)
				.ok()
				.map(Path::to_path_buf)
		})
		.unwrap_or_else(|| request.package_root.clone())
		.to_string_lossy()
		.to_string()
}

pub fn go_module_path(request: &PublishRequest) -> &str {
	request
		.package_metadata
		.get("module_path")
		.map_or(request.package_name.as_str(), String::as_str)
}

pub fn go_proxy_version(version: &str) -> String {
	if version.starts_with('v') {
		version.to_string()
	} else {
		format!("v{version}")
	}
}

pub fn go_proxy_module_path(module: &str) -> String {
	let mut escaped = String::with_capacity(module.len());
	for character in module.chars() {
		if character.is_ascii_uppercase() {
			escaped.push('!');
			escaped.push(character.to_ascii_lowercase());
		} else {
			escaped.push(character);
		}
	}
	escaped
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CiProviderKind {
	GitHubActions,
	GitLabCi,
	CircleCi,
	GoogleCloudBuild,
	Unknown,
}

impl CiProviderKind {
	pub fn label(self) -> &'static str {
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
pub enum TrustedPublishingIdentity {
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
	pub fn provider(&self) -> CiProviderKind {
		match self {
			Self::GitHubActions { .. } => CiProviderKind::GitHubActions,
			Self::GitLabCi { .. } => CiProviderKind::GitLabCi,
			Self::CircleCi { .. } => CiProviderKind::CircleCi,
			Self::GoogleCloudBuild { .. } => CiProviderKind::GoogleCloudBuild,
			Self::Unknown { .. } => CiProviderKind::Unknown,
		}
	}

	pub fn is_verifiable_by_env(&self) -> bool {
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
pub struct RegistryTrustCapabilities {
	pub registry: String,
	pub trusted_publishing: bool,
	pub supported_providers: Vec<CiProviderKind>,
	pub registry_setup_verifiable: bool,
	pub registry_setup_automation: bool,
	pub registry_native_provenance: bool,
	pub setup_url: Option<String>,
	pub notes: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(
	clippy::struct_excessive_bools,
	reason = "capability matrix reports independent provider/registry booleans"
)]
pub struct ProviderRegistryTrustCapability {
	pub registry: String,
	pub provider: CiProviderKind,
	pub trusted_publishing: bool,
	pub ci_identity_verifiable: bool,
	pub registry_setup_verifiable: bool,
	pub registry_setup_automation: bool,
	pub registry_native_provenance: bool,
	pub notes: Vec<String>,
}

pub fn detect_trusted_publishing_identity(
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

pub fn registry_trust_capabilities(registry: &PublishRegistry) -> RegistryTrustCapabilities {
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

pub fn builtin_registry_trust_capabilities(registry: RegistryKind) -> RegistryTrustCapabilities {
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

pub fn provider_registry_trust_capability(
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

pub fn trusted_publishing_capability_message_for_builtin(
	registry: RegistryKind,
	env_map: &BTreeMap<String, String>,
) -> String {
	let identity = detect_trusted_publishing_identity(env_map);
	trusted_publishing_capability_message(&PublishRegistry::Builtin(registry), &identity)
}

pub fn trusted_publishing_capability_message(
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

use std::collections::BTreeSet;
use std::fs;

use monochange_core::DependencyKind;
use monochange_core::materialize_dependency_edges;

pub fn read_publish_report_artifact(path: &Path) -> MonochangeResult<PackagePublishReport> {
	let body = fs::read_to_string(path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read package publish resume artifact {}: {error}",
			path.display()
		))
	})?;
	serde_json::from_str(&body).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse package publish resume artifact {}: {error}",
			path.display()
		))
	})
}

pub fn write_publish_report_artifact(
	path: &Path,
	report: &PackagePublishReport,
) -> MonochangeResult<()> {
	path.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(create_publish_report_directory)
		.transpose()?;
	let body = serde_json::to_string_pretty(report).map_err(publish_report_json_error)?;
	fs::write(path, format!("{body}\n")).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write package publish output {}: {error}",
			path.display()
		))
	})
}

pub fn ensure_publish_report_succeeded(report: &PackagePublishReport) -> MonochangeResult<()> {
	let Some(failed) = report
		.packages
		.iter()
		.find(|outcome| outcome.status == PackagePublishStatus::Failed)
	else {
		return Ok(());
	};

	Err(MonochangeError::Discovery(format!(
		"package publish failed for {} {}: {}",
		failed.package, failed.version, failed.message
	)))
}

pub fn create_publish_report_directory(parent: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create package publish output directory {}: {error}",
			parent.display()
		))
	})
}

pub fn publish_report_json_error(error: impl std::fmt::Display) -> MonochangeError {
	MonochangeError::Config(format!(
		"failed to serialize package publish report: {error}"
	))
}

type PublishResumeKey = (String, String, String);

pub fn resume_publish_requests(
	requests: &[PublishRequest],
	previous_report: Option<&PackagePublishReport>,
) -> MonochangeResult<(Vec<PublishRequest>, Vec<PackagePublishOutcome>)> {
	let Some(previous_report) = previous_report else {
		return Ok((requests.to_vec(), Vec::new()));
	};
	validate_resume_report(previous_report)?;

	let request_keys = requests
		.iter()
		.map(publish_request_resume_key)
		.collect::<BTreeSet<_>>();
	let completed_keys = previous_report
		.packages
		.iter()
		.filter(|outcome| package_publish_status_is_resumable_complete(outcome.status))
		.map(package_publish_outcome_resume_key)
		.collect::<BTreeSet<_>>();
	let resumed_outcomes = previous_report
		.packages
		.iter()
		.filter(|outcome| {
			request_keys.contains(&package_publish_outcome_resume_key(outcome))
				&& package_publish_status_is_resumable_complete(outcome.status)
		})
		.cloned()
		.collect::<Vec<_>>();
	let pending_requests = requests
		.iter()
		.filter(|request| !completed_keys.contains(&publish_request_resume_key(request)))
		.cloned()
		.collect::<Vec<_>>();

	Ok((pending_requests, resumed_outcomes))
}

pub fn validate_resume_report(report: &PackagePublishReport) -> MonochangeResult<()> {
	if report.mode != PackagePublishRunMode::Release {
		return Err(MonochangeError::Config(
			"package publish resume artifact must come from `mc publish`".to_string(),
		));
	}
	if report.dry_run {
		return Err(MonochangeError::Config(
			"package publish resume artifact must come from a real publish run".to_string(),
		));
	}
	Ok(())
}

pub fn publish_request_resume_key(request: &PublishRequest) -> PublishResumeKey {
	(
		request.package_id.clone(),
		request.registry.to_string(),
		request.version.clone(),
	)
}

pub fn package_publish_outcome_resume_key(outcome: &PackagePublishOutcome) -> PublishResumeKey {
	(
		outcome.package.clone(),
		outcome.registry.clone(),
		outcome.version.clone(),
	)
}

pub fn package_publish_status_is_resumable_complete(status: PackagePublishStatus) -> bool {
	matches!(
		status,
		PackagePublishStatus::Published
			| PackagePublishStatus::SkippedExisting
			| PackagePublishStatus::SkippedExternal
	)
}

pub fn merge_publish_resume_report(
	mode: PackagePublishRunMode,
	dry_run: bool,
	mut resumed_outcomes: Vec<PackagePublishOutcome>,
	mut current_report: PackagePublishReport,
) -> PackagePublishReport {
	if resumed_outcomes.is_empty() {
		return current_report;
	}

	resumed_outcomes.append(&mut current_report.packages);
	PackagePublishReport {
		mode,
		dry_run,
		packages: resumed_outcomes,
	}
}

pub fn order_release_requests_by_publish_dependencies(
	packages: &[PackageRecord],
	mut requests: Vec<PublishRequest>,
) -> MonochangeResult<Vec<PublishRequest>> {
	requests.sort_by(|left, right| {
		left.package_id
			.cmp(&right.package_id)
			.then_with(|| left.registry.to_string().cmp(&right.registry.to_string()))
			.then_with(|| left.version.cmp(&right.version))
	});

	let mut requests_by_package = BTreeMap::<String, Vec<PublishRequest>>::new();
	for request in requests {
		requests_by_package
			.entry(request.package_id.clone())
			.or_default()
			.push(request);
	}

	let request_ids = requests_by_package.keys().cloned().collect::<BTreeSet<_>>();
	let config_ids_by_record_id = config_ids_by_package_record_id(packages);
	let mut dependencies_by_package = request_ids
		.iter()
		.map(|package_id| (package_id.clone(), BTreeSet::<String>::new()))
		.collect::<BTreeMap<_, _>>();
	let mut dependents_by_package = BTreeMap::<String, BTreeSet<String>>::new();

	for edge in materialize_dependency_edges(packages) {
		if !publish_dependency_kind_is_ordering_relevant(edge.dependency_kind) {
			continue;
		}
		let Some(from_package_id) = config_ids_by_record_id.get(&edge.from_package_id) else {
			continue;
		};
		let Some(to_package_id) = config_ids_by_record_id.get(&edge.to_package_id) else {
			continue;
		};
		if !request_ids.contains(from_package_id) || !request_ids.contains(to_package_id) {
			continue;
		}

		dependencies_by_package
			.entry(from_package_id.clone())
			.or_default()
			.insert(to_package_id.clone());
		dependents_by_package
			.entry(to_package_id.clone())
			.or_default()
			.insert(from_package_id.clone());
	}

	let mut ready = dependencies_by_package
		.iter()
		.filter(|&(_package_id, dependencies)| dependencies.is_empty())
		.map(|(package_id, _dependencies)| package_id.clone())
		.collect::<BTreeSet<_>>();
	let mut ordered_package_ids = Vec::with_capacity(dependencies_by_package.len());

	while let Some(package_id) = ready.iter().next().cloned() {
		ready.remove(&package_id);
		ordered_package_ids.push(package_id.clone());
		dependencies_by_package.remove(&package_id);

		let Some(dependents) = dependents_by_package.get(&package_id).cloned() else {
			continue;
		};
		for dependent_package_id in dependents {
			let dependencies = dependencies_by_package
				.get_mut(&dependent_package_id)
				.expect(
					"dependent package must remain pending until its dependencies are published",
				);
			dependencies.remove(&package_id);
			if dependencies.is_empty() {
				ready.insert(dependent_package_id);
			}
		}
	}

	if !dependencies_by_package.is_empty() {
		return Err(MonochangeError::Config(format!(
			"cyclic publish dependencies detected among package publications: {}",
			render_publish_dependency_cycle(&dependencies_by_package)
		)));
	}

	let mut ordered_requests = Vec::new();
	for package_id in ordered_package_ids {
		let mut package_requests = requests_by_package
			.remove(&package_id)
			.expect("ordered package ids must come from publish requests");
		ordered_requests.append(&mut package_requests);
	}

	Ok(ordered_requests)
}

pub fn publish_dependency_kind_is_ordering_relevant(kind: DependencyKind) -> bool {
	!matches!(kind, DependencyKind::Development)
}

pub fn config_ids_by_package_record_id(packages: &[PackageRecord]) -> BTreeMap<String, String> {
	packages
		.iter()
		.map(|package| {
			let config_id = package
				.metadata
				.get("config_id")
				.map_or(package.name.as_str(), String::as_str);
			(package.id.clone(), config_id.to_string())
		})
		.collect()
}

pub fn render_publish_dependency_cycle(
	dependencies_by_package: &BTreeMap<String, BTreeSet<String>>,
) -> String {
	let cycle_edges = dependencies_by_package
		.iter()
		.flat_map(|(package_id, dependencies)| {
			dependencies
				.iter()
				.map(move |dependency_id| format!("{package_id} -> {dependency_id}"))
		})
		.collect::<Vec<_>>();
	cycle_edges.join(", ")
}

pub fn packages_by_config_id(packages: &[PackageRecord]) -> BTreeMap<&str, &PackageRecord> {
	packages
		.iter()
		.map(|package| {
			let config_id = package
				.metadata
				.get("config_id")
				.map_or(package.name.as_str(), String::as_str);
			(config_id, package)
		})
		.collect()
}

#[cfg(test)]
#[path = "__tests.rs"]
mod tests;
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RegistryEndpoints {
	pub npm_registry: String,
	pub crates_io_api: String,
	pub crates_io_index: String,
	pub pub_dev_api: String,
	pub jsr_base: String,
	pub pypi_api: String,
	pub go_proxy: String,
}

impl RegistryEndpoints {
	pub fn from_env() -> Self {
		Self {
			npm_registry: env::var("MONOCHANGE_NPM_REGISTRY_URL")
				.unwrap_or_else(|_| "https://registry.npmjs.org".to_string()),
			crates_io_api: env::var("MONOCHANGE_CRATES_IO_API_URL")
				.unwrap_or_else(|_| "https://crates.io/api/v1".to_string()),
			crates_io_index: env::var("MONOCHANGE_CRATES_IO_INDEX_URL")
				.unwrap_or_else(|_| "https://index.crates.io".to_string()),
			pub_dev_api: env::var("MONOCHANGE_PUB_DEV_API_URL")
				.unwrap_or_else(|_| "https://pub.dev/api".to_string()),
			jsr_base: env::var("MONOCHANGE_JSR_BASE_URL")
				.unwrap_or_else(|_| "https://jsr.io".to_string()),
			pypi_api: env::var("MONOCHANGE_PYPI_API_URL")
				.unwrap_or_else(|_| "https://pypi.org/pypi".to_string()),
			go_proxy: env::var("MONOCHANGE_GO_PROXY_URL")
				.unwrap_or_else(|_| "https://proxy.golang.org".to_string()),
		}
	}
}

pub fn registry_client() -> MonochangeResult<Client> {
	Client::builder()
		.user_agent(format!("monochange/{}", env!("CARGO_PKG_VERSION")))
		.build()
		.map_err(http_error("registry client build"))
}
pub fn package_can_be_published(
	package_definition: &monochange_core::PackageDefinition,
	package: &PackageRecord,
) -> bool {
	package_definition.publish.enabled
		&& !matches!(
			package.publish_state,
			PublishState::Private | PublishState::Excluded
		)
}
pub fn filter_pending_publish_requests(
	requests: &[PublishRequest],
) -> MonochangeResult<Vec<PublishRequest>> {
	let client = registry_client()?;
	let endpoints = RegistryEndpoints::from_env();
	filter_pending_publish_requests_with_transport(requests, &client, &endpoints)
}
pub fn filter_pending_publish_requests_with_transport(
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
) -> MonochangeResult<Vec<PublishRequest>> {
	let mut pending_requests = Vec::with_capacity(requests.len());

	for request in requests {
		if request.mode == PublishMode::External {
			continue;
		}
		if registry_version_exists(client, endpoints, request)? {
			continue;
		}
		pending_requests.push(request.clone());
	}

	Ok(pending_requests)
}
pub fn registry_version_exists(
	client: &Client,
	endpoints: &RegistryEndpoints,
	request: &PublishRequest,
) -> MonochangeResult<bool> {
	if request.registry == RegistryKind::Npm {
		let url = format!(
			"{}/{}",
			endpoints.npm_registry.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.map_err(http_error("npm registry lookup"))?;
		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("npm registry lookup"))?;
		let json = response
			.json::<JsonValue>()
			.map_err(http_error("npm registry decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_object)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.contains_key(&request.version)
			});
		return Ok(exists);
	}

	if request.registry == RegistryKind::CratesIo {
		return crates_io_version_exists(client, endpoints, request);
	}

	if request.registry == RegistryKind::PubDev {
		let url = format!(
			"{}/packages/{}",
			endpoints.pub_dev_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client
			.get(url)
			.send()
			.map_err(http_error("pub.dev lookup"))?;
		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}

		let response = response
			.error_for_status()
			.map_err(http_error("pub.dev lookup"))?;
		let json = response
			.json::<JsonValue>()
			.map_err(http_error("pub.dev decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_array)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.iter().any(|version| {
						version.get("version").and_then(JsonValue::as_str)
							== Some(request.version.as_str())
					})
			});
		return Ok(exists);
	}

	if request.registry == RegistryKind::Pypi {
		let url = format!(
			"{}/{}/json",
			endpoints.pypi_api.trim_end_matches('/'),
			encode(&request.package_name)
		);
		let response = client.get(url).send().map_err(http_error("PyPI lookup"))?;
		if response.status() == StatusCode::NOT_FOUND {
			return Ok(false);
		}
		let response = response
			.error_for_status()
			.map_err(http_error("PyPI lookup"))?;
		let json = response
			.json::<JsonValue>()
			.map_err(http_error("PyPI decode"))?;
		let exists = json
			.get("releases")
			.and_then(JsonValue::as_object)
			.is_some_and(|releases| {
				request.placeholder && !releases.is_empty()
					|| releases.contains_key(&request.version)
			});
		return Ok(exists);
	}

	if request.registry == RegistryKind::GoProxy {
		let url = format!(
			"{}/{}/@v/{}.info",
			endpoints.go_proxy.trim_end_matches('/'),
			go_proxy_module_path(go_module_path(request)),
			go_proxy_version(&request.version)
		);
		let response = client
			.get(url)
			.send()
			.map_err(http_error("Go proxy version lookup"))?;
		if response.status() == StatusCode::NOT_FOUND || response.status() == StatusCode::GONE {
			return Ok(false);
		}
		response
			.error_for_status()
			.map_err(http_error("Go proxy version lookup"))?;
		return Ok(true);
	}

	let url = format!(
		"{}/{}/meta.json",
		endpoints.jsr_base.trim_end_matches('/'),
		request.package_name
	);
	let response = client.get(url).send().map_err(http_error("jsr lookup"))?;
	if response.status() == StatusCode::NOT_FOUND {
		return Ok(false);
	}

	let response = response
		.error_for_status()
		.map_err(http_error("jsr lookup"))?;
	let json = response
		.json::<JsonValue>()
		.map_err(http_error("jsr decode"))?;
	let exists = json
		.get("versions")
		.and_then(JsonValue::as_object)
		.is_some_and(|versions| {
			request.placeholder && !versions.is_empty() || versions.contains_key(&request.version)
		});
	Ok(exists)
}
pub fn crates_io_version_exists(
	client: &Client,
	endpoints: &RegistryEndpoints,
	request: &PublishRequest,
) -> MonochangeResult<bool> {
	let url = format!(
		"{}/crates/{}",
		endpoints.crates_io_api.trim_end_matches('/'),
		encode(&request.package_name)
	);
	let response = client
		.get(url)
		.send()
		.map_err(http_error("crates.io lookup"))?;
	let status = response.status();

	if status == StatusCode::NOT_FOUND {
		return Ok(false);
	}

	if status.is_success() {
		let json = response
			.json::<JsonValue>()
			.map_err(http_error("crates.io decode"))?;
		let exists = json
			.get("versions")
			.and_then(JsonValue::as_array)
			.is_some_and(|versions| {
				request.placeholder && !versions.is_empty()
					|| versions.iter().any(|version| {
						version.get("num").and_then(JsonValue::as_str)
							== Some(request.version.as_str())
					})
			});
		return Ok(exists);
	}

	crates_io_index_version_exists(client, endpoints, request).map_err(|error| {
		MonochangeError::Discovery(format!(
			"crates.io lookup failed with http status {status}; crates.io index fallback failed: {error}"
		))
	})
}
pub fn crates_io_index_version_exists(
	client: &Client,
	endpoints: &RegistryEndpoints,
	request: &PublishRequest,
) -> MonochangeResult<bool> {
	let url = format!(
		"{}/{}",
		endpoints.crates_io_index.trim_end_matches('/'),
		crates_io_index_entry_path(&request.package_name)
	);
	let response = client
		.get(url)
		.send()
		.map_err(http_error("crates.io index lookup"))?;

	if response.status() == StatusCode::NOT_FOUND {
		return Ok(false);
	}

	let response = response
		.error_for_status()
		.map_err(http_error("crates.io index lookup"))?;
	let body = response
		.text()
		.map_err(http_error("crates.io index decode"))?;

	for line in body.lines().filter(|line| !line.trim().is_empty()) {
		let entry = serde_json::from_str::<JsonValue>(line).map_err(|error| {
			MonochangeError::Discovery(format!("crates.io index decode failed: {error}"))
		})?;
		let Some(version) = entry.get("vers").and_then(JsonValue::as_str) else {
			continue;
		};

		if request.placeholder || version == request.version {
			return Ok(true);
		}
	}

	Ok(false)
}
pub fn crates_io_index_entry_path(package_name: &str) -> String {
	let normalized = package_name.to_ascii_lowercase();
	match normalized.len() {
		0 => String::new(),
		1 => format!("1/{normalized}"),
		2 => format!("2/{normalized}"),
		3 => format!("3/{}/{normalized}", &normalized[..1]),
		_ => format!("{}/{}/{}", &normalized[..2], &normalized[2..4], normalized),
	}
}
fn http_error(context: &'static str) -> impl Fn(reqwest::Error) -> MonochangeError {
	move |error| MonochangeError::Discovery(format!("{context} failed: {error}"))
}
