use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::GroupDefinition;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::PackageRecord;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::RegistryKind;
use monochange_core::SourceConfiguration;
use monochange_core::TrustedPublishingSettings;
use monochange_core::WorkspaceConfiguration;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value as JsonValue;
use tempfile::TempDir;
use tracing::info;
use urlencoding::encode;

pub const PLACEHOLDER_VERSION: &str = "0.0.0";

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct SelectedReleasePublicationTargets {
	pub publication_targets: Vec<PackagePublicationTarget>,
	pub selected_packages: BTreeSet<String>,
}

pub fn select_release_publication_targets(
	groups: &[GroupDefinition],
	publication_targets: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
	selected_groups: &BTreeSet<String>,
	selected_ecosystems: &BTreeSet<Ecosystem>,
) -> SelectedReleasePublicationTargets {
	let mut publication_targets = publication_targets.to_vec();
	if !selected_ecosystems.is_empty() {
		publication_targets.retain(|target| selected_ecosystems.contains(&target.ecosystem));
	}

	let mut selected_packages = selected_packages.clone();
	for group_id in selected_groups {
		if let Some(group) = groups.iter().find(|group| group.id == *group_id) {
			selected_packages.extend(group.packages.iter().cloned());
		}
	}

	SelectedReleasePublicationTargets {
		publication_targets,
		selected_packages,
	}
}

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

#[must_use]
pub fn disabled_trust_outcome() -> TrustedPublishingOutcome {
	TrustedPublishingOutcome {
		status: TrustedPublishingStatus::Disabled,
		repository: None,
		workflow: None,
		environment: None,
		setup_url: None,
		message: "trusted publishing disabled".to_string(),
	}
}

#[must_use]
pub fn failed_publish_outcome(
	mode: PackagePublishRunMode,
	request: &PublishRequest,
	message: String,
) -> PackagePublishOutcome {
	PackagePublishOutcome {
		package: request.package_id.clone(),
		ecosystem: request.ecosystem,
		registry: request.registry.to_string(),
		version: request.version.clone(),
		status: PackagePublishStatus::Failed,
		message,
		placeholder: mode == PackagePublishRunMode::Placeholder,
		trusted_publishing: disabled_trust_outcome(),
		command: None,
		stdout: None,
		stderr: None,
	}
}

#[must_use]
pub fn planned_publish_message(mode: PackagePublishRunMode, request: &PublishRequest) -> String {
	match mode {
		PackagePublishRunMode::Placeholder => {
			format!(
				"would publish placeholder {} {} to {}",
				request.package_name, request.version, request.registry
			)
		}
		PackagePublishRunMode::Release => {
			format!(
				"would publish {} {} to {}",
				request.package_name, request.version, request.registry
			)
		}
	}
}

#[must_use]
pub fn non_empty_output(output: String) -> Option<String> {
	(!output.is_empty()).then_some(output)
}

pub fn reject_npm_token_environment(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<()> {
	let token_keys = forbidden_npm_token_env_keys(env_map);
	if token_keys.is_empty() {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"`{}` requires npm trusted publishing, but long-lived npm token environment variables are present: {}. Remove token-based npm credentials and publish from the configured CI/OIDC workflow, or set `publish.trusted_publishing = false` to opt out.",
		request.package_id,
		token_keys.join(", "),
	)))
}

#[must_use]
pub fn forbidden_npm_token_env_keys(env_map: &BTreeMap<String, String>) -> Vec<String> {
	env_map
		.keys()
		.filter(|key| is_forbidden_npm_token_env_key(key))
		.cloned()
		.collect()
}

#[must_use]
pub fn is_forbidden_npm_token_env_key(key: &str) -> bool {
	let lowercase_key = key.to_ascii_lowercase();
	matches!(
		key,
		"NPM_TOKEN" | "NODE_AUTH_TOKEN" | "NPM_CONFIG__AUTH_TOKEN" | "npm_config__authToken"
	) || (lowercase_key.starts_with("npm_config_")
		&& lowercase_key.contains("auth")
		&& lowercase_key.contains("token"))
}

