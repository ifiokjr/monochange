use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;

use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::PackageRecord;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::RegistryKind;
use monochange_core::SourceConfiguration;
use monochange_core::TrustedPublishingSettings;
use monochange_core::WorkspaceConfiguration;
use reqwest::StatusCode;
use reqwest::blocking::Client;
use serde::Serialize;
use serde_json::Value as JsonValue;
use serde_yaml_ng::Value as YamlValue;
use tempfile::TempDir;
use toml::Value as TomlValue;
use urlencoding::encode;

use crate::PreparedRelease;
use crate::discover_release_record;
use crate::discover_workspace;

const PLACEHOLDER_VERSION: &str = "0.0.0";
#[cfg(test)]
const NPM_TRUST_DOCS_URL: &str = "https://docs.npmjs.com/cli/v11/commands/npm-trust";
#[cfg(test)]
const CRATES_TRUST_DOCS_URL: &str = "https://crates.io/docs/trusted-publishing";
#[cfg(test)]
const DART_TRUST_DOCS_URL: &str = "https://dart.dev/tools/pub/automated-publishing";
#[cfg(test)]
const JSR_TRUST_DOCS_URL: &str = "https://jsr.io/docs/publishing-packages";

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackagePublishRunMode {
	Placeholder,
	Release,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackagePublishStatus {
	Planned,
	Published,
	SkippedExisting,
	SkippedExternal,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TrustedPublishingStatus {
	Disabled,
	Planned,
	Configured,
	ManualActionRequired,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct TrustedPublishingOutcome {
	pub status: TrustedPublishingStatus,
	pub repository: Option<String>,
	pub workflow: Option<String>,
	pub environment: Option<String>,
	pub setup_url: Option<String>,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackagePublishOutcome {
	pub package: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
	pub version: String,
	pub status: PackagePublishStatus,
	pub message: String,
	pub placeholder: bool,
	pub trusted_publishing: TrustedPublishingOutcome,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PackagePublishReport {
	pub mode: PackagePublishRunMode,
	pub dry_run: bool,
	pub packages: Vec<PackagePublishOutcome>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PublishRequest {
	pub(crate) package_id: String,
	pub(crate) package_name: String,
	pub(crate) ecosystem: Ecosystem,
	pub(crate) manifest_path: PathBuf,
	pub(crate) package_root: PathBuf,
	pub(crate) registry: RegistryKind,
	pub(crate) package_manager: Option<String>,
	pub(crate) mode: PublishMode,
	pub(crate) version: String,
	pub(crate) placeholder: bool,
	pub(crate) trusted_publishing: TrustedPublishingSettings,
	pub(crate) placeholder_readme: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct GitHubTrustContext {
	repository: String,
	workflow: String,
	environment: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CommandSpec {
	program: String,
	args: Vec<String>,
	cwd: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct CommandOutput {
	success: bool,
	stdout: String,
	stderr: String,
}

trait CommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput>;
}

struct ProcessCommandExecutor;

impl CommandExecutor for ProcessCommandExecutor {
	fn run(&mut self, spec: &CommandSpec) -> MonochangeResult<CommandOutput> {
		let output = ProcessCommand::new(&spec.program)
			.args(&spec.args)
			.current_dir(&spec.cwd)
			.output()
			.map_err(|error| {
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

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct RegistryEndpoints {
	pub(crate) npm_registry: String,
	pub(crate) crates_io_api: String,
	pub(crate) crates_io_index: String,
	pub(crate) pub_dev_api: String,
	pub(crate) jsr_base: String,
}

impl RegistryEndpoints {
	pub(crate) fn from_env() -> Self {
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
		}
	}
}

pub(crate) fn run_placeholder_publish(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	selected_packages: &BTreeSet<String>,
	dry_run: bool,
) -> MonochangeResult<PackagePublishReport> {
	let discovery = discover_workspace(root)?;
	let requests =
		build_placeholder_requests(root, configuration, &discovery.packages, selected_packages)?;
	let env_map = current_env_map();
	let endpoints = RegistryEndpoints::from_env();
	let client = registry_client()?;
	let mut executor = ProcessCommandExecutor;
	execute_publish_requests(
		root,
		configuration.source.as_ref(),
		PackagePublishRunMode::Placeholder,
		dry_run,
		&requests,
		&client,
		&endpoints,
		&env_map,
		&mut executor,
	)
}

pub(crate) fn run_publish_packages(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	dry_run: bool,
) -> MonochangeResult<PackagePublishReport> {
	let discovery = discover_workspace(root)?;
	let publication_targets =
		release_record_package_publications_from_prepared_or_head(root, prepared_release)?;
	#[rustfmt::skip]
	let requests = build_release_requests(configuration, &discovery.packages, &publication_targets, selected_packages)?;
	let env_map = current_env_map();
	let endpoints = RegistryEndpoints::from_env();
	let client = registry_client()?;
	let mut executor = ProcessCommandExecutor;
	execute_publish_requests(
		root,
		configuration.source.as_ref(),
		PackagePublishRunMode::Release,
		dry_run,
		&requests,
		&client,
		&endpoints,
		&env_map,
		&mut executor,
	)
}

pub(crate) fn release_record_package_publications_from_prepared_or_head(
	root: &Path,
	prepared_release: Option<&PreparedRelease>,
) -> MonochangeResult<Vec<PackagePublicationTarget>> {
	if let Some(prepared_release) = prepared_release {
		return Ok(prepared_release.package_publications.clone());
	}
	Ok(discover_release_record(root, "HEAD")?
		.record
		.package_publications)
}

fn current_env_map() -> BTreeMap<String, String> {
	env::vars().collect()
}

fn registry_client() -> MonochangeResult<Client> {
	Client::builder()
		.user_agent(format!("monochange/{}", env!("CARGO_PKG_VERSION")))
		.build()
		.map_err(http_error("registry client build"))
}

fn package_can_be_published(
	package_definition: &monochange_core::PackageDefinition,
	package: &PackageRecord,
) -> bool {
	package_definition.publish.enabled
		&& !matches!(
			package.publish_state,
			PublishState::Private | PublishState::Excluded
		)
}

pub(crate) fn build_placeholder_requests(
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
				mode: package_definition.publish.mode,
				version: PLACEHOLDER_VERSION.to_string(),
				placeholder: true,
				trusted_publishing: package_definition.publish.trusted_publishing.clone(),
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

pub(crate) fn build_release_requests(
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
			mode: publication.mode,
			version: publication.version.clone(),
			placeholder: false,
			trusted_publishing: publication.trusted_publishing.clone(),
			placeholder_readme: default_placeholder_readme(&package.name),
		});
	}

	requests.sort_by(|left, right| left.package_id.cmp(&right.package_id));
	Ok(requests)
}

pub(crate) fn filter_pending_publish_requests(
	requests: &[PublishRequest],
) -> MonochangeResult<Vec<PublishRequest>> {
	let client = registry_client()?;
	let endpoints = RegistryEndpoints::from_env();
	filter_pending_publish_requests_with_transport(requests, &client, &endpoints)
}

pub(crate) fn filter_pending_publish_requests_with_transport(
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

fn packages_by_config_id(packages: &[PackageRecord]) -> BTreeMap<&str, &PackageRecord> {
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

fn resolve_registry_kind(
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

fn default_registry_kind_for_ecosystem(ecosystem: &str) -> MonochangeResult<RegistryKind> {
	match ecosystem {
		"cargo" => Ok(RegistryKind::CratesIo),
		"npm" => Ok(RegistryKind::Npm),
		"deno" => Ok(RegistryKind::Jsr),
		"dart" | "flutter" => Ok(RegistryKind::PubDev),
		_ => {
			Err(MonochangeError::Config(format!(
				"built-in package publishing does not support ecosystem `{ecosystem}`"
			)))
		}
	}
}

fn resolve_placeholder_readme(
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

fn default_placeholder_readme(package_name: &str) -> String {
	format!(
		"# {package_name}\n\nThis is a placeholder release published by monochange to bootstrap trusted publishing.\n"
	)
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
	let mut outcomes = Vec::new();

	for request in requests {
		if request.mode == PublishMode::External {
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::SkippedExternal,
				message: "package opted out of built-in publishing".to_string(),
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: disabled_trust_outcome(),
			});
			continue;
		}

		let version_exists = registry_version_exists(client, endpoints, request)?;
		if version_exists {
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
				trusted_publishing: trust_outcome_for_skip(request, source, root, env_map),
			});
			continue;
		}

		let placeholder_dir = if mode == PackagePublishRunMode::Placeholder {
			Some(build_placeholder_directory(root, request, source)?)
		} else {
			None
		};
		let publish_command =
			build_publish_command(request, mode, placeholder_dir.as_ref(), dry_run);

		if dry_run {
			let output = executor.run(&publish_command)?;
			if !output.success {
				return Err(MonochangeError::Discovery(format!(
					"`{}` failed: {}",
					render_command(&publish_command),
					render_command_error(&output)
				)));
			}

			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::Planned,
				message: planned_publish_message(mode, request),
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: planned_trust_outcome(request, source, root, env_map),
			});
			continue;
		}

		if mode == PackagePublishRunMode::Release {
			enforce_release_trust_prerequisites(request, source, root, env_map)?;
		}

		let output = executor.run(&publish_command)?;
		if !output.success {
			return Err(MonochangeError::Discovery(format!(
				"`{}` failed: {}",
				render_command(&publish_command),
				render_command_error(&output)
			)));
		}

		let trusted_publishing = if !request.trusted_publishing.enabled {
			disabled_trust_outcome()
		} else if request.registry == RegistryKind::Npm {
			configure_npm_trusted_publishing(request, source, root, env_map, executor)?
		} else {
			manual_trust_outcome(request, source, root, env_map)
		};

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
		});
	}

	Ok(PackagePublishReport {
		mode,
		dry_run,
		packages: outcomes,
	})
}

fn planned_publish_message(mode: PackagePublishRunMode, request: &PublishRequest) -> String {
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

fn enforce_release_trust_prerequisites(
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	root: &Path,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<()> {
	if !request.trusted_publishing.enabled {
		return Ok(());
	}

	if request.registry == RegistryKind::Npm {
		resolve_github_trust_context(root, source, &request.trusted_publishing, env_map).map(|_| ())
	} else {
		let setup_url = manual_setup_url(request);
		match resolve_github_trust_context(root, source, &request.trusted_publishing, env_map) {
			Ok(context) => {
				Err(MonochangeError::Config(format!(
					"`{}` requires manual trusted publishing setup before built-in release publishing can continue: {}. Register {} there, then rerun `mc publish`.",
					request.package_id,
					setup_url,
					format_manual_trust_context(&context),
				)))
			}
			Err(error) => {
				Err(MonochangeError::Config(format!(
					"`{}` requires trusted-publishing preflight configuration before built-in release publishing can continue: {}. Finish the GitHub context configuration first, then complete registry setup at {} and rerun `mc publish`.",
					request.package_id, error, setup_url,
				)))
			}
		}
	}
}

fn trust_outcome_for_skip(
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	root: &Path,
	env_map: &BTreeMap<String, String>,
) -> TrustedPublishingOutcome {
	if !request.trusted_publishing.enabled {
		disabled_trust_outcome()
	} else if request.registry == RegistryKind::Npm {
		match resolve_github_trust_context(root, source, &request.trusted_publishing, env_map) {
			Ok(context) => {
				let command = render_npm_trust_command(request, &context);
				TrustedPublishingOutcome {
					status: TrustedPublishingStatus::Configured,
					repository: Some(context.repository),
					workflow: Some(context.workflow),
					environment: context.environment,
					setup_url: Some(manual_setup_url(request)),
					message: format!(
						"npm trusted publishing is expected for this package; rerun `{command}` if you need to repair it manually"
					),
				}
			}
			Err(_) => planned_trust_outcome(request, source, root, env_map),
		}
	} else {
		manual_trust_outcome(request, source, root, env_map)
	}
}

fn planned_trust_outcome(
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	root: &Path,
	env_map: &BTreeMap<String, String>,
) -> TrustedPublishingOutcome {
	if !request.trusted_publishing.enabled {
		disabled_trust_outcome()
	} else if request.registry == RegistryKind::Npm {
		match resolve_github_trust_context(root, source, &request.trusted_publishing, env_map) {
			Ok(context) => {
				let command = render_npm_trust_command(request, &context);
				TrustedPublishingOutcome {
					status: TrustedPublishingStatus::Planned,
					repository: Some(context.repository),
					workflow: Some(context.workflow),
					environment: context.environment,
					setup_url: Some(manual_setup_url(request)),
					message: format!("would configure npm trusted publishing with `{command}`"),
				}
			}
			Err(_) => manual_trust_outcome(request, source, root, env_map),
		}
	} else {
		manual_trust_outcome(request, source, root, env_map)
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

fn build_npm_trust_list_command(request: &PublishRequest) -> CommandSpec {
	build_npm_cli_command(
		request,
		vec![
			"trust".to_string(),
			"list".to_string(),
			request.package_name.clone(),
			"--json".to_string(),
		],
	)
}

fn build_npm_trust_command(request: &PublishRequest, context: &GitHubTrustContext) -> CommandSpec {
	let mut args = vec![
		"trust".to_string(),
		"github".to_string(),
		request.package_name.clone(),
		"--file".to_string(),
		context.workflow.clone(),
		"--repo".to_string(),
		context.repository.clone(),
		"--yes".to_string(),
	];
	append_npm_trust_environment_arg(&mut args, context.environment.as_ref());
	build_npm_cli_command(request, args)
}

fn render_npm_trust_command(request: &PublishRequest, context: &GitHubTrustContext) -> String {
	render_command(&build_npm_trust_command(request, context))
}

fn append_npm_trust_environment_arg(args: &mut Vec<String>, environment: Option<&String>) {
	let Some(environment) = environment else {
		return;
	};
	args.extend(["--env".to_string(), environment.clone()]);
}

fn build_npm_cli_command(request: &PublishRequest, args: Vec<String>) -> CommandSpec {
	if uses_pnpm_publish_manager(request) {
		let mut wrapped_args = vec!["exec".to_string(), "npm".to_string()];
		wrapped_args.extend(args);
		return CommandSpec {
			program: "pnpm".to_string(),
			args: wrapped_args,
			cwd: request.package_root.clone(),
		};
	}

	CommandSpec {
		program: "npm".to_string(),
		args,
		cwd: request.package_root.clone(),
	}
}

fn uses_pnpm_publish_manager(request: &PublishRequest) -> bool {
	request.registry == RegistryKind::Npm && request.package_manager.as_deref() == Some("pnpm")
}

fn resolve_github_trust_context(
	root: &Path,
	source: Option<&SourceConfiguration>,
	trust: &TrustedPublishingSettings,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<GitHubTrustContext> {
	let repository = trust
		.repository
		.clone()
		.or_else(|| source.map(|source| format!("{}/{}", source.owner, source.repo)))
		.or_else(|| env_map.get("GITHUB_REPOSITORY").cloned())
		.ok_or_else(|| {
			MonochangeError::Config(
				"trusted publishing could not determine the GitHub repository; set `publish.trusted_publishing.repository`".to_string(),
			)
		})?;

	let workflow = trust
		.workflow
		.clone()
		.or_else(|| {
			env_map
				.get("GITHUB_WORKFLOW_REF")
				.and_then(|value| parse_github_workflow_ref(value))
		})
		.ok_or_else(|| {
			MonochangeError::Config(
				"trusted publishing could not determine the GitHub workflow; set `publish.trusted_publishing.workflow`".to_string(),
			)
		})?;

	let environment = trust
		.environment
		.clone()
		.or_else(|| resolve_github_job_environment(root, &workflow, env_map));

	Ok(GitHubTrustContext {
		repository,
		workflow,
		environment,
	})
}

fn parse_github_workflow_ref(value: &str) -> Option<String> {
	let (_, path_and_ref) = value.split_once('/')?;
	let (_, path_and_ref) = path_and_ref.split_once('/')?;
	let (_, path_and_ref) = path_and_ref.split_once('/')?;
	let (workflow_path, _) = path_and_ref.split_once('@')?;
	Path::new(workflow_path)
		.file_name()
		.and_then(|name| name.to_str())
		.map(ToString::to_string)
}

fn resolve_github_job_environment(
	root: &Path,
	workflow: &str,
	env_map: &BTreeMap<String, String>,
) -> Option<String> {
	let job_id = env_map.get("GITHUB_JOB")?;
	let workflow_path = root.join(".github/workflows").join(workflow);
	let contents = fs::read_to_string(workflow_path).ok()?;
	let parsed = serde_yaml_ng::from_str::<YamlValue>(&contents).ok()?;
	let jobs = parsed.get("jobs")?;
	let job = jobs.get(job_id.as_str())?;
	match job.get("environment") {
		Some(YamlValue::String(environment)) => Some(environment.clone()),
		Some(YamlValue::Mapping(mapping)) => {
			mapping
				.get(YamlValue::String("name".to_string()))
				.and_then(YamlValue::as_str)
				.map(ToString::to_string)
		}
		_ => None,
	}
}

fn trust_list_contains_context(output: &str, context: &GitHubTrustContext) -> bool {
	if let Ok(json) = serde_json::from_str::<JsonValue>(output) {
		let mut required = vec![context.repository.as_str(), context.workflow.as_str()];
		if let Some(environment) = &context.environment {
			required.push(environment.as_str());
		}
		return required
			.into_iter()
			.all(|needle| json_value_contains(&json, needle));
	}

	output.contains(&context.repository)
		&& output.contains(&context.workflow)
		&& context
			.environment
			.as_deref()
			.is_none_or(|environment| output.contains(environment))
}

fn json_value_contains(value: &JsonValue, needle: &str) -> bool {
	match value {
		JsonValue::String(text) => text.contains(needle),
		JsonValue::Array(items) => items.iter().any(|item| json_value_contains(item, needle)),
		JsonValue::Object(map) => map.values().any(|value| json_value_contains(value, needle)),
		_ => false,
	}
}

fn build_publish_command(
	request: &PublishRequest,
	mode: PackagePublishRunMode,
	placeholder_dir: Option<&TempDir>,
	dry_run: bool,
) -> CommandSpec {
	let mut command = None;
	let is_jsr_release =
		request.registry == RegistryKind::Jsr && mode == PackagePublishRunMode::Release;
	let placeholder_path = placeholder_dir.map(TempDir::path);
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
	}
	if is_jsr_release {
		command = Some(build_jsr_publish_command(&request.package_root));
	}

	let mut command = command.expect("unsupported built-in publish registry");
	append_publish_dry_run_args(&mut command.args, request.registry, dry_run);
	command
}

fn append_publish_dry_run_args(args: &mut Vec<String>, registry: RegistryKind, dry_run: bool) {
	if !dry_run {
		return;
	}

	match registry {
		RegistryKind::Npm | RegistryKind::CratesIo | RegistryKind::Jsr => {
			args.push("--dry-run".to_string());
		}
		RegistryKind::PubDev => {
			args.retain(|arg| arg != "--force");
			args.push("--dry-run".to_string());
		}
		_ => {}
	}
}

fn build_npm_placeholder_publish_command(
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

fn build_npm_release_publish_command(request: &PublishRequest) -> CommandSpec {
	CommandSpec {
		program: npm_publish_program(request).to_string(),
		args: vec![
			"publish".to_string(),
			"--access".to_string(),
			"public".to_string(),
		],
		cwd: request.package_root.clone(),
	}
}

fn npm_publish_program(request: &PublishRequest) -> &'static str {
	if uses_pnpm_publish_manager(request) {
		"pnpm"
	} else {
		"npm"
	}
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

fn build_placeholder_directory(
	root: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<TempDir> {
	let tempdir = tempfile::tempdir().map_err(|error| placeholder_tempdir_error(&error))?;
	fs::write(
		tempdir.path().join("README.md"),
		&request.placeholder_readme,
	)
	.map_err(|error| MonochangeError::Io(format!("failed to write placeholder README: {error}")))?;

	let mut manifest_result = None;
	let is_jsr_registry = request.registry == RegistryKind::Jsr;
	let tempdir_path = tempdir.path();
	if request.registry == RegistryKind::Npm {
		manifest_result = Some(write_npm_placeholder_manifest(
			tempdir_path,
			request,
			source,
		));
	} else if request.registry == RegistryKind::CratesIo {
		manifest_result = Some(write_cargo_placeholder_manifest(
			tempdir_path,
			request,
			root,
			source,
		));
	} else if request.registry == RegistryKind::PubDev {
		manifest_result = Some(write_dart_placeholder_manifest(
			tempdir_path,
			request,
			source,
		));
	}
	if is_jsr_registry {
		manifest_result = Some(write_jsr_placeholder_manifest(
			tempdir_path,
			request,
			source,
		));
	}
	manifest_result.expect("unsupported built-in publish registry")?;

	Ok(tempdir)
}

fn write_npm_placeholder_manifest(
	dir: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	let mut manifest = serde_json::Map::new();
	manifest.insert(
		"name".to_string(),
		JsonValue::String(request.package_name.clone()),
	);
	manifest.insert(
		"version".to_string(),
		JsonValue::String(request.version.clone()),
	);
	manifest.insert(
		"description".to_string(),
		JsonValue::String(format!("Placeholder package for {}", request.package_name)),
	);
	if let Some(source) = source {
		manifest.insert(
			"repository".to_string(),
			JsonValue::String(format!(
				"https://github.com/{}/{}",
				source.owner, source.repo
			)),
		);
	}
	fs::write(
		dir.join("package.json"),
		JsonValue::Object(manifest).to_string(),
	)
	.map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder package.json: {error}"))
	})
}

fn write_cargo_placeholder_manifest(
	dir: &Path,
	request: &PublishRequest,
	root: &Path,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	let contents = fs::read_to_string(&request.manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read Cargo manifest {}: {error}",
			request.manifest_path.display()
		))
	})?;
	let parsed = toml::from_str::<TomlValue>(&contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {}: {error}",
			request.manifest_path.display()
		))
	})?;
	let package = parsed
		.get("package")
		.and_then(TomlValue::as_table)
		.ok_or_else(|| {
			MonochangeError::Config(format!(
				"{} is missing [package]",
				request.manifest_path.display()
			))
		})?;
	let (license, license_file) = resolve_cargo_placeholder_license_metadata(package, root)?;
	let package_license_file = package
		.get("license-file")
		.and_then(TomlValue::as_str)
		.map(ToString::to_string);
	if license.is_none() && license_file.is_none() {
		return Err(MonochangeError::Config(format!(
			"`{}` placeholder publishing requires `package.license` or `package.license-file`",
			request.package_id
		)));
	}

	let description = package
		.get("description")
		.and_then(TomlValue::as_str)
		.unwrap_or("Placeholder crate published by monochange");
	let edition = package
		.get("edition")
		.and_then(TomlValue::as_str)
		.unwrap_or("2021");
	let repository = package
		.get("repository")
		.and_then(TomlValue::as_str)
		.map(ToString::to_string)
		.or_else(|| {
			source.map(|source| format!("https://github.com/{}/{}", source.owner, source.repo))
		});

	let mut manifest = format!(
		"[package]\nname = \"{}\"\nversion = \"{}\"\nedition = \"{}\"\ndescription = \"{}\"\nreadme = \"README.md\"\n",
		request.package_name, request.version, edition, description
	);
	if let Some(license) = license {
		let _ = writeln!(manifest, "license = \"{license}\"");
	}
	if let Some(license_file) = license_file {
		manifest.push_str("license-file = \"LICENSE\"\n");
		let source_root = if package_license_file.as_deref() == Some(license_file.as_str()) {
			request.package_root.as_path()
		} else {
			root
		};
		let source_path = source_root.join(&license_file);
		let resolved_source = if source_path.is_absolute() {
			source_path
		} else {
			root.join(source_path)
		};
		fs::copy(&resolved_source, dir.join("LICENSE")).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to copy placeholder license file {}: {error}",
				resolved_source.display()
			))
		})?;
	}
	if let Some(repository) = repository {
		let _ = writeln!(manifest, "repository = \"{repository}\"");
	}
	fs::create_dir_all(dir.join("src")).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create placeholder src directory: {error}"
		))
	})?;
	fs::write(
		dir.join("src/lib.rs"),
		"//! Placeholder crate published by monochange.\n",
	)
	.map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder src/lib.rs: {error}"))
	})?;
	fs::write(dir.join("Cargo.toml"), manifest).map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder Cargo.toml: {error}"))
	})
}

