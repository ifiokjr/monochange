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
pub(crate) use monochange_core::PublishMode;
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
pub(crate) use monochange_publish::PLACEHOLDER_VERSION;
pub(crate) use monochange_publish::PackagePublishOutcome;
pub(crate) use monochange_publish::PackagePublishReport;
pub(crate) use monochange_publish::PackagePublishRunMode;
pub(crate) use monochange_publish::PackagePublishStatus;
use monochange_publish::PlaceholderManifestWriterRegistry;
pub(crate) use monochange_publish::ProcessCommandExecutor;
use monochange_publish::PublishReadinessRegistry;
pub(crate) use monochange_publish::PublishRequest;
use monochange_publish::PublishTrustHandler;
use monochange_publish::RegistryEndpoints;
use monochange_publish::TrustedPublishingIdentity;
pub(crate) use monochange_publish::TrustedPublishingOutcome;
pub(crate) use monochange_publish::TrustedPublishingStatus;
use monochange_publish::build_placeholder_directory as build_placeholder_directory_with_writers;
pub(crate) use monochange_publish::build_placeholder_requests;
pub(crate) use monochange_publish::build_publish_command;
use monochange_publish::build_publish_command_builder;
pub(crate) use monochange_publish::build_release_requests;
pub(crate) use monochange_publish::default_registry_kind_for_ecosystem;
use monochange_publish::detect_trusted_publishing_identity;
use monochange_publish::disabled_trust_outcome;
use monochange_publish::enforce_release_attestation_prerequisites as enforce_release_attestation_prerequisites_impl;
use monochange_publish::execute_publish_requests as execute_publish_requests_impl;
use monochange_publish::execute_publish_requests_with_process;
pub(crate) use monochange_publish::forbidden_npm_token_env_keys;
use monochange_publish::manual_setup_url;
use monochange_publish::merge_publish_resume_report;
use monochange_publish::provider_registry_trust_capability;
use monochange_publish::read_publish_report_artifact;
pub(crate) use monochange_publish::registry_version_exists;
use monochange_publish::reject_npm_token_environment;
use monochange_publish::render_command;
use monochange_publish::render_command_error;
pub(crate) use monochange_publish::resolve_placeholder_readme;
pub(crate) use monochange_publish::resolve_registry_kind;
use monochange_publish::resume_publish_requests;
use monochange_publish::select_release_publication_targets;
use monochange_publish::trusted_publishing_capability_message;
use monochange_publish::trusted_publishing_capability_message_for_builtin;
use monochange_python::write_python_placeholder_manifest;
use reqwest::blocking::Client;
use tempfile::TempDir;

use crate::PreparedRelease;
use crate::discover_release_record;
use crate::discover_workspace;

pub(crate) fn run_placeholder_publish(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	selected_packages: &BTreeSet<String>,
	dry_run: bool,
) -> MonochangeResult<PackagePublishReport> {
	let discovery = discover_workspace(root)?;
	let requests =
		build_placeholder_requests(root, configuration, &discovery.packages, selected_packages)?;
	execute_publish_requests_with_process(
		root,
		configuration.source.as_ref(),
		PackagePublishRunMode::Placeholder,
		dry_run,
		&requests,
		&build_publish_command_builder(),
		&placeholder_manifest_writer_registry(),
		&publish_readiness_registry(),
		&CliPublishTrustHandler,
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
	let publication_targets =
		release_record_package_publications_from_prepared_or_head(root, prepared_release)?;
	let selected_targets = select_release_publication_targets(
		&configuration.groups,
		&publication_targets,
		selected_packages,
		selected_groups,
		selected_ecosystems,
	);

	run_publish_packages_with_publications_and_resume(
		root,
		configuration,
		&selected_targets.publication_targets,
		&selected_targets.selected_packages,
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
	execute_publish_requests_with_process(
		root,
		configuration.source.as_ref(),
		PackagePublishRunMode::Release,
		dry_run,
		requests,
		&build_publish_command_builder(),
		&placeholder_manifest_writer_registry(),
		&publish_readiness_registry(),
		&CliPublishTrustHandler,
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

struct CliPublishTrustHandler;

impl PublishTrustHandler for CliPublishTrustHandler {
	fn trust_outcome_for_skip(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		trust_outcome_for_skip(request, source, root, env_map)
	}

	fn planned_trust_outcome(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> TrustedPublishingOutcome {
		planned_trust_outcome(request, source, root, env_map)
	}

	fn enforce_release_trust_prerequisites(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
	) -> MonochangeResult<()> {
		enforce_release_trust_prerequisites(request, source, root, env_map)
	}

	fn configure_successful_publish_trust(
		&self,
		request: &PublishRequest,
		source: Option<&SourceConfiguration>,
		root: &Path,
		env_map: &BTreeMap<String, String>,
		executor: &mut dyn CommandExecutor,
	) -> MonochangeResult<TrustedPublishingOutcome> {
		if request.registry == RegistryKind::Npm {
			configure_npm_trusted_publishing(request, source, root, env_map, executor)
		} else {
			Ok(manual_trust_outcome(request, source, root, env_map))
		}
	}
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

pub(crate) fn enforce_release_attestation_prerequisites(
	request: &PublishRequest,
	env_map: &BTreeMap<String, String>,
) -> MonochangeResult<()> {
	enforce_release_attestation_prerequisites_impl(
		request,
		env_map,
		&build_publish_command_builder(),
	)
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

pub(crate) fn build_placeholder_directory(
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

fn publish_readiness_registry() -> PublishReadinessRegistry {
	PublishReadinessRegistry::new().with_checker(
		RegistryKind::CratesIo,
		Box::new(|root, request| {
			let blockers = cargo_publish_readiness_blockers(root, request)?;
			if blockers.is_empty() {
				Ok(None)
			} else {
				Ok(Some(publish_blocked_message(request, &blockers)))
			}
		}),
	)
}

fn placeholder_manifest_writer_registry() -> PlaceholderManifestWriterRegistry {
	PlaceholderManifestWriterRegistry::new()
		.with_writer(
			RegistryKind::Npm,
			Box::new(|placeholder_dir, request, _root, source| {
				write_npm_placeholder_manifest(placeholder_dir, request, source)
			}),
		)
		.with_writer(
			RegistryKind::CratesIo,
			Box::new(|placeholder_dir, request, root, source| {
				write_cargo_placeholder_manifest(placeholder_dir, request, root, source)
			}),
		)
		.with_writer(
			RegistryKind::PubDev,
			Box::new(|placeholder_dir, request, _root, source| {
				write_dart_placeholder_manifest(placeholder_dir, request, source)
			}),
		)
		.with_writer(
			RegistryKind::Jsr,
			Box::new(|placeholder_dir, request, _root, source| {
				write_jsr_placeholder_manifest(placeholder_dir, request, source)
			}),
		)
		.with_writer(
			RegistryKind::Pypi,
			Box::new(|placeholder_dir, request, _root, source| {
				write_python_placeholder_manifest(placeholder_dir, request, source)
			}),
		)
		.with_writer(
			RegistryKind::GoProxy,
			Box::new(|placeholder_dir, request, _root, _source| {
				write_go_placeholder_manifest(placeholder_dir, request)
			}),
		)
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

#[allow(clippy::disallowed_methods, clippy::cloned_ref_to_slice_refs)]
#[cfg(test)]
#[path = "__tests__/package_publish_tests.rs"]
mod tests;