pub fn enforce_release_attestation_prerequisites(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
	command_builder: &PublishCommandBuilder,
) -> MonochangeResult<()> {
	if !request.attestations.require_registry_provenance {
		return Ok(());
	}

	if !request.trusted_publishing.enabled {
		return Err(MonochangeError::Config(format!(
			"`{}` requires registry-native package provenance, but trusted publishing is disabled. Registry provenance is only enforceable for built-in publishing from a verifiable CI/OIDC identity; set `publish.trusted_publishing = true` or set `publish.attestations.require_registry_provenance = false` to opt out.",
			request.package_id,
		)));
	}

	let registry = PublishRegistry::Builtin(request.registry);
	let identity = detect_trusted_publishing_identity(env_map);
	let capability_message = trusted_publishing_capability_message(&registry, &identity);
	if !identity.is_verifiable_by_env() {
		return Err(MonochangeError::Config(format!(
			"`{}` requires registry-native package provenance from a verifiable CI/OIDC identity, but the current publishing context is local or unverifiable. {capability_message} Run `mc publish` from the configured CI workflow or set `publish.attestations.require_registry_provenance = false` to opt out.",
			request.package_id,
		)));
	}

	let capability = provider_registry_trust_capability(&registry, identity.provider());
	if !capability.registry_native_provenance {
		return Err(MonochangeError::Config(format!(
			"`{}` cannot require registry-native package provenance for {} from {}. {capability_message} This registry/provider combination does not expose provenance monochange can require; set `publish.attestations.require_registry_provenance = false` to opt out or use an external publisher that enforces its own attestation policy.",
			request.package_id,
			request.registry,
			identity.provider().label(),
		)));
	}
	if !command_builder
		.adapter_for_registry(request.registry)
		.is_some_and(PublishAdapter::supports_provenance)
	{
		return Err(MonochangeError::Config(format!(
			"`{}` cannot require registry-native package provenance for {} yet. {capability_message} The registry supports provenance, but monochange's current built-in publisher for this ecosystem does not expose a publish command that can require it; set `publish.attestations.require_registry_provenance = false` to opt out or use an external publisher that enforces its own attestation policy.",
			request.package_id, request.registry,
		)));
	}

	Ok(())
}

#[must_use]
pub fn manual_setup_url(request: &PublishRequest) -> String {
	if request.registry == RegistryKind::CratesIo {
		format!("https://crates.io/crates/{}", encode(&request.package_name))
	} else if request.registry == RegistryKind::PubDev {
		format!("https://pub.dev/packages/{}/admin", request.package_name)
	} else if request.registry == RegistryKind::Jsr {
		format!("https://jsr.io/{}", request.package_name)
	} else if request.registry == RegistryKind::Pypi {
		format!(
			"https://pypi.org/manage/project/{}/settings/publishing/",
			request.package_name
		)
	} else if request.registry == RegistryKind::GoProxy {
		format!("https://pkg.go.dev/{}", go_module_path(request))
	} else {
		format!(
			"https://www.npmjs.com/package/{}/access",
			request.package_name
		)
	}
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

pub type PlaceholderManifestWriter =
	dyn Fn(&Path, &PublishRequest, &Path, Option<&SourceConfiguration>) -> MonochangeResult<()>;

pub type PublishReadinessChecker =
	dyn Fn(&Path, &PublishRequest) -> MonochangeResult<Option<String>>;

#[derive(Default)]
pub struct PublishReadinessRegistry {
	checkers: Vec<(RegistryKind, Box<PublishReadinessChecker>)>,
}

impl PublishReadinessRegistry {
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	#[must_use]
	pub fn with_checker(
		mut self,
		registry: RegistryKind,
		checker: Box<PublishReadinessChecker>,
	) -> Self {
		self.checkers.push((registry, checker));
		self
	}

	pub fn push_checker(&mut self, registry: RegistryKind, checker: Box<PublishReadinessChecker>) {
		self.checkers.push((registry, checker));
	}

	pub fn blocked_message(
		&self,
		root: &Path,
		request: &PublishRequest,
	) -> MonochangeResult<Option<String>> {
		let Some((_, checker)) = self
			.checkers
			.iter()
			.find(|(registry, _)| *registry == request.registry)
		else {
			return Ok(None);
		};
		checker(root, request)
	}
}

#[derive(Default)]
pub struct PlaceholderManifestWriterRegistry {
	writers: Vec<(RegistryKind, Box<PlaceholderManifestWriter>)>,
}

impl PlaceholderManifestWriterRegistry {
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	#[must_use]
	pub fn with_writer(
		mut self,
		registry: RegistryKind,
		writer: Box<PlaceholderManifestWriter>,
	) -> Self {
		self.writers.push((registry, writer));
		self
	}

	pub fn push_writer(&mut self, registry: RegistryKind, writer: Box<PlaceholderManifestWriter>) {
		self.writers.push((registry, writer));
	}

	pub fn write_manifest(
		&self,
		placeholder_dir: &Path,
		request: &PublishRequest,
		root: &Path,
		source: Option<&SourceConfiguration>,
	) -> MonochangeResult<()> {
		let (_, writer) = self
			.writers
			.iter()
			.find(|(registry, _)| *registry == request.registry)
			.expect("unsupported built-in publish registry");
		writer(placeholder_dir, request, root, source)
	}
}

pub fn build_placeholder_directory(
	root: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	manifest_writers: &PlaceholderManifestWriterRegistry,
) -> MonochangeResult<TempDir> {
	let tempdir = tempfile::tempdir().map_err(|error| placeholder_tempdir_error(&error))?;
	fs::write(
		tempdir.path().join("README.md"),
		&request.placeholder_readme,
	)
	.map_err(|error| MonochangeError::Io(format!("failed to write placeholder README: {error}")))?;

	manifest_writers.write_manifest(tempdir.path(), request, root, source)?;

	Ok(tempdir)
}

fn placeholder_tempdir_error(error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!("failed to create placeholder tempdir: {error}"))
}

