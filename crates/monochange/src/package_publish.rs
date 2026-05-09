use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::Path;

use monochange_cargo::cargo_publish_readiness_blockers;
use monochange_cargo::publish_blocked_message;
use monochange_cargo::write_cargo_placeholder_manifest;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackagePublicationTarget;
use monochange_core::PackageRecord;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::RegistryKind;
use monochange_core::SourceConfiguration;
use monochange_core::WorkspaceConfiguration;
use monochange_dart::write_dart_placeholder_manifest;
use monochange_deno::write_jsr_placeholder_manifest;
use monochange_github::format_manual_trust_context;
use monochange_github::resolve_github_trust_context;
use monochange_github::trust_list_contains_context;
use monochange_github::verify_github_trust_context;
use monochange_go::write_go_placeholder_manifest;
use monochange_npm::build_npm_trust_command;
use monochange_npm::build_npm_trust_list_command;
use monochange_npm::render_npm_trust_command;
use monochange_npm::write_npm_placeholder_manifest;
use monochange_publish::CommandExecutor;
pub(crate) use monochange_publish::PackagePublishOutcome;
pub(crate) use monochange_publish::PackagePublishReport;
pub(crate) use monochange_publish::PackagePublishRunMode;
pub(crate) use monochange_publish::PackagePublishStatus;
use monochange_publish::ProcessCommandExecutor;
use monochange_publish::PublishAdapter;
pub(crate) use monochange_publish::PublishRequest;
use monochange_publish::RegistryEndpoints;
use monochange_publish::TrustedPublishingIdentity;
pub(crate) use monochange_publish::TrustedPublishingOutcome;
pub(crate) use monochange_publish::TrustedPublishingStatus;
use monochange_publish::build_publish_command;
use monochange_publish::build_publish_command_builder;
use monochange_publish::detect_trusted_publishing_identity;
use monochange_publish::go_module_path;
use monochange_publish::merge_publish_resume_report;
use monochange_publish::order_release_requests_by_publish_dependencies;
use monochange_publish::package_can_be_published;
use monochange_publish::packages_by_config_id;
use monochange_publish::provider_registry_trust_capability;
use monochange_publish::read_publish_report_artifact;
use monochange_publish::registry_client;
use monochange_publish::registry_version_exists;
use monochange_publish::render_command;
use monochange_publish::render_command_error;
use monochange_publish::resume_publish_requests;
use monochange_publish::trusted_publishing_capability_message;
use monochange_publish::trusted_publishing_capability_message_for_builtin;
use monochange_python::write_python_placeholder_manifest;
use reqwest::blocking::Client;
use tempfile::TempDir;
use urlencoding::encode;

use crate::PreparedRelease;
use crate::discover_release_record;
use crate::discover_workspace;

const PLACEHOLDER_VERSION: &str = "0.0.0";

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
	run_publish_packages_with_resume(
		root,
		configuration,
		prepared_release,
		selected_packages,
		&BTreeSet::new(),
		&BTreeSet::new(),
		dry_run,
		None,
	)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn run_publish_packages_with_resume(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	selected_groups: &BTreeSet<String>,
	selected_ecosystems: &BTreeSet<Ecosystem>,
	dry_run: bool,
	resume_path: Option<&Path>,
) -> MonochangeResult<PackagePublishReport> {
	let mut publication_targets =
		release_record_package_publications_from_prepared_or_head(root, prepared_release)?;

	if !selected_ecosystems.is_empty() {
		publication_targets.retain(|t| selected_ecosystems.contains(&t.ecosystem));
	}

	let mut effective_selected_packages = selected_packages.clone();
	for group_id in selected_groups {
		if let Some(group) = configuration.group_by_id(group_id) {
			effective_selected_packages.extend(group.packages.iter().cloned());
		}
	}

	run_publish_packages_with_publications_and_resume(
		root,
		configuration,
		&publication_targets,
		&effective_selected_packages,
		dry_run,
		resume_path,
	)
}

pub(crate) fn run_publish_packages_with_publications(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	publication_targets: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
	dry_run: bool,
) -> MonochangeResult<PackagePublishReport> {
	run_publish_packages_with_publications_and_resume(
		root,
		configuration,
		publication_targets,
		selected_packages,
		dry_run,
		None,
	)
}