fn resolve_cargo_placeholder_license_metadata(
	package: &toml::map::Map<String, TomlValue>,
	root: &Path,
) -> MonochangeResult<(Option<String>, Option<String>)> {
	let license = package
		.get("license")
		.and_then(TomlValue::as_str)
		.map(ToString::to_string);
	let license_file = package
		.get("license-file")
		.and_then(TomlValue::as_str)
		.map(ToString::to_string);
	if license.is_some() || license_file.is_some() {
		return Ok((license, license_file));
	}

	let workspace_package = read_workspace_package_table(root)?;
	let workspace_license = workspace_package
		.as_ref()
		.and_then(|package| package.get("license"))
		.and_then(TomlValue::as_str)
		.map(ToString::to_string);
	let workspace_license_file = workspace_package
		.as_ref()
		.and_then(|package| package.get("license-file"))
		.and_then(TomlValue::as_str)
		.map(ToString::to_string);
	Ok((workspace_license, workspace_license_file))
}

fn read_workspace_package_table(
	root: &Path,
) -> MonochangeResult<Option<toml::map::Map<String, TomlValue>>> {
	let workspace_manifest_path = root.join("Cargo.toml");
	if !workspace_manifest_path.is_file() {
		return Ok(None);
	}

	let contents = fs::read_to_string(&workspace_manifest_path).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read Cargo manifest {}: {error}",
			workspace_manifest_path.display()
		))
	})?;
	let parsed = toml::from_str::<TomlValue>(&contents).map_err(|error| {
		MonochangeError::Config(format!(
			"failed to parse {}: {error}",
			workspace_manifest_path.display()
		))
	})?;
	let workspace_package = parsed
		.get("workspace")
		.and_then(TomlValue::as_table)
		.and_then(|workspace| workspace.get("package"))
		.and_then(TomlValue::as_table)
		.cloned();
	Ok(workspace_package)
}