#[must_use]
pub fn current_env_map() -> BTreeMap<String, String> {
	env::vars().collect()
}

pub trait PublishTrustHandler {
	fn trust_outcome_for_skip(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome;

	fn planned_trust_outcome(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome;

	fn enforce_release_trust_prerequisites(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<()>;
}

#[allow(clippy::too_many_arguments)]
pub fn execute_publish_requests_with_process(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
) -> MonochangeResult<PackagePublishReport> {
	let env_map = current_env_map();
	let endpoints = RegistryEndpoints::from_env();
	let client = registry_client()?;
	let mut executor = ProcessCommandExecutor;
	execute_publish_requests(
		root,
		source,
		mode,
		dry_run,
		requests,
		&client,
		&endpoints,
		&env_map,
		&mut executor,
		command_builder,
		manifest_writers,
		readiness,
		trust_handler,
	)
}

#[allow(clippy::too_many_arguments)]
pub fn execute_publish_requests(
	root: &Path,
	source: Option<&SourceConfiguration>,
	mode: PackagePublishRunMode,
	dry_run: bool,
	requests: &[PublishRequest],
	client: &Client,
	endpoints: &RegistryEndpoints,
	env_map: &BTreeMap<String, String>,
	executor: &mut dyn CommandExecutor,
	command_builder: &PublishCommandBuilder,
	manifest_writers: &PlaceholderManifestWriterRegistry,
	readiness: &PublishReadinessRegistry,
	trust_handler: &dyn PublishTrustHandler,
) -> MonochangeResult<PackagePublishReport> {
	let mut outcomes = Vec::new();

	for request in requests {
		if request.mode == PublishMode::External {
			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				"skipping external package"
			);
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::SkippedExternal,
				message: "package opted out of built-in publishing".to_string(),
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: disabled_trust_outcome(),
				command: None,
				stdout: None,
				stderr: None,
			});
			continue;
		}

		info!(
			package_name = request.package_name,
			version = %request.version,
			registry = %request.registry,
			dry_run,
			mode = ?mode,
			"publishing package"
		);

		let version_exists = registry_version_exists(client, endpoints, request)?;
		if version_exists {
			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				"skipping already-published version"
			);
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::SkippedExisting,
				message: format!(
					"{} {} already exists on {}",
					request.package_name, request.version, request.registry
				),
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: trust_handler
					.trust_outcome_for_skip(request, source, root, env_map),
				command: None,
				stdout: None,
				stderr: None,
			});
			continue;
		}

		let blocked_message = if mode == PackagePublishRunMode::Release {
			readiness.blocked_message(root, request)?
		} else {
			None
		};
		if let Some(message) = blocked_message {
			if dry_run {
				outcomes.push(PackagePublishOutcome {
					package: request.package_id.clone(),
					ecosystem: request.ecosystem,
					registry: request.registry.to_string(),
					version: request.version.clone(),
					status: PackagePublishStatus::Blocked,
					message,
					placeholder: mode == PackagePublishRunMode::Placeholder,
					trusted_publishing: trust_handler
						.planned_trust_outcome(request, source, root, env_map),
					command: None,
					stdout: None,
					stderr: None,
				});
				continue;
			}

			return Err(MonochangeError::Config(message));
		}

		let placeholder_dir = if mode == PackagePublishRunMode::Placeholder {
			Some(build_placeholder_directory(
				root,
				request,
				source,
				manifest_writers,
			)?)
		} else {
			None
		};
		let publish_command = command_builder.build_publish_command(
			request,
			mode,
			placeholder_dir.as_ref().map(TempDir::path),
			dry_run,
		);

		if dry_run {
			info!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				mode = ?mode,
				"would publish package (dry run)"
			);
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::Planned,
				message: planned_publish_message(mode, request),
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: trust_handler
					.planned_trust_outcome(request, source, root, env_map),
				command: None,
				stdout: None,
				stderr: None,
			});
			continue;
		}

		if mode == PackagePublishRunMode::Release {
			trust_handler.enforce_release_trust_prerequisites(request, source, root, env_map)?;
			enforce_release_attestation_prerequisites(request, env_map, command_builder)?;
		}

		let output = match executor.run(&publish_command) {
			Ok(output) => output,
			Err(error) => {
				tracing::error!(
					package_name = request.package_name,
					version = %request.version,
					registry = %request.registry,
					error = %error,
					"publish command failed to execute"
				);
				outcomes.push(failed_publish_outcome(mode, request, error.to_string()));
				break;
			}
		};
		if !output.success {
			tracing::error!(
				package_name = request.package_name,
				version = %request.version,
				registry = %request.registry,
				"publish command returned non-zero exit"
			);
			let mut outcome = failed_publish_outcome(
				mode,
				request,
				format!(
					"`{}` failed: {}",
					render_command(&publish_command),
					render_command_error(&output)
				),
			);
			outcome.command = Some(render_command(&publish_command));
			outcome.stdout = non_empty_output(output.stdout);
			outcome.stderr = non_empty_output(output.stderr);
			outcomes.push(outcome);
			break;
		}

		let trusted_publishing = if request.trusted_publishing.enabled {
			trust_handler.trust_outcome_for_skip(request, source, root, env_map)
		} else {
			disabled_trust_outcome()
		};

		info!(
			package_name = request.package_name,
			version = %request.version,
			registry = %request.registry,
			"published package"
		);
		outcomes.push(PackagePublishOutcome {
			package: request.package_id.clone(),
			ecosystem: request.ecosystem,
			registry: request.registry.to_string(),
			version: request.version.clone(),
			status: PackagePublishStatus::Published,
			message: format!(
				"published {} {} to {}",
				request.package_name, request.version, request.registry
			),
			placeholder: mode == PackagePublishRunMode::Placeholder,
			trusted_publishing,
			command: Some(render_command(&publish_command)),
			stdout: non_empty_output(output.stdout),
			stderr: non_empty_output(output.stderr),
		});
	}

	Ok(PackagePublishReport {
		mode,
		dry_run,
		packages: outcomes,
	})
}