fn run_publish_packages_with_publications_and_resume(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	publication_targets: &[PackagePublicationTarget],
	selected_packages: &BTreeSet<String>,
	dry_run: bool,
	resume_path: Option<&Path>,
) -> MonochangeResult<PackagePublishReport> {
	let discovery = discover_workspace(root)?;
	let requests = build_release_requests(
		configuration,
		&discovery.packages,
		publication_targets,
		selected_packages,
	)?;
	let previous_report = resume_path.map(read_publish_report_artifact).transpose()?;
	let (requests, resumed_outcomes) =
		resume_publish_requests(&requests, previous_report.as_ref())?;
	let report = execute_release_publish_requests(root, configuration, dry_run, &requests)?;
	Ok(merge_publish_resume_report(
		PackagePublishRunMode::Release,
		dry_run,
		resumed_outcomes,
		report,
	))
}

fn execute_release_publish_requests(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	dry_run: bool,
	requests: &[PublishRequest],
) -> MonochangeResult<PackagePublishReport> {
	let env_map = current_env_map();
	let endpoints = RegistryEndpoints::from_env();
	let client = registry_client()?;
	let mut executor = ProcessCommandExecutor;
	execute_publish_requests(
		root,
		configuration.source.as_ref(),
		PackagePublishRunMode::Release,
		dry_run,
		requests,
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
	let parsed = ecosystem.parse::<Ecosystem>().map_err(|()| {
		MonochangeError::Config(format!(
			"built-in package publishing does not support ecosystem `{ecosystem}`"
		))
	})?;
	monochange_core::default_registry_kind_for_ecosystem(parsed).ok_or_else(|| {
		MonochangeError::Config(format!(
			"built-in package publishing does not support ecosystem `{ecosystem}`"
		))
	})
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
				command: None,
				stdout: None,
				stderr: None,
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
				command: None,
				stdout: None,
				stderr: None,
			});
			continue;
		}

		let blockers = if mode == PackagePublishRunMode::Release {
			cargo_publish_readiness_blockers(root, request)?
		} else {
			Vec::new()
		};
		if !blockers.is_empty() {
			let message = publish_blocked_message(request, &blockers);

			if dry_run {
				outcomes.push(PackagePublishOutcome {
					package: request.package_id.clone(),
					ecosystem: request.ecosystem,
					registry: request.registry.to_string(),
					version: request.version.clone(),
					status: PackagePublishStatus::Blocked,
					message,
					placeholder: mode == PackagePublishRunMode::Placeholder,
					trusted_publishing: planned_trust_outcome(request, source, root, env_map),
					command: None,
					stdout: None,
					stderr: None,
				});
				continue;
			}

			return Err(MonochangeError::Config(message));
		}

		let placeholder_dir = if mode == PackagePublishRunMode::Placeholder {
			Some(build_placeholder_directory(root, request, source)?)
		} else {
			None
		};
		let publish_command = build_publish_command(
			request,
			mode,
			placeholder_dir.as_ref().map(TempDir::path),
			dry_run,
		);

		if dry_run {
			outcomes.push(PackagePublishOutcome {
				package: request.package_id.clone(),
				ecosystem: request.ecosystem,
				registry: request.registry.to_string(),
				version: request.version.clone(),
				status: PackagePublishStatus::Planned,
				message: planned_publish_message(mode, request),
				placeholder: mode == PackagePublishRunMode::Placeholder,
				trusted_publishing: planned_trust_outcome(request, source, root, env_map),
				command: None,
				stdout: None,
				stderr: None,
			});
			continue;
		}

		if mode == PackagePublishRunMode::Release {
			enforce_release_trust_prerequisites(request, source, root, env_map)?;
			enforce_release_attestation_prerequisites(request, env_map)?;
		}

		let output = match executor.run(&publish_command) {
			Ok(output) => output,
			Err(error) => {
				outcomes.push(failed_publish_outcome(mode, request, error.to_string()));
				break;
			}
		};
		if !output.success {
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

fn failed_publish_outcome(
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

	let registry = PublishRegistry::Builtin(request.registry);
	let identity = detect_trusted_publishing_identity(env_map);
	let capability_message = trusted_publishing_capability_message(&registry, &identity);

	if !identity.is_verifiable_by_env() {
		return Err(MonochangeError::Config(format!(
			"`{}` requires trusted publishing from a verifiable CI/OIDC identity before built-in release publishing can continue; local/manual publishing is not allowed when `publish.trusted_publishing = true`. {capability_message} Run `mc publish` from the configured CI workflow or set `publish.trusted_publishing = false` to opt out.",
			request.package_id,
		)));
	}

	let capability = provider_registry_trust_capability(&registry, identity.provider());
	if !capability.trusted_publishing || !capability.ci_identity_verifiable {
		return Err(MonochangeError::Config(format!(
			"`{}` cannot enforce trusted publishing for {} from {}. {capability_message} Set `publish.trusted_publishing = false` to opt out for unsupported registries/providers.",
			request.package_id,
			request.registry,
			identity.provider().label(),
		)));
	}

	if request.registry == RegistryKind::Npm {
		reject_npm_token_environment(request, env_map)?;
	}

	let TrustedPublishingIdentity::GitHubActions {
		repository,
		workflow,
		environment,
		..
	} = identity
	else {
		return Ok(());
	};

	let expected = resolve_github_trust_context(root, source, &request.trusted_publishing, env_map)
		.map_err(|error| MonochangeError::Config(format!("{error}. {capability_message}")))?;
	verify_github_trust_context(
		request,
		root,
		env_map,
		&expected,
		repository.as_deref(),
		workflow.as_deref(),
		environment.as_deref(),
	)
}

fn enforce_release_attestation_prerequisites(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
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
	if !build_publish_command_builder()
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

fn reject_npm_token_environment(
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

fn forbidden_npm_token_env_keys(env_map: &BTreeMap<String, String>) -> Vec<String> {
	env_map
		.keys()
		.filter(|key| is_forbidden_npm_token_env_key(key))
		.cloned()
		.collect()
}

fn is_forbidden_npm_token_env_key(key: &str) -> bool {
	let lowercase_key = key.to_ascii_lowercase();
	matches!(
		key,
		"NPM_TOKEN" | "NODE_AUTH_TOKEN" | "NPM_CONFIG__AUTH_TOKEN" | "npm_config__authToken"
	) || (lowercase_key.starts_with("npm_config_")
		&& lowercase_key.contains("auth")
		&& lowercase_key.contains("token"))
}

#[allow(clippy::too_many_arguments)]
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
	} else if request.registry == RegistryKind::Pypi {
		manifest_result = Some(write_python_placeholder_manifest(
			tempdir_path,
			request,
			source,
		));
	} else if request.registry == RegistryKind::GoProxy {
		manifest_result = Some(write_go_placeholder_manifest(tempdir_path, request));
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

fn placeholder_tempdir_error(error: &std::io::Error) -> MonochangeError {
	MonochangeError::Io(format!("failed to create placeholder tempdir: {error}"))
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
			let capability_message =
				trusted_publishing_capability_message_for_builtin(request.registry, env_map);
			TrustedPublishingOutcome {
				status: TrustedPublishingStatus::ManualActionRequired,
				repository: request.trusted_publishing.repository.clone(),
				workflow: request.trusted_publishing.workflow.clone(),
				environment: request.trusted_publishing.environment.clone(),
				setup_url: Some(setup_url.clone()),
				message: format!(
					"configure trusted publishing manually for `{}` before the next built-in release publish; open {} and finish the GitHub context setup first: {}. {capability_message}",
					request.package_name, setup_url, error,
				),
			}
		}
	}
}

fn manual_setup_url(request: &PublishRequest) -> String {
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

fn non_empty_output(output: String) -> Option<String> {
	(!output.is_empty()).then_some(output)
}

#[cfg(test)]
#[allow(clippy::disallowed_methods, clippy::cloned_ref_to_slice_refs)]
#[path = "__tests__/package_publish_tests.rs"]
mod tests;