fn write_dart_placeholder_manifest(
	dir: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	let repository =
		source.map(|source| format!("https://github.com/{}/{}", source.owner, source.repo));
	let mut rendered = format!(
		"name: {}\nversion: {}\ndescription: Placeholder package published by monochange.\n",
		request.package_name, request.version
	);
	if let Some(repository) = repository {
		let _ = writeln!(rendered, "repository: {repository}");
	}
	fs::write(dir.join("pubspec.yaml"), rendered).map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder pubspec.yaml: {error}"))
	})
}

fn write_jsr_placeholder_manifest(
	dir: &Path,
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
) -> MonochangeResult<()> {
	let mut manifest = serde_json::Map::new();
	manifest.insert(
		"name".to_string(),
		JsonValue::String(request.package_name.clone()),
	);
	manifest.insert(
		"version".to_string(),
		JsonValue::String(request.version.clone()),
	);
	manifest.insert(
		"exports".to_string(),
		JsonValue::Object(
			[(".".to_string(), JsonValue::String("./mod.ts".to_string()))]
				.into_iter()
				.collect(),
		),
	);
	if let Some(source) = source {
		manifest.insert(
			"repository".to_string(),
			JsonValue::String(format!(
				"https://github.com/{}/{}",
				source.owner, source.repo
			)),
		);
	}
	fs::write(
		dir.join("deno.json"),
		JsonValue::Object(manifest).to_string(),
	)
	.map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder deno.json: {error}"))
	})?;
	fs::write(dir.join("mod.ts"), "export {};\n").map_err(|error| {
		MonochangeError::Io(format!("failed to write placeholder mod.ts: {error}"))
	})
}