pub fn build_placeholder_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let packages_by_config_id = packages_by_config_id(packages);
	let mut requests = Vec::new();

	for package_definition in &configuration.packages {
		let package = packages_by_config_id
			.get(package_definition.id.as_str())
			.copied();
		let should_publish =
			package.is_some_and(|package| package_can_be_published(package_definition, package));
		if let Some(package) = package.filter(|_| {
			should_publish
				&& (selected_packages.is_empty()
					|| selected_packages.contains(&package_definition.id))
		}) {
			requests.push(PublishRequest {
				package_id: package_definition.id.clone(),
				package_name: package.name.clone(),
				ecosystem: package.ecosystem,
				manifest_path: package.manifest_path.clone(),
				package_root: package
					.manifest_path
					.parent()
					.unwrap_or(&package.workspace_root)
					.to_path_buf(),
				registry: resolve_registry_kind(
					package_definition.publish.registry.as_ref(),
					package.ecosystem,
				)?,
				package_manager: package.metadata.get("manager").cloned(),
				package_metadata: package.metadata.clone(),
				mode: package_definition.publish.mode,
				version: PLACEHOLDER_VERSION.to_string(),
				placeholder: true,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
				attestations: package_definition.publish.attestations.clone(),
				placeholder_readme: resolve_placeholder_readme(
					root,
					package_definition.publish.placeholder.readme.as_deref(),
					package_definition
						.publish
						.placeholder
						.readme_file
						.as_deref(),
					&package.name,
				)?,
			});
		}
	}

	requests.sort_by(|left, right| left.package_id.cmp(&right.package_id));
	Ok(requests)
}

pub fn configured_package_publication_targets(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
) -> Vec<PackagePublicationTarget> {
	let packages_by_config_id = packages_by_config_id(packages);
	configuration
		.packages
		.iter()
		.filter_map(|package_definition| {
			let package = packages_by_config_id
				.get(package_definition.id.as_str())
				.copied()?;
			let version = package.current_version.as_ref()?.to_string();
			Some(PackagePublicationTarget {
				package: package_definition.id.clone(),
				ecosystem: package.ecosystem,
				registry: package_definition.publish.registry.clone(),
				version,
				mode: package_definition.publish.mode,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
				attestations: package_definition.publish.attestations.clone(),
			})
		})
		.collect()
}

pub fn build_configured_package_release_requests(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let publications = configured_package_publication_targets(configuration, packages);
	build_release_requests(configuration, packages, &publications, selected_packages)
}

pub fn build_pending_configured_package_release_requests(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let requests =
		build_configured_package_release_requests(configuration, packages, selected_packages)?;
	filter_pending_publish_requests(&requests)
}

pub fn build_release_requests(
	configuration: &WorkspaceConfiguration,
	packages: &[PackageRecord],
	publications: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<Vec<PublishRequest>> {
	let packages_by_config_id = packages_by_config_id(packages);
	let mut requests = Vec::new();

	for publication in publications {
		if !selected_packages.is_empty() && !selected_packages.contains(&publication.package) {
			continue;
		}

		let Some(package_definition) = configuration.package_by_id(&publication.package) else {
			continue;
		};
		let Some(package) = packages_by_config_id
			.get(publication.package.as_str())
			.copied()
		else {
			continue;
		};

		if !package_can_be_published(package_definition, package) {
			continue;
		}

		requests.push(PublishRequest {
			package_id: publication.package.clone(),
			package_name: package.name.clone(),
			ecosystem: package.ecosystem,
			manifest_path: package.manifest_path.clone(),
			package_root: package
				.manifest_path
				.parent()
				.unwrap_or(&package.workspace_root)
				.to_path_buf(),
			registry: resolve_registry_kind(publication.registry.as_ref(), package.ecosystem)?,
			package_manager: package.metadata.get("manager").cloned(),
			package_metadata: package.metadata.clone(),
			mode: publication.mode,
			version: publication.version.clone(),
			placeholder: false,
			trusted_publishing: publication.trusted_publishing.clone(),
			attestations: publication.attestations.clone(),
			placeholder_readme: default_placeholder_readme(&package.name),
		});
	}

	order_release_requests_by_publish_dependencies(packages, requests)
}

pub fn resolve_registry_kind(
	registry: Option<&PublishRegistry>,
	ecosystem: Ecosystem,
) -> MonochangeResult<RegistryKind> {
	match registry {
		Some(PublishRegistry::Builtin(registry)) => Ok(*registry),
		Some(PublishRegistry::Custom(name)) => {
			Err(MonochangeError::Config(format!(
				"built-in package publishing does not support custom registry `{name}`"
			)))
		}
		None => default_registry_kind_for_ecosystem(ecosystem.as_str()),
	}
}

pub fn default_registry_kind_for_ecosystem(ecosystem: &str) -> MonochangeResult<RegistryKind> {
	let parsed = ecosystem.parse::<Ecosystem>().map_err(|()| {
		MonochangeError::Config(format!(
			"built-in package publishing does not support ecosystem `{ecosystem}`"
		))
	})?;
	Ok(monochange_core::default_registry_kind_for_ecosystem(parsed)
		.expect("all built-in ecosystems have default registries"))
}

pub fn resolve_placeholder_readme(
	root: &Path,
	inline: Option<&str>,
	file: Option<&Path>,
	package_name: &str,
) -> MonochangeResult<String> {
	if let Some(inline) = inline {
		return Ok(inline.to_string());
	}
	if let Some(file) = file {
		let path = root.join(file);
		return fs::read_to_string(&path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read placeholder README {}: {error}",
				path.display()
			))
		});
	}
	Ok(default_placeholder_readme(package_name))
}