fn placeholder_tempdir_error(error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!("failed to create placeholder tempdir: {error}"))
}

fn registry_version_exists(
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

fn crates_io_version_exists(
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

fn crates_io_index_version_exists(
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

fn crates_io_index_entry_path(package_name: &str) -> String {
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

fn disabled_trust_outcome() -> TrustedPublishingOutcome {
	TrustedPublishingOutcome {
		status: TrustedPublishingStatus::Disabled,
		repository: None,
		workflow: None,
		environment: None,
		setup_url: None,
		message: "trusted publishing disabled".to_string(),
	}
}

fn manual_trust_outcome(
	request: &PublishRequest,
	source: Option<&SourceConfiguration>,
	root: &Path,
	env_map: &BTreeMap<String, String>,
) -> TrustedPublishingOutcome {
	let setup_url = manual_setup_url(request);
	match resolve_github_trust_context(root, source, &request.trusted_publishing, env_map) {
		Ok(context) => {
			let message = if request.registry == RegistryKind::Npm {
				let command = render_npm_trust_command(request, &context);
				format!(
					"configure trusted publishing for `{}` before the next built-in release publish by running `{command}`; you can also open {} and register {} there",
					request.package_name,
					setup_url,
					format_manual_trust_context(&context),
				)
			} else {
				format!(
					"configure trusted publishing manually for `{}` before the next built-in release publish; open {} and register {} there",
					request.package_name,
					setup_url,
					format_manual_trust_context(&context),
				)
			};
			TrustedPublishingOutcome {
				status: TrustedPublishingStatus::ManualActionRequired,
				repository: Some(context.repository),
				workflow: Some(context.workflow),
				environment: context.environment,
				setup_url: Some(setup_url),
				message,
			}
		}
		Err(error) => {
			TrustedPublishingOutcome {
				status: TrustedPublishingStatus::ManualActionRequired,
				repository: request.trusted_publishing.repository.clone(),
				workflow: request.trusted_publishing.workflow.clone(),
				environment: request.trusted_publishing.environment.clone(),
				setup_url: Some(setup_url.clone()),
				message: format!(
					"configure trusted publishing manually for `{}` before the next built-in release publish; open {} and finish the GitHub context setup first: {}",
					request.package_name, setup_url, error,
				),
			}
		}
	}
}

fn format_manual_trust_context(context: &GitHubTrustContext) -> String {
	let mut parts = vec![
		format!("repository `{}`", context.repository),
		format!("workflow `{}`", context.workflow),
	];
	if let Some(environment) = &context.environment {
		parts.push(format!("environment `{environment}`"));
	}
	parts.join(", ")
}

fn manual_setup_url(request: &PublishRequest) -> String {
	if request.registry == RegistryKind::CratesIo {
		format!("https://crates.io/crates/{}", encode(&request.package_name))
	} else if request.registry == RegistryKind::PubDev {
		format!("https://pub.dev/packages/{}/admin", request.package_name)
	} else if request.registry == RegistryKind::Jsr {
		format!("https://jsr.io/{}", request.package_name)
	} else {
		format!(
			"https://www.npmjs.com/package/{}/access",
			request.package_name
		)
	}
}

#[cfg(test)]
fn trust_docs_url(registry: RegistryKind) -> &'static str {
	(if registry == RegistryKind::CratesIo {
		CRATES_TRUST_DOCS_URL
	} else if registry == RegistryKind::PubDev {
		DART_TRUST_DOCS_URL
	} else if registry == RegistryKind::Jsr {
		JSR_TRUST_DOCS_URL
	} else {
		NPM_TRUST_DOCS_URL
	}) as _
}

fn render_command(spec: &CommandSpec) -> String {
	std::iter::once(spec.program.as_str())
		.chain(spec.args.iter().map(String::as_str))
		.collect::<Vec<_>>()
		.join(" ")
}

fn render_command_error(output: &CommandOutput) -> String {
	if output.stderr.is_empty() {
		"command failed".to_string()
	} else {
		output.stderr.clone()
	}
}

#[cfg(test)]
#[allow(clippy::disallowed_methods, clippy::cloned_ref_to_slice_refs)]
mod tests {
	use std::collections::BTreeSet;
	use std::collections::VecDeque;

	use httpmock::Method::GET;
	use httpmock::MockServer;
	use monochange_core::PackageRecord;
	use monochange_core::PublishRegistry;
	use monochange_core::ReleaseRecord;
	use monochange_core::SourceProvider;
	use monochange_core::render_release_record_block;
	use monochange_test_helpers::git;
	use semver::Version;
	use temp_env::with_vars;

	use super::*;
	use crate::TEST_ENV_LOCK;

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
			self.outputs.pop_front().ok_or_else(|| {
				MonochangeError::Discovery("missing fake command output".to_string())
			})
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
			} else {
				Ecosystem::Deno
			},
			manifest_path: PathBuf::from("/workspace/pkg/manifest"),
			package_root: PathBuf::from("/workspace/pkg"),
			registry,
			package_manager: (registry == RegistryKind::Npm).then(|| "npm".to_string()),
			mode: PublishMode::Builtin,
			version: "1.2.3".to_string(),
			placeholder: false,
			trusted_publishing: TrustedPublishingSettings {
				enabled: false,
				repository: None,
				workflow: None,
				environment: None,
			},
			placeholder_readme: "placeholder".to_string(),
		}
	}

	fn sample_source() -> SourceConfiguration {
		SourceConfiguration {
			provider: SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
			api_url: None,
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings::default(),
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

	fn sample_endpoints(base_url: &str) -> RegistryEndpoints {
		RegistryEndpoints {
			npm_registry: base_url.to_string(),
			crates_io_api: base_url.to_string(),
			crates_io_index: base_url.to_string(),
			pub_dev_api: base_url.to_string(),
			jsr_base: base_url.to_string(),
		}
	}

	fn with_locked_env_vars<T>(action: impl FnOnce() -> T) -> T {
		let _env_lock = TEST_ENV_LOCK
			.lock()
			.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
		action()
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
		}
	}

	fn commit_release_record(root: &Path, publications: Vec<PackagePublicationTarget>) {
		let record = ReleaseRecord {
			schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
			kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
			created_at: "2026-04-14T08:00:00Z".to_string(),
			command: "release-pr".to_string(),
			version: Some("1.2.3".to_string()),
			group_version: None,
			release_targets: Vec::new(),
			released_packages: vec!["pkg".to_string()],
			changed_files: vec![PathBuf::from("tracked.txt")],
			package_publications: publications,
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			provider: None,
		};
		let block = render_release_record_block(&record).expect("render release record");
		fs::write(root.join("tracked.txt"), "release\n").expect("write tracked release file");
		git(root, &["add", "tracked.txt"]);
		git(
			root,
			[
				"commit",
				"--message",
				"chore(release): prepare release",
				"--message",
				block.as_str(),
			]
			.as_slice(),
		);
	}

	#[test]
	fn parse_github_workflow_ref_extracts_filename() {
		assert_eq!(
			parse_github_workflow_ref(
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main"
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
					"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
				),
				("GITHUB_JOB".to_string(), "release".to_string()),
			]),
		)
		.expect("context:");
		assert_eq!(context.repository, "ifiokjr/monochange");
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
			repository: "ifiokjr/monochange".to_string(),
			workflow: "publish.yml".to_string(),
			environment: Some("publisher".to_string()),
		};
		assert!(trust_list_contains_context(
			r#"{"publisher":{"repository":"ifiokjr/monochange","workflow":"publish.yml","environment":"publisher"}}"#,
			&context,
		));
		assert!(trust_list_contains_context(
			"repository ifiokjr/monochange workflow publish.yml environment publisher",
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
			repository: "ifiokjr/monochange".to_string(),
			workflow: "publish.yml".to_string(),
			environment: None,
		};
		assert!(trust_list_contains_context(
			r#"[{"repository":"ifiokjr/monochange"},{"workflow":"publish.yml"}]"#,
			&context,
		));
		assert!(!json_value_contains(&serde_json::json!(false), "publish"));
		assert!(!trust_list_contains_context(
			r#"{"repository":"ifiokjr/monochange"}"#,
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
		let requests = build_placeholder_requests(
			root.path(),
			&disabled,
			&[package.clone()],
			&BTreeSet::new(),
		)
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
		missing_readme.packages[0].publish.placeholder.readme_file =
			Some(PathBuf::from("missing.md"));
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
			Some(&tempdir),
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
			Some(&tempdir),
			false,
		);
		assert_eq!(pnpm_placeholder.program, "pnpm");
		let pnpm =
			build_publish_command(&pnpm_request, PackagePublishRunMode::Release, None, false);
		assert_eq!(pnpm.program, "pnpm");
		let cargo_placeholder = build_publish_command(
			&sample_request(RegistryKind::CratesIo),
			PackagePublishRunMode::Placeholder,
			Some(&tempdir),
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
			Some(&tempdir),
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
			Some(&tempdir),
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
	}

	#[test]
	fn build_publish_command_appends_dry_run_flags_for_supported_registries() {
		let tempdir = tempfile::tempdir().expect("tempdir:");

		let npm = build_publish_command(
			&sample_request(RegistryKind::Npm),
			PackagePublishRunMode::Placeholder,
			Some(&tempdir),
			true,
		);
		assert_eq!(npm.args.last(), Some(&"--dry-run".to_string()));

		let cargo = build_publish_command(
			&sample_request(RegistryKind::CratesIo),
			PackagePublishRunMode::Placeholder,
			Some(&tempdir),
			true,
		);
		assert_eq!(cargo.args.last(), Some(&"--dry-run".to_string()));

		let dart = build_publish_command(
			&sample_request(RegistryKind::PubDev),
			PackagePublishRunMode::Placeholder,
			Some(&tempdir),
			true,
		);
		assert!(dart.args.contains(&"--dry-run".to_string()));
		assert!(!dart.args.contains(&"--force".to_string()));

		let jsr = build_publish_command(
			&sample_request(RegistryKind::Jsr),
			PackagePublishRunMode::Placeholder,
			Some(&tempdir),
			true,
		);
		assert_eq!(jsr.args.last(), Some(&"--dry-run".to_string()));
	}

	#[test]
	fn build_npm_trust_commands_use_pnpm_exec_when_needed() {
		let request = PublishRequest {
			package_manager: Some("pnpm".to_string()),
			..sample_request(RegistryKind::Npm)
		};
		let list_command = build_npm_trust_list_command(&request);
		assert_eq!(list_command.program, "pnpm");
		assert_eq!(
			list_command.args,
			vec![
				"exec".to_string(),
				"npm".to_string(),
				"trust".to_string(),
				"list".to_string(),
				"pkg".to_string(),
				"--json".to_string(),
			]
		);

		let trust_command = build_npm_trust_command(
			&request,
			&GitHubTrustContext {
				repository: "ifiokjr/monochange".to_string(),
				workflow: "publish.yml".to_string(),
				environment: Some("publisher".to_string()),
			},
		);
		assert_eq!(trust_command.program, "pnpm");
		assert_eq!(
			trust_command.args,
			vec![
				"exec".to_string(),
				"npm".to_string(),
				"trust".to_string(),
				"github".to_string(),
				"pkg".to_string(),
				"--file".to_string(),
				"publish.yml".to_string(),
				"--repo".to_string(),
				"ifiokjr/monochange".to_string(),
				"--yes".to_string(),
				"--env".to_string(),
				"publisher".to_string(),
			]
		);
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
		assert_eq!(trust_docs_url(RegistryKind::Npm), NPM_TRUST_DOCS_URL);
		assert_eq!(
			trust_docs_url(RegistryKind::CratesIo),
			CRATES_TRUST_DOCS_URL
		);
		assert_eq!(trust_docs_url(RegistryKind::PubDev), DART_TRUST_DOCS_URL);
		assert_eq!(trust_docs_url(RegistryKind::Jsr), JSR_TRUST_DOCS_URL);
	}

	#[test]
	fn resolve_placeholder_readme_prefers_inline_then_file_then_default() {
		let tempdir = tempfile::tempdir().expect("tempdir:");
		fs::write(tempdir.path().join("README.md"), "from-file").expect("write readme:");
		assert_eq!(
			resolve_placeholder_readme(tempdir.path(), Some("inline"), None, "pkg")
				.expect("inline:"),
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
		let client = Client::builder().build().expect("http client:");
		let endpoints = RegistryEndpoints {
			npm_registry: server.base_url(),
			crates_io_api: server.base_url(),
			crates_io_index: server.base_url(),
			pub_dev_api: server.base_url(),
			jsr_base: server.base_url(),
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
			then.status(200).body(
				"{\"name\":\"pkg\",\"vers\":\"1.0.0\"}\n{\"name\":\"pkg\",\"vers\":\"1.2.3\"}\n",
			);
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
		let client = Client::builder().build().expect("http client:");
		let endpoints = RegistryEndpoints {
			npm_registry: server.base_url(),
			crates_io_api: server.base_url(),
			crates_io_index: server.base_url(),
			pub_dev_api: server.base_url(),
			jsr_base: server.base_url(),
		};
		let request = sample_request(RegistryKind::Npm);
		let request = PublishRequest {
			package_name: "missing".to_string(),
			..request
		};
		assert!(!registry_version_exists(&client, &endpoints, &request).expect("missing:"));
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
			placeholder_manifest.contains("repository = \"https://github.com/ifiokjr/monochange\"")
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

		let dart_error = write_dart_placeholder_manifest(
			&file_root,
			&sample_request(RegistryKind::PubDev),
			None,
		)
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
		let mkdir_error =
			write_cargo_placeholder_manifest(&file_root, &request, tempdir.path(), None)
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
		fs::create_dir_all(manifest_file_root.join("Cargo.toml"))
			.expect("create Cargo.toml directory");
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
			},
			PackagePublicationTarget {
				package: "pkg".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: "1.2.3".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
			},
		];

		let configuration =
			sample_configuration(&[("pkg", monochange_core::PackageType::Npm, true)]);

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
		let configuration =
			sample_configuration(&[("pkg", monochange_core::PackageType::Npm, true)]);
		let publications = vec![PackagePublicationTarget {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
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
			},
			PackagePublicationTarget {
				package: "disabled".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: "1.0.1".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
			},
			PackagePublicationTarget {
				package: "private".to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: "1.0.1".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
			},
		];

		let requests =
			build_release_requests(&configuration, &packages, &publications, &BTreeSet::new())
				.expect("requests");

		assert_eq!(requests.len(), 1);
		assert_eq!(requests[0].package_id, "public");
	}

	#[test]
	fn write_placeholder_directory_builds_npm_jsr_and_dart_scaffolds() {
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
			Some("https://github.com/ifiokjr/monochange")
		);

		let dart = build_placeholder_directory(
			tempdir.path(),
			&sample_request(RegistryKind::PubDev),
			Some(&sample_source()),
		)
		.expect("dart placeholder:");
		let pubspec =
			fs::read_to_string(dart.path().join("pubspec.yaml")).expect("read pubspec.yaml:");
		assert!(pubspec.contains("repository: https://github.com/ifiokjr/monochange"));

		let jsr = build_placeholder_directory(
			tempdir.path(),
			&sample_request(RegistryKind::Jsr),
			Some(&sample_source()),
		)
		.expect("jsr placeholder:");
		let deno_manifest =
			fs::read_to_string(jsr.path().join("deno.json")).expect("read deno.json:");
		let deno_manifest_json =
			serde_json::from_str::<JsonValue>(&deno_manifest).expect("parse deno.json");
		let deno_repository = deno_manifest_json
			.get("repository")
			.and_then(JsonValue::as_str);
		assert_eq!(
			deno_repository,
			Some("https://github.com/ifiokjr/monochange")
		);
	}

	#[test]
	fn planned_and_skip_trust_outcomes_cover_npm_and_manual_flows() {
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
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
		assert_eq!(manual.repository.as_deref(), Some("ifiokjr/monochange"));
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
	fn trust_outcome_for_skip_uses_manual_action_for_non_npm_packages() {
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
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
		assert_eq!(outcome.repository.as_deref(), Some("ifiokjr/monochange"));
		assert_eq!(outcome.workflow.as_deref(), Some("publish.yml"));
		assert_eq!(outcome.environment.as_deref(), Some("publisher"));
	}

	#[test]
	fn manual_trust_outcome_preserves_explicit_context_and_registry_setup_url() {
		let mut request = trusted_request(RegistryKind::PubDev);
		request.trusted_publishing.repository = Some("ifiokjr/monochange".to_string());
		request.trusted_publishing.workflow = Some("publish.yml".to_string());
		request.trusted_publishing.environment = Some("pub.dev".to_string());

		let outcome = manual_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());

		assert_eq!(
			outcome.status,
			TrustedPublishingStatus::ManualActionRequired
		);
		assert_eq!(outcome.repository.as_deref(), Some("ifiokjr/monochange"));
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
			"register repository `ifiokjr/monochange`, workflow `publish.yml`, environment `pub.dev`"
		));
	}

	#[test]
	fn manual_trust_outcome_includes_copyable_npm_trust_command_when_context_is_known() {
		let mut request = trusted_request(RegistryKind::Npm);
		request.trusted_publishing.repository = Some("ifiokjr/monochange".to_string());
		request.trusted_publishing.workflow = Some("publish.yml".to_string());
		request.trusted_publishing.environment = Some("publisher".to_string());

		let outcome = manual_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());

		assert_eq!(
			outcome.status,
			TrustedPublishingStatus::ManualActionRequired
		);
		assert!(outcome.message.contains(
			"npm trust github pkg --file publish.yml --repo ifiokjr/monochange --yes --env publisher"
		));
	}

	#[test]
	fn planned_trust_outcome_includes_copyable_npm_trust_command_when_context_is_known() {
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
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
			"would configure npm trusted publishing with `npm trust github pkg --file publish.yml --repo ifiokjr/monochange --yes --env publisher`"
		));
	}

	#[test]
	fn manual_trust_outcome_reports_missing_github_context_configuration() {
		let mut request = trusted_request(RegistryKind::Jsr);
		request.trusted_publishing.repository = Some("ifiokjr/monochange".to_string());

		let outcome = manual_trust_outcome(&request, None, Path::new("."), &BTreeMap::new());

		assert_eq!(
			outcome.status,
			TrustedPublishingStatus::ManualActionRequired
		);
		assert_eq!(outcome.repository.as_deref(), Some("ifiokjr/monochange"));
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
	fn configure_npm_trusted_publishing_creates_configuration_when_missing() {
		let request = sample_request(RegistryKind::Npm);
		let root = tempfile::tempdir().expect("tempdir:");
		let workflows = root.path().join(".github/workflows");
		fs::create_dir_all(&workflows).expect("mkdir:");
		fs::write(
			workflows.join("publish.yml"),
			"jobs:\n  release:\n    environment: publisher\n",
		)
		.expect("write workflow:");
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]);
		let mut executor = FakeExecutor::new(vec![
			CommandOutput {
				success: true,
				stdout: "[]".to_string(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: String::new(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: r#"{"repository":"ifiokjr/monochange","workflow":"publish.yml","environment":"publisher"}"#.to_string(),
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
		.expect("npm trust:");

		assert_eq!(outcome.status, TrustedPublishingStatus::Configured);
		assert_eq!(
			outcome.setup_url.as_deref(),
			Some("https://www.npmjs.com/package/pkg/access")
		);
		assert_eq!(executor.commands.len(), 3);
		assert_eq!(executor.commands[1].program, "npm");
		assert!(executor.commands[1].args.contains(&"github".to_string()));
	}

	#[test]
	fn configure_npm_trusted_publishing_short_circuits_when_already_configured() {
		let request = trusted_request(RegistryKind::Npm);
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]);
		let mut executor = FakeExecutor::new(vec![CommandOutput {
			success: true,
			stdout: r#"{"repository":"ifiokjr/monochange","workflow":"publish.yml","environment":"publisher"}"#.to_string(),
			stderr: String::new(),
		}]);

		let outcome = configure_npm_trusted_publishing(
			&request,
			Some(&sample_source()),
			root.path(),
			&env_map,
			&mut executor,
		)
		.expect("npm trust:");

		assert_eq!(outcome.status, TrustedPublishingStatus::Configured);
		assert_eq!(
			outcome.setup_url.as_deref(),
			Some("https://www.npmjs.com/package/pkg/access")
		);
		assert_eq!(executor.commands.len(), 1);
	}

	#[test]
	fn configure_npm_trusted_publishing_reports_trust_command_failures() {
		let request = trusted_request(RegistryKind::Npm);
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]);
		let mut executor = FakeExecutor::new(vec![
			CommandOutput {
				success: true,
				stdout: "[]".to_string(),
				stderr: String::new(),
			},
			CommandOutput {
				success: false,
				stdout: String::new(),
				stderr: "trust failed".to_string(),
			},
		]);

		let error = configure_npm_trusted_publishing(
			&request,
			Some(&sample_source()),
			root.path(),
			&env_map,
			&mut executor,
		)
		.expect_err("expected npm trust failure");
		assert!(error.to_string().contains("trust failed"));
	}

	#[test]
	fn configure_npm_trusted_publishing_requires_post_command_verification() {
		let request = trusted_request(RegistryKind::Npm);
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]);
		let mut executor = FakeExecutor::new(vec![
			CommandOutput {
				success: true,
				stdout: "[]".to_string(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: String::new(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: "[]".to_string(),
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
		.expect_err("expected npm verify failure");
		assert!(error.to_string().contains("could not be verified"));
	}

	#[test]
	fn enforce_release_trust_prerequisites_accepts_npm_and_rejects_manual_registries() {
		let root = workflow_root();
		let env_map = BTreeMap::from([
			(
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]);

		enforce_release_trust_prerequisites(
			&trusted_request(RegistryKind::Npm),
			Some(&sample_source()),
			root.path(),
			&env_map,
		)
		.expect("expected npm trust prereq success:");

		let manual_error = enforce_release_trust_prerequisites(
			&trusted_request(RegistryKind::CratesIo),
			Some(&sample_source()),
			root.path(),
			&env_map,
		)
		.expect_err("expected manual trust error");
		assert!(
			manual_error
				.to_string()
				.contains("manual trusted publishing setup")
		);
		assert!(manual_error.to_string().contains(
			"repository `ifiokjr/monochange`, workflow `publish.yml`, environment `publisher`"
		));

		enforce_release_trust_prerequisites(
			&sample_request(RegistryKind::Npm),
			None,
			root.path(),
			&BTreeMap::new(),
		)
		.expect("expected disabled trust success:");

		let mut missing_workflow_request = trusted_request(RegistryKind::PubDev);
		missing_workflow_request.trusted_publishing.repository =
			Some("ifiokjr/monochange".to_string());
		let missing_context_error = enforce_release_trust_prerequisites(
			&missing_workflow_request,
			None,
			root.path(),
			&BTreeMap::new(),
		)
		.expect_err("expected missing context error");
		assert!(
			missing_context_error
				.to_string()
				.contains("trusted-publishing preflight configuration")
		);
		assert!(
			missing_context_error
				.to_string()
				.contains("set `publish.trusted_publishing.workflow`")
		);
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
	fn execute_publish_requests_publishes_release_and_configures_npm_trust() {
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
				"GITHUB_WORKFLOW_REF".to_string(),
				"ifiokjr/monochange/.github/workflows/publish.yml@refs/heads/main".to_string(),
			),
			("GITHUB_JOB".to_string(), "release".to_string()),
		]);
		let mut executor = FakeExecutor::new(vec![
			CommandOutput {
				success: true,
				stdout: String::new(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: "[]".to_string(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: String::new(),
				stderr: String::new(),
			},
			CommandOutput {
				success: true,
				stdout: r#"{"repository":"ifiokjr/monochange","workflow":"publish.yml","environment":"publisher"}"#.to_string(),
				stderr: String::new(),
			},
		]);

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
		assert_eq!(executor.commands.len(), 4);
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
		assert_eq!(executor.commands.len(), 1);
		assert_eq!(
			executor.commands[0].args.last(),
			Some(&"--dry-run".to_string())
		);
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
		assert!(error.to_string().contains(
			"placeholder publishing requires `package.license` or `package.license-file`"
		));
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

		let error = execute_publish_requests(
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
		.expect_err("expected publish error");
		let text = error.to_string();
		assert!(text.contains("npm publish"));
		assert!(text.contains("boom"));
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
		let configuration =
			crate::load_workspace_configuration(root.path()).expect("configuration:");

		with_locked_env_vars(|| {
			with_vars(
				vec![(
					"MONOCHANGE_NPM_REGISTRY_URL",
					Some(server.base_url().as_str()),
				)],
				|| {
					let report = run_placeholder_publish(
						root.path(),
						&configuration,
						&BTreeSet::new(),
						true,
					)
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
		let configuration =
			crate::load_workspace_configuration(root.path()).expect("configuration:");
		let prepared_release = sample_prepared_release(
			root.path(),
			vec![PackagePublicationTarget {
				package: "pkg".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: "1.2.3".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
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
		let configuration =
			crate::load_workspace_configuration(root.path()).expect("configuration:");
		commit_release_record(
			root.path(),
			vec![PackagePublicationTarget {
				package: "pkg".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: "1.2.3".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
			}],
		);
		let discovered =
			release_record_package_publications_from_prepared_or_head(root.path(), None)
				.expect("release record publications");
		assert_eq!(discovered.len(), 1);

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
						None,
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
			metadata: BTreeMap::from([("config_id".to_string(), "pkg".to_string())]),
			declared_dependencies: Vec::new(),
		};
		let publication = PackagePublicationTarget {
			package: "pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
		};
		let configuration =
			sample_configuration(&[("pkg", monochange_core::PackageType::Npm, true)]);
		let requests =
			build_release_requests(&configuration, &[package], &[publication], &BTreeSet::new())
				.expect("requests:");
		assert_eq!(requests.len(), 1);
		assert_eq!(requests[0].version, "1.2.3");
		assert_eq!(requests[0].package_name, "pkg");
	}
}