pub fn default_placeholder_readme(package_name: &str) -> String {
	format!(
		"# {package_name}\n\nThis is a placeholder release published by monochange to bootstrap trusted publishing.\n"
	)
}

/// Adapter for building ecosystem-specific publish commands.
pub trait PublishAdapter {
	fn registry_kind(&self) -> RegistryKind;
	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec>;
	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec>;
	fn append_dry_run_args(&self, args: &mut Vec<String>) {
		args.push("--dry-run".to_string());
	}
	fn supported_providers(&self) -> Vec<CiProviderKind> {
		Vec::new()
	}
	fn registry_setup_url(&self) -> Option<&'static str> {
		None
	}
	fn registry_notes(&self) -> Vec<String> {
		vec!["unknown registry capabilities are treated as unsupported".to_string()]
	}
	fn supports_provenance(&self) -> bool {
		false
	}
}

/// Registry of publish adapters used to dispatch publish command construction.
#[derive(Default)]
pub struct PublishCommandBuilder {
	adapters: Vec<Box<dyn PublishAdapter>>,
}

impl PublishCommandBuilder {
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	#[must_use]
	pub fn with_adapter(mut self, adapter: Box<dyn PublishAdapter>) -> Self {
		self.adapters.push(adapter);
		self
	}

	pub fn push_adapter(&mut self, adapter: Box<dyn PublishAdapter>) {
		self.adapters.push(adapter);
	}

	pub fn adapter_for_registry(&self, registry: RegistryKind) -> Option<&dyn PublishAdapter> {
		self.adapters
			.iter()
			.find(|adapter| adapter.registry_kind() == registry)
			.map(AsRef::as_ref)
	}

	pub fn build_publish_command(
		&self,
		request: &PublishRequest,
		mode: PackagePublishRunMode,
		placeholder_dir: Option<&Path>,
		dry_run: bool,
	) -> CommandSpec {
		let adapter = self
			.adapter_for_registry(request.registry)
			.expect("unsupported built-in publish registry");
		let mut command = match mode {
			PackagePublishRunMode::Placeholder => {
				let path = placeholder_dir.expect("placeholder directory must exist");
				adapter.build_placeholder_command(request, path)
			}
			PackagePublishRunMode::Release => adapter.build_release_command(request),
		}
		.expect("unsupported publish mode for this registry");
		if dry_run {
			adapter.append_dry_run_args(&mut command.args);
		}
		command
	}
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
	build_publish_command_builder().build_publish_command(request, mode, placeholder_dir, dry_run)
}

pub fn build_publish_command_builder() -> PublishCommandBuilder {
	PublishCommandBuilder::new()
		.with_adapter(Box::new(NpmPublishAdapter))
		.with_adapter(Box::new(CargoPublishAdapter))
		.with_adapter(Box::new(DartPublishAdapter))
		.with_adapter(Box::new(JsrPublishAdapter))
		.with_adapter(Box::new(PythonPublishAdapter))
		.with_adapter(Box::new(GoPublishAdapter))
}

struct NpmPublishAdapter;

impl PublishAdapter for NpmPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::Npm
	}

	fn supports_provenance(&self) -> bool {
		true
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_npm_placeholder_publish_command(
			request,
			placeholder_path,
		))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_npm_release_publish_command(request))
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![CiProviderKind::GitHubActions, CiProviderKind::GitLabCi]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://docs.npmjs.com/trusted-publishers")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"npm trusted publishing supports GitHub Actions and GitLab CI/CD".to_string(),
			"monochange can verify and automate npm GitHub trusted-publisher setup with npm CLI trust commands".to_string(),
		].to_vec()
	}
}

struct CargoPublishAdapter;

impl PublishAdapter for CargoPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::CratesIo
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_cargo_placeholder_publish_command(
			request,
			placeholder_path,
		))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_cargo_release_publish_command(request))
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![CiProviderKind::GitHubActions]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://crates.io/docs/trusted-publishing")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"crates.io trusted publishing uses OIDC short-lived tokens".to_string(),
			"monochange does not currently verify crates.io registry-side trusted-publisher setup"
				.to_string(),
		]
		.to_vec()
	}
}

struct DartPublishAdapter;

impl PublishAdapter for DartPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::PubDev
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_dart_publish_command(request, placeholder_path))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_dart_publish_command(request, &request.package_root))
	}

	fn append_dry_run_args(&self, args: &mut Vec<String>) {
		args.retain(|arg| arg != "--force");
		args.push("--dry-run".to_string());
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![
			CiProviderKind::GitHubActions,
			CiProviderKind::GoogleCloudBuild,
		]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://dart.dev/tools/pub/automated-publishing")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"pub.dev automated publishing uses configured OIDC publishers".to_string(),
			"pub.dev registry-side publisher setup requires manual review".to_string(),
		]
		.to_vec()
	}
}

struct JsrPublishAdapter;

impl PublishAdapter for JsrPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::Jsr
	}

	fn supports_provenance(&self) -> bool {
		true
	}

	fn build_placeholder_command(
		&self,
		_request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_jsr_publish_command(placeholder_path))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_jsr_publish_command(&request.package_root))
	}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![CiProviderKind::GitHubActions]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://jsr.io/docs/publishing-packages")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"JSR can publish from supported CI without long-lived tokens".to_string(),
			"JSR package provenance is available, but monochange does not verify registry-side setup".to_string(),
		].to_vec()
	}
}

struct PythonPublishAdapter;

impl PublishAdapter for PythonPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::Pypi
	}

	fn build_placeholder_command(
		&self,
		request: &PublishRequest,
		placeholder_path: &Path,
	) -> Option<CommandSpec> {
		Some(build_python_publish_command(request, placeholder_path))
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_python_publish_command(request, &request.package_root))
	}

	fn append_dry_run_args(&self, _args: &mut Vec<String>) {}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		vec![
			CiProviderKind::GitHubActions,
			CiProviderKind::GitLabCi,
			CiProviderKind::GoogleCloudBuild,
		]
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		Some("https://docs.pypi.org/trusted-publishers/")
	}

	fn registry_notes(&self) -> Vec<String> {
		[
			"PyPI Trusted Publishers support multiple CI identity providers".to_string(),
			"PEP 740 digital attestations are separate from trusted-publisher authorization"
				.to_string(),
		]
		.to_vec()
	}
}

struct GoPublishAdapter;

impl PublishAdapter for GoPublishAdapter {
	fn registry_kind(&self) -> RegistryKind {
		RegistryKind::GoProxy
	}

	fn build_placeholder_command(
		&self,
		_request: &PublishRequest,
		_placeholder_path: &Path,
	) -> Option<CommandSpec> {
		None
	}

	fn build_release_command(&self, request: &PublishRequest) -> Option<CommandSpec> {
		Some(build_go_publish_command(request))
	}

	fn append_dry_run_args(&self, _args: &mut Vec<String>) {}

	fn supported_providers(&self) -> Vec<CiProviderKind> {
		Vec::new()
	}

	fn registry_setup_url(&self) -> Option<&'static str> {
		None
	}

	fn registry_notes(&self) -> Vec<String> {
		["unknown registry capabilities are treated as unsupported".to_string()].to_vec()
	}
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
	build_publish_command_builder()
		.adapter_for_registry(registry)
		.map_or_else(Vec::new, PublishAdapter::supported_providers)
}

fn registry_setup_url(registry: RegistryKind) -> Option<&'static str> {
	build_publish_command_builder()
		.adapter_for_registry(registry)
		.and_then(PublishAdapter::registry_setup_url)
}

fn registry_notes(registry: RegistryKind) -> Vec<String> {
	build_publish_command_builder()
		.adapter_for_registry(registry)
		.map_or_else(
			|| vec!["unknown registry capabilities are treated as unsupported".to_string()],
			PublishAdapter::registry_notes,
		)
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
#[path = "__tests__/lib_tests.rs"]
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
