use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::BufRead;
use std::io::BufReader;
use std::io::IsTerminal;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::ExitStatus;
use std::process::Stdio;
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;
use std::time::Instant;

use clap::ArgMatches;
use clap::parser::ValueSource;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::CommandVariable;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ShellConfig;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceChangeRequestOutcome;
use monochange_core::SourceConfiguration;
use monochange_core::SourceReleaseOutcome;
use monochange_core::SourceReleaseRequest;
use monochange_telemtry::CommandTelemetry;
use monochange_telemtry::StepTelemetry;
use monochange_telemtry::TelemetryOutcome;
use monochange_telemtry::TelemetrySink;
use serde::Serialize;

use crate::cli::command_supports_release_diff_preview;
use crate::cli_progress::CliProgressReporter;
use crate::cli_progress::CommandStream;
use crate::cli_progress::ProgressFormat;
use crate::maybe_load_prepared_release_execution;
use crate::save_prepared_release_execution;
use crate::*;

pub(crate) fn execute_matches(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command_name: &str,
	cli_command_matches: &ArgMatches,
	quiet: bool,
) -> MonochangeResult<String> {
	let cli_command = if cli_command_name.starts_with("step:") {
		Some(synthetic_step_command_definition(cli_command_name)?)
	} else {
		configuration
			.cli
			.iter()
			.find(|cli_command| cli_command.name == cli_command_name)
			.cloned()
	};
	let Some(cli_command) = cli_command else {
		return Err(MonochangeError::Config(format!(
			"unknown command `{cli_command_name}`"
		)));
	};

	let inputs = collect_cli_command_inputs(&cli_command, cli_command_matches);
	let dry_run = quiet || cli_command_matches.get_flag("dry-run");
	let show_diff =
		command_supports_release_diff_preview(&cli_command) && cli_command_matches.get_flag("diff");
	let progress_format = cli_command_matches
		.get_one::<String>("progress-format")
		.map_or_else(
			|| {
				std::env::var("MONOCHANGE_PROGRESS_FORMAT")
					.ok()
					.map_or(Ok(ProgressFormat::Auto), |value| {
						parse_progress_format(&value)
					})
			},
			|value| parse_progress_format(value),
		)?;
	let prepared_release_path = command_supports_release_diff_preview(&cli_command)
		.then(|| cli_command_matches.get_one::<String>("prepared-release"))
		.flatten()
		.map(PathBuf::from);
	if show_diff {
		execute_cli_command_with_options(
			root,
			configuration,
			&cli_command,
			ExecuteCliCommandOptions {
				dry_run,
				quiet,
				show_diff: true,
				inputs,
				prepared_release_path,
				progress_format,
			},
		)
	} else {
		execute_cli_command_with_options(
			root,
			configuration,
			&cli_command,
			ExecuteCliCommandOptions {
				dry_run,
				quiet,
				show_diff: false,
				inputs,
				prepared_release_path,
				progress_format,
			},
		)
	}
}

fn parse_progress_format(value: &str) -> MonochangeResult<ProgressFormat> {
	ProgressFormat::parse(value).ok_or_else(|| {
		MonochangeError::Config(format!(
			"unknown progress format `{value}`; expected one of: auto, unicode, ascii, json"
		))
	})
}

fn telemetry_progress_format(format: ProgressFormat) -> &'static str {
	match format {
		ProgressFormat::Auto => "auto",
		ProgressFormat::Unicode => "unicode",
		ProgressFormat::Ascii => "ascii",
		ProgressFormat::Json => "json",
	}
}

pub(crate) fn collect_cli_command_inputs(
	cli_command: &CliCommandDefinition,
	matches: &ArgMatches,
) -> BTreeMap<String, Vec<String>> {
	let mut inputs = BTreeMap::new();

	for input in &cli_command.inputs {
		let value_source = matches.value_source(input.name.as_str());
		let values = match input.kind {
			CliInputKind::StringList => {
				matches
					.get_many::<String>(input.name.as_str())
					.map(|values| values.cloned().collect::<Vec<_>>())
					.unwrap_or_default()
			}
			CliInputKind::Boolean => {
				if input.default.as_deref() == Some("true") {
					matches
						.get_one::<String>(input.name.as_str())
						.map(|value| vec![value.clone()])
						.unwrap_or_default()
				} else if matches.get_flag(input.name.as_str()) {
					vec!["true".to_string()]
				} else {
					Vec::new()
				}
			}
			CliInputKind::String | CliInputKind::Path | CliInputKind::Choice => {
				// Special case: `change` command with `bump` default value
				if cli_command.name == "change"
					&& input.name == "bump"
					&& value_source == Some(ValueSource::DefaultValue)
				{
					Vec::new()
				} else {
					matches
						.get_one::<String>(input.name.as_str())
						.map(|value| vec![value.clone()])
						.unwrap_or_default()
				}
			}
		};
		inputs.insert(input.name.clone(), values);
	}

	inputs
}

fn resolve_step_inputs(
	context: &CliContext,
	step: &CliStepDefinition,
) -> MonochangeResult<BTreeMap<String, Vec<String>>> {
	let mut resolved = context.inputs.clone();
	if step.inputs().is_empty() {
		return Ok(resolved);
	}

	let template_context = build_cli_template_context(context, &context.inputs, None);
	for (input_name, input_value) in step.inputs() {
		resolved.insert(
			input_name.clone(),
			resolve_step_input_override(input_value, &template_context)?,
		);
	}

	Ok(resolved)
}

fn resolve_step_input_override(
	input_value: &CliStepInputValue,
	template_context: &serde_json::Map<String, serde_json::Value>,
) -> MonochangeResult<Vec<String>> {
	match input_value {
		CliStepInputValue::Boolean(value) => Ok(vec![value.to_string()]),
		CliStepInputValue::List(values) => {
			let mut resolved = Vec::new();
			for value in values {
				resolved.extend(resolve_step_input_template(value, template_context)?);
			}
			Ok(resolved)
		}
		CliStepInputValue::String(value) => resolve_step_input_template(value, template_context),
	}
}

fn resolve_step_input_template(
	template: &str,
	template_context: &serde_json::Map<String, serde_json::Value>,
) -> MonochangeResult<Vec<String>> {
	if let Some(path) = parse_direct_template_reference(template) {
		return Ok(lookup_template_value(
			&serde_json::Value::Object(template_context.clone()),
			path,
		)
		.map_or_else(Vec::new, template_value_to_input_values));
	}

	let jinja_context =
		minijinja::Value::from_serialize(serde_json::Value::Object(template_context.clone()));
	Ok(vec![render_jinja_template(template, &jinja_context)?])
}

pub(crate) fn parse_direct_template_reference(template: &str) -> Option<&str> {
	let trimmed = template.trim();
	let inner = trimmed.strip_prefix("{{")?.strip_suffix("}}")?.trim();
	if inner.is_empty()
		|| !matches!(
			inner.chars().next(),
			Some(first) if first.is_ascii_alphabetic() || first == '_'
		) || inner
		.split('.')
		.any(|segment| matches!(segment, "true" | "false" | "null" | "none"))
		|| !inner
			.chars()
			.all(|ch| ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
	{
		return None;
	}
	Some(inner)
}

pub(crate) fn lookup_template_value<'a>(
	value: &'a serde_json::Value,
	path: &str,
) -> Option<&'a serde_json::Value> {
	let mut current = value;
	for segment in path.split('.') {
		current = match current {
			serde_json::Value::Object(map) => map.get(segment)?,
			serde_json::Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
			_ => return None,
		};
	}
	Some(current)
}

pub(crate) fn template_value_to_input_values(value: &serde_json::Value) -> Vec<String> {
	match value {
		serde_json::Value::Null => Vec::new(),
		serde_json::Value::Bool(value) => vec![value.to_string()],
		serde_json::Value::Number(value) => vec![value.to_string()],
		serde_json::Value::String(value) => vec![value.clone()],
		serde_json::Value::Array(values) => {
			values
				.iter()
				.flat_map(template_value_to_input_values)
				.collect()
		}
		serde_json::Value::Object(_) => vec![value.to_string()],
	}
}

const DEFAULT_RELEASE_MANIFEST_PATH: &str = ".monochange/release-manifest.json";

pub(crate) fn write_release_manifest_file(
	root: &Path,
	path: &Path,
	manifest: &ReleaseManifest,
) -> MonochangeResult<PathBuf> {
	let resolved_path = resolve_config_path(root, path);
	ensure_monochange_artifact_ignored(root, &resolved_path)?;
	let rendered = render_release_manifest_json(manifest)?;
	let update = FileUpdate {
		path: resolved_path.clone(),
		content: rendered.into_bytes(),
	};
	apply_file_updates(&[update])?;
	Ok(root_relative(root, &resolved_path))
}

fn write_default_release_manifest_file(
	root: &Path,
	manifest: &ReleaseManifest,
) -> MonochangeResult<PathBuf> {
	write_release_manifest_file(root, Path::new(DEFAULT_RELEASE_MANIFEST_PATH), manifest)
}

fn ensure_prepared_release_for_consumer_step(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	context: &mut CliContext,
	prepared_release_path: Option<&Path>,
	dry_run: bool,
	build_file_diffs: bool,
	step_name: &str,
) -> MonochangeResult<()> {
	if context.prepared_release.is_some() {
		return Ok(());
	}
	#[rustfmt::skip]
	let loaded = maybe_load_prepared_release_execution(root, configuration, prepared_release_path, dry_run, build_file_diffs)?;
	let Some(loaded) = loaded else {
		return Err(MonochangeError::Config(format!(
			"`{step_name}` requires a previous `PrepareRelease` step or a reusable prepared release artifact"
		)));
	};
	context.command_logs.push(loaded.message);
	context.prepared_file_diffs = loaded.execution.file_diffs;
	context.prepared_release = Some(loaded.execution.prepared_release);
	Ok(())
}

pub(crate) fn build_release_results(
	dry_run: bool,
	requests: &[SourceReleaseRequest],
	publish: impl FnOnce() -> MonochangeResult<Vec<SourceReleaseOutcome>>,
) -> MonochangeResult<Vec<String>> {
	if dry_run {
		Ok(requests
			.iter()
			.map(|request| {
				format!(
					"dry-run {} {} ({}) via {}",
					request.repository, request.tag_name, request.name, request.provider
				)
			})
			.collect())
	} else {
		Ok(publish()?
			.into_iter()
			.map(|result| {
				format!(
					"{} {} ({}) via {}",
					result.repository,
					result.tag_name,
					format_source_operation(&result.operation),
					result.provider
				)
			})
			.collect())
	}
}

fn build_release_results_for_source(
	dry_run: bool,
	source: &SourceConfiguration,
	requests: &[SourceReleaseRequest],
) -> MonochangeResult<Vec<String>> {
	#[rustfmt::skip]
	let result = build_release_results(dry_run, requests, || publish_source_release_requests(source, requests));
	result
}

pub(crate) fn build_release_request_result(
	dry_run: bool,
	request: &SourceChangeRequest,
	publish: impl FnOnce() -> MonochangeResult<SourceChangeRequestOutcome>,
) -> MonochangeResult<String> {
	if dry_run {
		Ok(format!(
			"dry-run {} {} -> {} via {}",
			request.repository, request.head_branch, request.base_branch, request.provider
		))
	} else {
		let result = publish()?;
		Ok(format!(
			"{} #{} ({}) via {}",
			result.repository,
			result.number,
			format_change_request_operation(&result.operation),
			result.provider
		))
	}
}

fn build_release_request_result_for_source(
	dry_run: bool,
	source: &SourceConfiguration,
	root: &Path,
	request: &SourceChangeRequest,
	tracked_paths: &[PathBuf],
	no_verify: bool,
) -> MonochangeResult<String> {
	#[rustfmt::skip]
	let result = build_release_request_result(dry_run, request, || publish_source_change_request(source, root, request, tracked_paths, no_verify));
	result
}

pub(crate) fn build_issue_comment_results(
	dry_run: bool,
	plans: &[HostedIssueCommentPlan],
	publish: impl FnOnce() -> MonochangeResult<Vec<monochange_core::HostedIssueCommentOutcome>>,
) -> MonochangeResult<Vec<String>> {
	if dry_run {
		Ok(plans
			.iter()
			.map(|plan| format!("dry-run {} {}", plan.repository, plan.issue_id))
			.collect())
	} else {
		Ok(publish()?
			.into_iter()
			.map(|result| {
				format!(
					"{} {} ({})",
					result.repository,
					result.issue_id,
					match result.operation {
						monochange_core::HostedIssueCommentOperation::Created => "created",
						monochange_core::HostedIssueCommentOperation::SkippedExisting => {
							"skipped_existing"
						}
						monochange_core::HostedIssueCommentOperation::Closed => "closed",
					}
				)
			})
			.collect())
	}
}

fn build_issue_comment_results_for_source(
	dry_run: bool,
	source: &SourceConfiguration,
	manifest: &ReleaseManifest,
	plans: &[HostedIssueCommentPlan],
) -> MonochangeResult<Vec<String>> {
	let adapter = hosted_sources::configured_hosted_source_adapter(source);
	#[rustfmt::skip]
	let result = build_issue_comment_results(dry_run, plans, || adapter.comment_released_issues(source, manifest));
	result
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn execute_cli_command(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command: &CliCommandDefinition,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	execute_cli_command_with_options(
		root,
		configuration,
		cli_command,
		ExecuteCliCommandOptions {
			dry_run,
			quiet: false,
			show_diff: false,
			inputs,
			prepared_release_path: None,
			progress_format: ProgressFormat::Auto,
		},
	)
}

pub(crate) struct ExecuteCliCommandOptions {
	dry_run: bool,
	quiet: bool,
	show_diff: bool,
	inputs: BTreeMap<String, Vec<String>>,
	prepared_release_path: Option<PathBuf>,
	progress_format: ProgressFormat,
}

#[tracing::instrument(skip_all, fields(command = cli_command.name))]
pub(crate) fn execute_cli_command_with_options(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command: &CliCommandDefinition,
	options: ExecuteCliCommandOptions,
) -> MonochangeResult<String> {
	let ExecuteCliCommandOptions {
		dry_run,
		quiet,
		show_diff,
		inputs,
		prepared_release_path,
		progress_format,
	} = options;
	let mut context = CliContext {
		root: root.to_path_buf(),
		dry_run,
		quiet,
		show_diff,
		last_step_inputs: inputs.clone(),
		inputs,
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let mut output = None;
	let command_started_at = Instant::now();
	let mut progress = CliProgressReporter::new(cli_command, dry_run, quiet, progress_format);
	let telemetry = CliTelemetry::new(
		TelemetrySink::from_env(),
		cli_command,
		dry_run,
		show_diff,
		progress_format,
		command_started_at,
	);

	for (step_index, step) in cli_command.steps.iter().enumerate() {
		let step_started_at = Instant::now();
		let step_inputs = match resolve_step_inputs(&context, step) {
			Ok(step_inputs) => step_inputs,
			Err(error) => {
				telemetry.capture_step(
					step_index,
					step,
					false,
					step_started_at.elapsed(),
					TelemetryOutcome::Error,
					Some(&error),
				);
				telemetry.capture_command(TelemetryOutcome::Error, Some(&error));
				return Err(error);
			}
		};
		context.last_step_inputs = step_inputs.clone();
		let show_progress = step_shows_progress(step, &step_inputs);

		let should_execute = match should_execute_cli_step(step, &context, &step_inputs) {
			Ok(should_execute) => should_execute,
			Err(error) => {
				telemetry.capture_step(
					step_index,
					step,
					false,
					step_started_at.elapsed(),
					TelemetryOutcome::Error,
					Some(&error),
				);
				telemetry.capture_command(TelemetryOutcome::Error, Some(&error));
				return Err(error);
			}
		};
		if !should_execute {
			record_skipped_cli_step(&mut context, step, step_index, &mut progress, show_progress);
			telemetry.capture_step(
				step_index,
				step,
				true,
				step_started_at.elapsed(),
				TelemetryOutcome::Skipped,
				None,
			);
			continue;
		}

		if show_progress {
			progress.step_started(step_index, step);
		}
		tracing::debug!(step = step.kind_name(), "executing CLI step");
		let mut step_phase_timings = Vec::new();
		let step_result: MonochangeResult<()> = (|| {
			match step {
				CliStepDefinition::Validate { .. } => {
					validate_workspace(root)?;
					validate_cargo_workspace_version_groups(root)?;
					let warnings = validate_versioned_files_content(root)?;
					if !context.quiet {
						for warning in &warnings {
							eprintln!("warning: {warning}");
						}
					}
					let fix = step_inputs
						.get("fix")
						.and_then(|values| values.first())
						.is_some_and(|value| value == "true");
					let (lint_output, _lint_has_errors) = lint::run_lint_step(root, fix)?;
					if !context.quiet && !lint_output.is_empty() {
						eprintln!("{lint_output}");
					}
					output = Some(format!(
						"workspace validation passed for {}",
						root_relative(root, root).display()
					));
					Ok(())
				}
				CliStepDefinition::Discover { .. } => {
					let format = step_inputs
						.get("format")
						.and_then(|values| values.first())
						.map_or(Ok(OutputFormat::Markdown), |value| {
							parse_output_format(value)
						})?;
					output = Some(render_discovery_report(&discover_workspace(root)?, format)?);
					Ok(())
				}
				CliStepDefinition::DisplayVersions { .. } => {
					let prepared_execution = match maybe_load_prepared_release_execution(
						root,
						configuration,
						prepared_release_path.as_deref(),
						true,
						false,
					)? {
						Some(loaded) => loaded.execution,
						None => prepare_release_execution_with_file_diffs(root, true, false)?,
					};
					step_phase_timings.clone_from(&prepared_execution.phase_timings);
					let rendered_output = render_display_versions_output(
						&prepared_execution.prepared_release,
						&step_inputs,
					)?;
					output = Some(rendered_output);
					Ok(())
				}
				CliStepDefinition::CreateChangeFile { .. } => {
					output = Some(execute_create_change_file_step(
						root,
						configuration,
						&step_inputs,
					)?);
					Ok(())
				}
				CliStepDefinition::PrepareRelease { .. } => {
					let build_file_diffs = context.show_diff
						|| steps_reference_release_file_diffs(&cli_command.steps[step_index + 1..]);
					let prepared_execution = if let Some(loaded) =
						maybe_load_prepared_release_execution(
							root,
							configuration,
							prepared_release_path.as_deref(),
							dry_run,
							build_file_diffs,
						)? {
						context.command_logs.push(loaded.message);
						loaded.execution
					} else {
						prepare_release_execution_with_file_diffs(root, dry_run, build_file_diffs)?
					};
					step_phase_timings.clone_from(&prepared_execution.phase_timings);
					context.prepared_file_diffs = prepared_execution.file_diffs;
					context.prepared_release = Some(prepared_execution.prepared_release);
					let prepared_release = context
						.prepared_release
						.as_ref()
						.expect("prepared release must be available after prepare step");
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					context.release_manifest_path =
						Some(write_default_release_manifest_file(root, &manifest)?);
					output = None;
					Ok(())
				}
				CliStepDefinition::PublishRelease { .. } => {
					let manifest = if let Some(prepared_release) = context.prepared_release.as_ref()
					{
						build_release_manifest(cli_command, prepared_release, &context.command_logs)
					} else {
						let from_ref = step_inputs
							.get("from-ref")
							.and_then(|v| v.first().cloned())
							.unwrap_or_else(|| "HEAD".to_string());
						let discovery = discover_release_record(root, &from_ref)?;
						build_release_manifest_from_record(&discovery.record)
					};
					let source = configuration.source.clone().ok_or_else(|| {
						MonochangeError::Config(
							"`PublishRelease` requires `[source]` configuration".to_string(),
						)
					})?;
					context.release_requests = build_source_release_requests(&source, &manifest);
					#[rustfmt::skip]
						let results = build_release_results_for_source(context.dry_run, &source, &context.release_requests)?;
					context.release_results = results;
					output = None;
					Ok(())
				}
				CliStepDefinition::PlaceholderPublish { .. } => {
					let selected_packages = selected_package_ids(&step_inputs);
					#[rustfmt::skip]
					let rate_limit_report = publish_rate_limits::plan_publish_rate_limits(root, configuration, context.prepared_release.as_ref(), &selected_packages, publish_rate_limits::PublishRateLimitMode::Placeholder, context.dry_run)?;
					if !context.dry_run {
						#[rustfmt::skip]
						publish_rate_limits::enforce_publish_rate_limits(configuration, &rate_limit_report, publish_rate_limits::PublishRateLimitMode::Placeholder)?;
					}
					#[rustfmt::skip]
					let report = package_publish::run_placeholder_publish(root, configuration, &selected_packages, context.dry_run)?;
					context.package_publish_report = Some(report);
					context.rate_limit_report = Some(rate_limit_report);
					output = None;
					Ok(())
				}
				CliStepDefinition::PublishPackages { .. } => {
					let selected_packages = selected_package_ids(&step_inputs);
					#[rustfmt::skip]
					let rate_limit_report = publish_rate_limits::plan_publish_rate_limits(root, configuration, context.prepared_release.as_ref(), &selected_packages, publish_rate_limits::PublishRateLimitMode::Publish, context.dry_run)?;
					if !context.dry_run {
						#[rustfmt::skip]
						publish_rate_limits::enforce_publish_rate_limits(configuration, &rate_limit_report, publish_rate_limits::PublishRateLimitMode::Publish)?;
					}
					#[rustfmt::skip]
					let report = package_publish::run_publish_packages(root, configuration, context.prepared_release.as_ref(), &selected_packages, context.dry_run)?;
					context.package_publish_report = Some(report);
					context.rate_limit_report = Some(rate_limit_report);
					output = None;
					Ok(())
				}
				CliStepDefinition::PlanPublishRateLimits { .. } => {
					#[rustfmt::skip]
					let report = publish_rate_limits::plan_publish_rate_limits(root, configuration, context.prepared_release.as_ref(), &selected_package_ids(&step_inputs), publish_rate_limit_mode_from_inputs(&step_inputs)?, context.dry_run)?;
					context.rate_limit_report = Some(report);
					output = None;
					Ok(())
				}
				CliStepDefinition::CommitRelease { no_verify, .. } => {
					ensure_prepared_release_for_consumer_step(
						root,
						configuration,
						&mut context,
						prepared_release_path.as_deref(),
						dry_run,
						false,
						"CommitRelease",
					)?;
					let prepared_release = context
						.prepared_release
						.as_ref()
						.expect("prepared release must be available before committing release");
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					let no_verify =
						parse_boolean_step_input(&step_inputs, "no_verify")?.unwrap_or(*no_verify);
					#[rustfmt::skip]
				let release_commit_report =
					commit_release(root, &context, configuration.source.as_ref(), &manifest, no_verify)?;
					context.release_commit_report = Some(release_commit_report);
					output = None;
					Ok(())
				}
				CliStepDefinition::OpenReleaseRequest { no_verify, .. } => {
					let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
						MonochangeError::Config(
							"`OpenReleaseRequest` requires a previous `PrepareRelease` step"
								.to_string(),
						)
					})?;
					let source = configuration.source.clone().ok_or_else(|| {
						MonochangeError::Config(
							"`OpenReleaseRequest` requires `[source]` configuration".to_string(),
						)
					})?;
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					let request = build_source_change_request(&source, &manifest);
					let tracked_paths = tracked_release_pull_request_paths(&context, &manifest);
					let dry_run = context.dry_run;
					let no_verify =
						parse_boolean_step_input(&step_inputs, "no_verify")?.unwrap_or(*no_verify);
					#[rustfmt::skip]
						let result = build_release_request_result_for_source(
						dry_run,
						&source,
						root,
						&request,
						&tracked_paths,
						no_verify,
					)?;
					context.release_request_result = Some(result);
					context.release_request = Some(request);
					output = None;
					Ok(())
				}
				CliStepDefinition::CommentReleasedIssues { .. } => {
					let manifest = if let Some(prepared_release) = context.prepared_release.as_ref()
					{
						build_release_manifest(cli_command, prepared_release, &context.command_logs)
					} else {
						let from_ref = step_inputs
							.get("from-ref")
							.and_then(|v| v.first().cloned())
							.unwrap_or_else(|| "HEAD".to_string());
						let discovery = discover_release_record(root, &from_ref)?;
						build_release_manifest_from_record(&discovery.record)
					};
					let auto_close_issues =
						parse_boolean_step_input(&step_inputs, "auto-close-issues")?
							.unwrap_or(false);
					let source = configuration.source.clone().ok_or_else(|| {
						MonochangeError::Config(
							"`CommentReleasedIssues` requires `[source]` configuration".to_string(),
						)
					})?;
					let adapter = hosted_sources::configured_hosted_source_adapter(&source);
					if !adapter.features().released_issue_comments {
						return Err(MonochangeError::Config(format!(
							"`CommentReleasedIssues` is not supported for `[source].provider = \"{}\"`",
							source.provider
						)));
					}
					let mut issue_comment_plans =
						adapter.plan_released_issue_comments(&source, &manifest);
					for plan in &mut issue_comment_plans {
						plan.close &= auto_close_issues;
					}
					#[rustfmt::skip]
					let dry_run = context.dry_run;
					#[rustfmt::skip]
					let results = build_issue_comment_results_for_source(dry_run, &source, &manifest, &issue_comment_plans)?;
					context.issue_comment_plans = issue_comment_plans;
					context.issue_comment_results = results;
					output = None;
					Ok(())
				}
				CliStepDefinition::AffectedPackages { .. } => {
					let evaluation =
						execute_affected_packages_step(root, &step_inputs, context.quiet)?;
					context.changeset_policy_evaluation = Some(evaluation);
					output = None;
					Ok(())
				}
				CliStepDefinition::DiagnoseChangesets { .. } => {
					let requested = step_inputs.get("changeset").cloned().unwrap_or_default();
					let report = diagnose_changesets(root, &requested)?;
					context.changeset_diagnostics = Some(report);
					output = None;
					Ok(())
				}
				CliStepDefinition::RetargetRelease { .. } => {
					let from = step_inputs
						.get("from")
						.and_then(|values| values.first())
						.cloned()
						.ok_or_else(|| {
							MonochangeError::Config(
								"`RetargetRelease` requires a `from` input".to_string(),
							)
						})?;
					let target = step_inputs
						.get("target")
						.and_then(|values| values.first())
						.cloned()
						.unwrap_or_else(|| "HEAD".to_string());
					let force = parse_boolean_step_input(&step_inputs, "force")?.unwrap_or(false);
					let sync_provider =
						parse_boolean_step_input(&step_inputs, "sync_provider")?.unwrap_or(true);
					let discovery = discover_release_record(root, &from)?;
					let source = inferred_retarget_source_configuration(
						configuration.source.as_ref(),
						&discovery,
						sync_provider,
					);
					let plan = plan_release_retarget(
						root,
						&discovery,
						&target,
						force,
						sync_provider,
						context.dry_run,
						source.as_ref(),
					)?;
					let result = execute_release_retarget(root, source.as_ref(), &plan)?;
					context.retarget_report = Some(build_retarget_release_report(
						&from,
						&target,
						&discovery,
						plan.is_descendant,
						&result,
					));
					output = None;
					Ok(())
				}
				CliStepDefinition::Command {
					command,
					dry_run_command,
					shell,
					id,
					variables,
					..
				} => {
					run_cli_command_command(
						&mut context,
						step,
						step_index,
						&mut progress,
						show_progress,
						CommandStepOptions {
							command,
							dry_run_command: dry_run_command.as_deref(),
							shell,
							step_id: id.as_deref(),
							variables: variables.as_ref(),
							step_inputs: &step_inputs,
						},
					)?;
					Ok(())
				}
				_ => {
					Err(MonochangeError::Config(
						"unsupported CLI step definition".to_string(),
					))
				}
			}
		})();
		if let Err(error) = step_result {
			let elapsed = step_started_at.elapsed();
			report_cli_step_failure(
				&mut progress,
				show_progress,
				step_index,
				step,
				elapsed,
				&error,
			);
			telemetry.capture_step(
				step_index,
				step,
				false,
				elapsed,
				TelemetryOutcome::Error,
				Some(&error),
			);
			telemetry.capture_command(TelemetryOutcome::Error, Some(&error));

			return Err(error);
		}
		let elapsed = step_started_at.elapsed();
		if show_progress {
			progress.step_finished(step_index, step, elapsed, &step_phase_timings);
		}
		telemetry.capture_step(
			step_index,
			step,
			false,
			elapsed,
			TelemetryOutcome::Success,
			None,
		);
	}

	progress.command_finished(command_started_at.elapsed());

	let artifact_path = prepared_release_path.as_deref();
	let result = save_prepared_release_artifact(root, configuration, &context, artifact_path)
		.and_then(|()| resolve_command_output(cli_command, &context, dry_run, output));
	match &result {
		Ok(_) => telemetry.capture_command(TelemetryOutcome::Success, None),
		Err(error) => telemetry.capture_command(TelemetryOutcome::Error, Some(error)),
	}

	result
}

struct CliTelemetry<'a> {
	sink: TelemetrySink,
	cli_command: &'a CliCommandDefinition,
	dry_run: bool,
	show_diff: bool,
	progress_format: ProgressFormat,
	started_at: Instant,
}

impl<'a> CliTelemetry<'a> {
	fn new(
		sink: TelemetrySink,
		cli_command: &'a CliCommandDefinition,
		dry_run: bool,
		show_diff: bool,
		progress_format: ProgressFormat,
		started_at: Instant,
	) -> Self {
		Self {
			sink,
			cli_command,
			dry_run,
			show_diff,
			progress_format,
			started_at,
		}
	}

	fn capture_command(&self, outcome: TelemetryOutcome, error: Option<&MonochangeError>) {
		self.sink.capture_command(CommandTelemetry {
			command_name: &self.cli_command.name,
			dry_run: self.dry_run,
			show_diff: self.show_diff,
			progress_format: telemetry_progress_format(self.progress_format),
			step_count: self.cli_command.steps.len(),
			duration: self.started_at.elapsed(),
			outcome,
			error,
		});
	}

	fn capture_step(
		&self,
		step_index: usize,
		step: &CliStepDefinition,
		skipped: bool,
		duration: Duration,
		outcome: TelemetryOutcome,
		error: Option<&MonochangeError>,
	) {
		self.sink.capture_step(StepTelemetry {
			command_name: &self.cli_command.name,
			step_index,
			step_kind: step.kind_name(),
			skipped,
			duration,
			outcome,
			error,
		});
	}
}

pub(crate) fn should_execute_cli_step(
	step: &CliStepDefinition,
	context: &CliContext,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<bool> {
	let Some(condition) = step.when() else {
		return Ok(true);
	};
	evaluate_cli_step_condition(condition, context, step_inputs)
}

fn steps_reference_release_file_diffs(steps: &[CliStepDefinition]) -> bool {
	steps.iter().any(step_references_release_file_diffs)
}

fn step_references_release_file_diffs(step: &CliStepDefinition) -> bool {
	let mentions_file_diffs = |value: &str| value.contains("file_diffs");
	let inputs_mention_file_diffs = step.inputs().values().any(|value| {
		match value {
			CliStepInputValue::String(value) => mentions_file_diffs(value),
			CliStepInputValue::Boolean(_) => false,
			CliStepInputValue::List(values) => {
				values.iter().any(|value| mentions_file_diffs(value))
			}
		}
	});
	if step.when().is_some_and(mentions_file_diffs) || inputs_mention_file_diffs {
		return true;
	}
	match step {
		CliStepDefinition::Command {
			command,
			dry_run_command,
			variables,
			..
		} => {
			mentions_file_diffs(command)
				|| dry_run_command.as_deref().is_some_and(mentions_file_diffs)
				|| variables.as_ref().is_some_and(|variables| {
					variables.keys().any(|value| mentions_file_diffs(value))
				})
		}
		_ => false,
	}
}

fn evaluate_cli_step_condition(
	condition: &str,
	context: &CliContext,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<bool> {
	let trimmed = condition.trim();
	if trimmed.is_empty() {
		return Ok(false);
	}
	let template_context = build_cli_template_context(context, step_inputs, None);
	let template_context_json = serde_json::Value::Object(template_context.clone());
	if let Some(path) = parse_direct_template_reference(trimmed) {
		let Some(value) = lookup_template_value(&template_context_json, path) else {
			return Err(MonochangeError::Config(format!(
				"failed to evaluate `when` condition `{condition}`: unknown template path `{path}`"
			)));
		};
		return parse_template_as_boolean(value, condition);
	}
	let normalized = normalize_when_expression(trimmed);
	let jinja_context =
		minijinja::Value::from_serialize(serde_json::Value::Object(template_context));
	let rendered = render_jinja_template(&normalized, &jinja_context)?;
	parse_string_as_boolean(&rendered, condition)
}

fn parse_template_as_boolean(value: &serde_json::Value, condition: &str) -> MonochangeResult<bool> {
	match value {
		serde_json::Value::Bool(value) => Ok(*value),
		serde_json::Value::Number(value) => parse_string_as_boolean(&value.to_string(), condition),
		serde_json::Value::String(value) => parse_string_as_boolean(value, condition),
		serde_json::Value::Null => Ok(false),
		serde_json::Value::Array(values) => {
			if values.len() == 1 {
				parse_template_as_boolean(&values[0], condition)
			} else {
				Err(MonochangeError::Config(format!(
					"`when` condition `{condition}` is not a scalar boolean value"
				)))
			}
		}
		serde_json::Value::Object(_) => {
			Err(MonochangeError::Config(format!(
				"`when` condition `{condition}` is not a scalar boolean value"
			)))
		}
	}
}

pub(crate) fn normalize_when_expression(condition: &str) -> String {
	let expression = condition.replace("&&", " and ").replace("||", " or ");
	let mut normalized = String::with_capacity(expression.len());
	let mut chars = expression.chars().peekable();
	while let Some(ch) = chars.next() {
		if ch == '!' {
			if let Some('=') = chars.peek() {
				normalized.push('!');
				continue;
			}
			let previous_was_expression_boundary = normalized.chars().last().is_none_or(|prev| {
				prev.is_whitespace() || prev == '(' || prev == ',' || prev == '>' || prev == '<'
			});
			if previous_was_expression_boundary {
				normalized.push_str("not ");
			} else {
				normalized.push('!');
			}
			continue;
		}
		normalized.push(ch);
	}
	normalized
}

fn parse_string_as_boolean(value: &str, condition: &str) -> MonochangeResult<bool> {
	let value = value.trim().to_ascii_lowercase();
	if let Ok(number) = value.parse::<i64>() {
		return Ok(number != 0);
	}
	match value.as_str() {
		"true" => Ok(true),
		"false" | "0" | "" => Ok(false),
		other => {
			Err(MonochangeError::Config(format!(
				"`when` condition `{condition}` must be a boolean, got `{other}`"
			)))
		}
	}
}

#[derive(Clone, Copy)]
struct CommandStepOptions<'a> {
	command: &'a str,
	dry_run_command: Option<&'a str>,
	shell: &'a ShellConfig,
	step_id: Option<&'a str>,
	variables: Option<&'a BTreeMap<String, CommandVariable>>,
	step_inputs: &'a BTreeMap<String, Vec<String>>,
}

fn step_shows_progress(
	step: &CliStepDefinition,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> bool {
	if matches!(step, CliStepDefinition::CreateChangeFile { .. })
		&& step_inputs
			.get("interactive")
			.and_then(|values| values.first())
			.is_some_and(|value| value == "true")
	{
		return false;
	}
	step.show_progress().unwrap_or(true)
}

fn run_cli_command_command(
	context: &mut CliContext,
	step: &CliStepDefinition,
	step_index: usize,
	progress: &mut CliProgressReporter,
	show_progress: bool,
	options: CommandStepOptions<'_>,
) -> MonochangeResult<()> {
	let Some(command_to_run) = resolve_command_step_command(context, &options) else {
		return Ok(());
	};
	let interpolated = interpolate_cli_command_command(
		context,
		command_to_run,
		options.variables,
		options.step_inputs,
	);
	let mut process_command = build_process_command(&context.root, options.shell, &interpolated)?;
	let output = execute_process_command(
		&mut process_command,
		progress,
		show_progress,
		step_index,
		step,
		&interpolated,
	)?;

	ensure_command_succeeded(&output, &interpolated)?;
	store_command_step_output(context, options.step_id, &output);
	log_command_step_output(context, &interpolated, &output);

	Ok(())
}

fn record_skipped_cli_step(
	context: &mut CliContext,
	step: &CliStepDefinition,
	step_index: usize,
	progress: &mut CliProgressReporter,
	show_progress: bool,
) {
	if show_progress {
		progress.step_skipped(step_index, step, step.when());
	}

	let Some(condition) = step.when() else {
		return;
	};

	tracing::debug!(step = step.kind_name(), condition = %condition, "skipped CLI step");
	context.command_logs.push(format!(
		"skipped step `{}` because when condition `{condition}` is false",
		step.display_name()
	));
}

fn resolve_command_step_command<'a>(
	context: &mut CliContext,
	options: &CommandStepOptions<'a>,
) -> Option<&'a str> {
	if !context.dry_run {
		return Some(options.command);
	}

	if let Some(command) = options.dry_run_command {
		return Some(command);
	}

	let skipped = interpolate_cli_command_command(
		context,
		options.command,
		options.variables,
		options.step_inputs,
	);
	context
		.command_logs
		.push(format!("skipped command `{skipped}` (dry-run)"));

	None
}

fn build_process_command(
	root: &Path,
	shell: &ShellConfig,
	interpolated: &str,
) -> MonochangeResult<ProcessCommand> {
	let mut process_command = if let Some(shell_binary) = shell.shell_binary() {
		let mut process_command = ProcessCommand::new(shell_binary);
		process_command.arg("-c").arg(interpolated);
		process_command
	} else {
		let parts = shlex::split(interpolated).ok_or_else(|| {
			MonochangeError::Config(format!("failed to parse command `{interpolated}`"))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(
				"command must not be empty".to_string(),
			));
		};
		let mut process_command = ProcessCommand::new(program);
		process_command.args(args);
		process_command
	};
	process_command.current_dir(root);

	Ok(process_command)
}

fn execute_process_command(
	process_command: &mut ProcessCommand,
	progress: &mut CliProgressReporter,
	show_progress: bool,
	step_index: usize,
	step: &CliStepDefinition,
	interpolated: &str,
) -> MonochangeResult<PreparedProcessOutput> {
	if progress.is_enabled() && show_progress {
		return run_process_with_streaming(
			process_command,
			progress,
			step_index,
			step,
			interpolated,
		);
	}

	let output = process_command.output().map_err(|error| {
		MonochangeError::Io(format!("failed to run command `{interpolated}`: {error}"))
	})?;

	Ok(PreparedProcessOutput {
		status: output.status,
		stdout: output.stdout,
		stderr: output.stderr,
	})
}

fn ensure_command_succeeded(
	output: &PreparedProcessOutput,
	interpolated: &str,
) -> MonochangeResult<()> {
	if output.status.success() {
		return Ok(());
	}

	let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
	let details = if stderr.is_empty() {
		format!("exit status {}", output.status)
	} else {
		stderr
	};
	let rendered_command = render_command_for_error(interpolated);

	Err(MonochangeError::Discovery(format!(
		"command `{rendered_command}` failed: {details}"
	)))
}

fn store_command_step_output(
	context: &mut CliContext,
	step_id: Option<&str>,
	output: &PreparedProcessOutput,
) {
	let Some(id) = step_id else {
		return;
	};

	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
	context
		.step_outputs
		.insert(id.to_string(), CommandStepOutput { stdout, stderr });
}

fn log_command_step_output(
	context: &mut CliContext,
	interpolated: &str,
	output: &PreparedProcessOutput,
) {
	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();

	if stdout.is_empty() {
		context.command_logs.push(format!("ran `{interpolated}`"));
		return;
	}

	context.command_logs.push(stdout);
}

struct PreparedProcessOutput {
	status: ExitStatus,
	stdout: Vec<u8>,
	stderr: Vec<u8>,
}

enum StreamEvent {
	Chunk(CommandStream, Vec<u8>),
	Closed(CommandStream),
}

fn run_process_with_streaming(
	process_command: &mut ProcessCommand,
	progress: &mut CliProgressReporter,
	step_index: usize,
	step: &CliStepDefinition,
	interpolated: &str,
) -> MonochangeResult<PreparedProcessOutput> {
	process_command
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let mut child = map_process_spawn_result(process_command.spawn(), interpolated)?;
	let stdout = take_process_stream(child.stdout.take(), "stdout", interpolated)?;
	let stderr = take_process_stream(child.stderr.take(), "stderr", interpolated)?;
	let (sender, receiver) = mpsc::channel();
	let stdout_handle = spawn_stream_reader(stdout, CommandStream::Stdout, sender.clone());
	let stderr_handle = spawn_stream_reader(stderr, CommandStream::Stderr, sender);
	let (stdout_buffer, stderr_buffer) = drain_stream_events(&receiver, progress, step_index, step);
	let status = map_process_wait_result(child.wait(), interpolated)?;
	let _ = stdout_handle.join();
	let _ = stderr_handle.join();
	Ok(PreparedProcessOutput {
		status,
		stdout: stdout_buffer,
		stderr: stderr_buffer,
	})
}

fn map_process_spawn_result(
	result: std::io::Result<std::process::Child>,
	interpolated: &str,
) -> MonochangeResult<std::process::Child> {
	result.map_err(|error| {
		MonochangeError::Io(format!("failed to run command `{interpolated}`: {error}"))
	})
}

fn take_process_stream<T>(
	stream: Option<T>,
	stream_name: &str,
	interpolated: &str,
) -> MonochangeResult<T> {
	stream.ok_or_else(|| {
		MonochangeError::Io(format!(
			"failed to capture {stream_name} for command `{interpolated}`"
		))
	})
}

fn drain_stream_events(
	receiver: &mpsc::Receiver<StreamEvent>,
	progress: &mut CliProgressReporter,
	step_index: usize,
	step: &CliStepDefinition,
) -> (Vec<u8>, Vec<u8>) {
	let mut stdout_buffer = Vec::new();
	let mut stderr_buffer = Vec::new();
	let mut stdout_closed = false;
	let mut stderr_closed = false;
	while !stdout_closed || !stderr_closed {
		match receiver.recv() {
			Ok(StreamEvent::Chunk(stream, chunk)) => {
				match stream {
					CommandStream::Stdout => stdout_buffer.extend_from_slice(&chunk),
					CommandStream::Stderr => stderr_buffer.extend_from_slice(&chunk),
				}
				progress.log_command_output(
					step_index,
					step,
					stream,
					String::from_utf8_lossy(&chunk).as_ref(),
				);
			}
			Ok(StreamEvent::Closed(stream)) => {
				match stream {
					CommandStream::Stdout => stdout_closed = true,
					CommandStream::Stderr => stderr_closed = true,
				}
			}
			Err(_) => break,
		}
	}
	(stdout_buffer, stderr_buffer)
}

fn map_process_wait_result(
	result: std::io::Result<ExitStatus>,
	interpolated: &str,
) -> MonochangeResult<ExitStatus> {
	result.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to wait for command `{interpolated}`: {error}"
		))
	})
}

fn spawn_stream_reader(
	reader: impl Read + Send + 'static,
	stream: CommandStream,
	sender: mpsc::Sender<StreamEvent>,
) -> JoinHandle<()> {
	thread::spawn(move || {
		let mut reader = BufReader::new(reader);
		loop {
			let mut buffer = Vec::new();
			match reader.read_until(b'\n', &mut buffer) {
				Ok(0) | Err(_) => break,
				Ok(_) => {
					let _ = sender.send(StreamEvent::Chunk(stream, buffer));
				}
			}
		}
		let _ = sender.send(StreamEvent::Closed(stream));
	})
}

fn progress_error_detail(error: &MonochangeError) -> &str {
	match error {
		MonochangeError::Io(message)
		| MonochangeError::Config(message)
		| MonochangeError::Discovery(message)
		| MonochangeError::Diagnostic(message) => message,
		_ => "",
	}
}

fn render_command_for_error(command: &str) -> String {
	command
		.replace('\r', "\\r")
		.replace('\n', "\\n")
		.replace('\t', "\\t")
}

fn cli_inputs_template_value(
	inputs: &BTreeMap<String, Vec<String>>,
) -> serde_json::Map<String, serde_json::Value> {
	inputs
		.iter()
		.map(|(input_name, input_values)| {
			(input_name.clone(), cli_input_template_value(input_values))
		})
		.collect()
}

fn cli_input_template_value(input_values: &[String]) -> serde_json::Value {
	if input_values.len() == 1 {
		let value = input_values.first().map_or("", String::as_str);
		if value == "true" || value == "false" {
			return serde_json::Value::Bool(value == "true");
		}
		return serde_json::Value::String(value.to_string());
	}
	if input_values.is_empty() {
		return serde_json::Value::Bool(false);
	}
	serde_json::Value::Array(
		input_values
			.iter()
			.cloned()
			.map(serde_json::Value::String)
			.collect(),
	)
}

pub(crate) fn build_cli_template_context(
	context: &CliContext,
	inputs: &BTreeMap<String, Vec<String>>,
	variables: Option<&BTreeMap<String, CommandVariable>>,
) -> serde_json::Map<String, serde_json::Value> {
	let mut template_context = serde_json::Map::new();

	// Core release variables
	template_context.insert(
		"version".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::Version,
		)),
	);
	template_context.insert(
		"group_version".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::GroupVersion,
		)),
	);
	template_context.insert(
		"released_packages".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::ReleasedPackages,
		)),
	);
	template_context.insert(
		"changed_files".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::ChangedFiles,
		)),
	);
	template_context.insert(
		"changesets".to_string(),
		serde_json::Value::String(cli_command_variable_value(
			context,
			CommandVariable::Changesets,
		)),
	);

	// Released packages list (structured)
	if let Some(prepared) = &context.prepared_release {
		template_context.insert(
			"released_packages_list".to_string(),
			serde_json::Value::Array(
				prepared
					.released_packages
					.iter()
					.cloned()
					.map(serde_json::Value::String)
					.collect(),
			),
		);
	}

	// Structured release.* namespace
	template_context.insert("release".to_string(), build_release_template_value(context));

	// Structured manifest.* namespace
	if let Some(path) = &context.release_manifest_path {
		let mut manifest_map = serde_json::Map::new();
		manifest_map.insert(
			"path".to_string(),
			serde_json::Value::String(path.display().to_string()),
		);
		template_context.insert(
			"manifest".to_string(),
			serde_json::Value::Object(manifest_map),
		);
	}

	// Structured affected.* namespace
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		let mut affected_map = serde_json::Map::new();
		affected_map.insert(
			"status".to_string(),
			serde_json::Value::String(evaluation.status.to_string()),
		);
		affected_map.insert(
			"summary".to_string(),
			serde_json::Value::String(evaluation.summary.clone()),
		);
		template_context.insert(
			"affected".to_string(),
			serde_json::Value::Object(affected_map),
		);
	}

	// Structured retarget.* namespace
	if let Some(report) = &context.retarget_report {
		template_context.insert(
			"retarget".to_string(),
			build_retarget_template_value(report),
		);
	}

	// Structured release_commit.* namespace
	if let Some(report) = &context.release_commit_report {
		template_context.insert(
			"release_commit".to_string(),
			build_release_commit_template_value(report),
		);
	}

	// Structured publish.* namespace
	if let Some(report) = &context.package_publish_report {
		template_context.insert(
			"publish".to_string(),
			build_package_publish_template_value(report, context.rate_limit_report.as_ref()),
		);
	} else if let Some(report) = &context.rate_limit_report {
		template_context.insert(
			"publish_rate_limits".to_string(),
			serde_json::to_value(report).unwrap_or(serde_json::Value::Null),
		);
	}

	// Structured steps.<id>.* namespace from command step outputs
	if !context.step_outputs.is_empty() {
		let mut steps_map = serde_json::Map::new();
		for (id, output) in &context.step_outputs {
			let mut output_map = serde_json::Map::new();
			output_map.insert(
				"stdout".to_string(),
				serde_json::Value::String(output.stdout.clone()),
			);
			output_map.insert(
				"stderr".to_string(),
				serde_json::Value::String(output.stderr.clone()),
			);
			steps_map.insert(id.clone(), serde_json::Value::Object(output_map));
		}
		template_context.insert("steps".to_string(), serde_json::Value::Object(steps_map));
	}

	// User-provided inputs
	let input_context = cli_inputs_template_value(inputs);
	template_context.insert(
		"inputs".to_string(),
		serde_json::Value::Object(input_context),
	);

	// Custom template variables
	if let Some(variables) = variables {
		for (needle, variable) in variables {
			template_context.insert(
				needle.clone(),
				serde_json::Value::String(cli_command_variable_value(context, *variable)),
			);
		}
	}

	template_context
}

fn build_release_template_value(context: &CliContext) -> serde_json::Value {
	let Some(prepared) = &context.prepared_release else {
		return serde_json::Value::Null;
	};

	let mut release_map = serde_json::Map::new();
	release_map.insert(
		"version".to_string(),
		prepared
			.version
			.as_deref()
			.map_or(serde_json::Value::Null, |v| {
				serde_json::Value::String(v.to_string())
			}),
	);
	release_map.insert(
		"group_version".to_string(),
		prepared
			.group_version
			.as_deref()
			.map_or(serde_json::Value::Null, |v| {
				serde_json::Value::String(v.to_string())
			}),
	);
	release_map.insert(
		"dry_run".to_string(),
		serde_json::Value::Bool(prepared.dry_run),
	);
	release_map.insert(
		"released_packages".to_string(),
		serde_json::Value::Array(
			prepared
				.released_packages
				.iter()
				.cloned()
				.map(serde_json::Value::String)
				.collect(),
		),
	);
	release_map.insert(
		"changed_files".to_string(),
		serde_json::Value::Array(
			prepared
				.changed_files
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	release_map.insert(
		"updated_changelogs".to_string(),
		serde_json::Value::Array(
			prepared
				.updated_changelogs
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	release_map.insert(
		"deleted_changesets".to_string(),
		serde_json::Value::Array(
			prepared
				.deleted_changesets
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	release_map.insert(
		"changeset_paths".to_string(),
		serde_json::Value::Array(
			prepared
				.changeset_paths
				.iter()
				.map(|p| serde_json::Value::String(p.display().to_string()))
				.collect(),
		),
	);
	let file_diffs = context
		.prepared_file_diffs
		.iter()
		.map(|file_diff| {
			serde_json::json!({
				"path": file_diff.path,
				"diff": file_diff.diff,
			})
		})
		.collect();
	release_map.insert(
		"file_diffs".to_string(),
		serde_json::Value::Array(file_diffs),
	);

	let targets: Vec<serde_json::Value> = prepared
		.release_targets
		.iter()
		.map(|target| {
			let mut target_map = serde_json::Map::new();
			target_map.insert(
				"id".to_string(),
				serde_json::Value::String(target.id.clone()),
			);
			target_map.insert(
				"version".to_string(),
				serde_json::Value::String(target.tag_name.clone()),
			);
			target_map.insert(
				"kind".to_string(),
				serde_json::Value::String(target.kind.to_string()),
			);
			target_map.insert("tag".to_string(), serde_json::Value::Bool(target.tag));
			serde_json::Value::Object(target_map)
		})
		.collect();
	release_map.insert("targets".to_string(), serde_json::Value::Array(targets));

	serde_json::Value::Object(release_map)
}

fn build_retarget_template_value(report: &RetargetReleaseReport) -> serde_json::Value {
	serde_json::to_value(report).unwrap_or(serde_json::Value::Null)
}

fn build_release_commit_template_value(report: &CommitReleaseReport) -> serde_json::Value {
	serde_json::to_value(report).unwrap_or(serde_json::Value::Null)
}

fn build_package_publish_template_value(
	report: &package_publish::PackagePublishReport,
	rate_limit_report: Option<&monochange_core::PublishRateLimitReport>,
) -> serde_json::Value {
	let mut value = serde_json::to_value(report).unwrap_or(serde_json::Value::Null);
	if let Some(rate_limit_report) = rate_limit_report
		&& let Some(object) = value.as_object_mut()
	{
		object.insert(
			"rateLimits".to_string(),
			serde_json::to_value(rate_limit_report).unwrap_or(serde_json::Value::Null),
		);
	}
	value
}

fn render_json_output<T>(value: &T, context: &str) -> MonochangeResult<String>
where
	T: Serialize,
{
	serde_json::to_string_pretty(value).map_err(|error| {
		MonochangeError::Config(format!("failed to render {context} as json: {error}"))
	})
}

fn render_publish_command_json(
	package_publish: Option<&package_publish::PackagePublishReport>,
	rate_limit_report: Option<&monochange_core::PublishRateLimitReport>,
) -> MonochangeResult<String> {
	render_json_output(
		&serde_json::json!({
			"packagePublish": package_publish,
			"publishRateLimits": rate_limit_report,
		}),
		"combined publish output",
	)
}

fn requested_ci_renderer(inputs: &BTreeMap<String, Vec<String>>) -> MonochangeResult<Option<&str>> {
	match inputs
		.get("ci")
		.and_then(|values| values.first())
		.map(String::as_str)
	{
		None => Ok(None),
		Some("github-actions") => Ok(Some("github-actions")),
		Some("gitlab-ci") => Ok(Some("gitlab-ci")),
		Some(other) => {
			Err(MonochangeError::Config(format!(
				"unsupported publish CI renderer `{other}`"
			)))
		}
	}
}

fn render_publish_rate_limit_ci_snippet(
	report: &monochange_core::PublishRateLimitReport,
	renderer: &str,
) -> MonochangeResult<String> {
	match renderer {
		"github-actions" => Ok(render_github_actions_publish_batches(report)),
		"gitlab-ci" => Ok(render_gitlab_ci_publish_batches(report)),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported publish CI renderer `{other}`"
			)))
		}
	}
}

fn render_github_actions_publish_batches(
	report: &monochange_core::PublishRateLimitReport,
) -> String {
	let mut lines = vec![
		"jobs:".to_string(),
		"  publish_batches:".to_string(),
		"    runs-on: ubuntu-latest".to_string(),
		"    strategy:".to_string(),
		"      fail-fast: false".to_string(),
		"      matrix:".to_string(),
		"        include:".to_string(),
	];

	for batch in &report.batches {
		lines.push(format!("          - registry: {}", batch.registry));
		lines.push(format!("            batch: {}", batch.batch_index));
		lines.push(format!(
			"            total_batches: {}",
			batch.total_batches
		));
		let packages = batch
			.packages
			.iter()
			.map(|package| format!("--package {package}"))
			.collect::<Vec<_>>()
			.join(" ");
		lines.push(format!("            packages: \"{packages}\""));
		if let Some(wait_seconds) = batch.recommended_wait_seconds {
			lines.push(format!("            wait_seconds: {wait_seconds}"));
		} else {
			lines.push("            wait_seconds: 0".to_string());
		}
	}

	lines.extend([
		"    steps:".to_string(),
		"      - name: publish planned batch".to_string(),
		"        run: |".to_string(),
		"          # For batches after the first, trigger a later workflow run instead of sleeping in CI.".to_string(),
		"          mc publish ${{ matrix.packages }} --format json".to_string(),
	]);
	lines.join("\n")
}

fn render_gitlab_ci_publish_batches(report: &monochange_core::PublishRateLimitReport) -> String {
	let mut lines = vec![
		"publish_batches:".to_string(),
		"  stage: publish".to_string(),
		"  parallel:".to_string(),
		"    matrix:".to_string(),
	];
	for batch in &report.batches {
		let packages = batch
			.packages
			.iter()
			.map(|package| format!("--package {package}"))
			.collect::<Vec<_>>()
			.join(" ");
		lines.push("      -".to_string());
		lines.push(format!("        REGISTRY: \"{}\"", batch.registry));
		lines.push(format!("        BATCH: \"{}\"", batch.batch_index));
		lines.push(format!(
			"        TOTAL_BATCHES: \"{}\"",
			batch.total_batches
		));
		lines.push(format!("        PACKAGES: \"{packages}\""));
		lines.push(format!(
			"        WAIT_SECONDS: \"{}\"",
			batch.recommended_wait_seconds.unwrap_or_default()
		));
	}
	lines.extend([
		"  script:".to_string(),
		"    - '# For batches after the first, run a later pipeline instead of sleeping inside CI.'".to_string(),
		"    - mc publish $PACKAGES --format json".to_string(),
	]);
	lines.join("\n")
}

fn selected_package_ids(inputs: &BTreeMap<String, Vec<String>>) -> BTreeSet<String> {
	inputs
		.get("package")
		.into_iter()
		.flatten()
		.map(ToString::to_string)
		.collect()
}

fn publish_rate_limit_mode_from_inputs(
	inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<publish_rate_limits::PublishRateLimitMode> {
	match inputs
		.get("mode")
		.and_then(|values| values.first())
		.map_or("publish", String::as_str)
	{
		"publish" => Ok(publish_rate_limits::PublishRateLimitMode::Publish),
		"placeholder" => Ok(publish_rate_limits::PublishRateLimitMode::Placeholder),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported publish plan mode `{other}`"
			)))
		}
	}
}

pub(crate) fn parse_boolean_step_input(
	inputs: &BTreeMap<String, Vec<String>>,
	name: &str,
) -> MonochangeResult<Option<bool>> {
	inputs
		.get(name)
		.and_then(|values| values.first())
		.map(|value| {
			match value.as_str() {
				"true" => Ok(true),
				"false" => Ok(false),
				other => {
					Err(MonochangeError::Config(format!(
						"invalid boolean value `{other}` for `{name}`"
					)))
				}
			}
		})
		.transpose()
}

pub(crate) fn inferred_retarget_source_configuration(
	configured_source: Option<&SourceConfiguration>,
	discovery: &ReleaseRecordDiscovery,
	sync_provider: bool,
) -> Option<SourceConfiguration> {
	if let Some(source) = configured_source {
		return Some(source.clone());
	}
	if !sync_provider {
		return None;
	}
	let provider = discovery.record.provider.as_ref()?;
	Some(SourceConfiguration {
		provider: provider.kind,
		owner: provider.owner.clone(),
		repo: provider.repo.clone(),
		host: provider.host.clone(),
		api_url: None,
		releases: monochange_core::ProviderReleaseSettings::default(),
		pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
		bot: monochange_core::ProviderBotSettings::default(),
	})
}

pub(crate) fn build_retarget_release_report(
	from: &str,
	target: &str,
	discovery: &ReleaseRecordDiscovery,
	is_descendant: bool,
	result: &RetargetResult,
) -> RetargetReleaseReport {
	RetargetReleaseReport {
		from: from.to_string(),
		target: target.to_string(),
		resolved_from_commit: discovery.resolved_commit.clone(),
		record_commit: result.record_commit.clone(),
		target_commit: result.target_commit.clone(),
		distance: discovery.distance,
		is_descendant,
		force: result.force,
		dry_run: result.dry_run,
		sync_provider: result.sync_provider,
		tags: result
			.git_tag_results
			.iter()
			.map(|tag_result| tag_result.tag_name.clone())
			.collect(),
		git_tag_results: result.git_tag_results.clone(),
		provider_results: result.provider_results.clone(),
		status: if result.dry_run {
			"dry_run".to_string()
		} else {
			"completed".to_string()
		},
	}
}

fn render_release_commit_report(report: &CommitReleaseReport) -> Vec<String> {
	let mut lines = vec!["release commit:".to_string()];
	lines.push(format!("  subject: {}", report.subject));
	lines.extend(
		report
			.commit
			.as_ref()
			.map(|commit| format!("  commit: {}", short_commit_sha(commit))),
	);
	lines.extend((!report.tracked_paths.is_empty()).then_some("  tracked paths:".to_string()));
	#[rustfmt::skip]
	lines.extend(report.tracked_paths.iter().map(|path| format!("    - {}", path.display())));
	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines
}

fn render_package_publish_report(report: &package_publish::PackagePublishReport) -> Vec<String> {
	let mut lines = vec![match report.mode {
		package_publish::PackagePublishRunMode::Placeholder => {
			"placeholder publishing:".to_string()
		}
		package_publish::PackagePublishRunMode::Release => "package publishing:".to_string(),
	}];

	if report.packages.is_empty() {
		lines.push("- no packages matched the publishing criteria".to_string());
		return lines;
	}

	for package in &report.packages {
		lines.push(format!(
			"- {} {} via {} -> {}",
			package.package,
			package.version,
			package.registry,
			package_publish_status_label(package.status),
		));
		lines.push(format!("  ecosystem: {}", package.ecosystem));
		lines.push(format!("  placeholder: {}", yes_no(package.placeholder)));
		lines.push(format!("  publish: {}", package.message));
		lines.push(format!(
			"  trusted publishing: {}",
			trusted_publishing_status_label(package.trusted_publishing.status)
		));
		lines.push(format!(
			"  trust message: {}",
			package.trusted_publishing.message
		));
		if let Some(repository) = &package.trusted_publishing.repository {
			lines.push(format!("  repository: {repository}"));
		}
		if let Some(workflow) = &package.trusted_publishing.workflow {
			lines.push(format!("  workflow: {workflow}"));
		}
		if let Some(environment) = &package.trusted_publishing.environment {
			lines.push(format!("  environment: {environment}"));
		}
		if let Some(setup_url) = &package.trusted_publishing.setup_url {
			lines.push(format!("  setup: {setup_url}"));
			lines.push("  next: open the setup URL, configure trusted publishing for this package, then rerun `mc publish`".to_string());
		}
	}

	lines
}

fn render_package_publish_report_markdown(
	report: &package_publish::PackagePublishReport,
	color: bool,
) -> Vec<String> {
	if report.packages.is_empty() {
		return vec!["- no packages matched the publishing criteria".to_string()];
	}

	let mut lines = Vec::new();
	for package in &report.packages {
		lines.push(format!(
			"- **{}** {} via {} → {}",
			paint_markdown_inline(
				&format!("`{}`", package.package),
				MarkdownStyle::Code,
				color,
			),
			paint_markdown_inline(
				&format!("`{}`", package.version),
				MarkdownStyle::Code,
				color,
			),
			paint_markdown_inline(
				&format!("`{}`", package.registry),
				MarkdownStyle::Code,
				color,
			),
			package_publish_status_label(package.status),
		));
		lines.push(format!("- **Ecosystem:** {}", package.ecosystem));
		lines.push(format!(
			"- **Placeholder:** {}",
			yes_no(package.placeholder)
		));
		lines.push(format!("- **Publish:** {}", package.message));
		lines.push(format!(
			"- **Trusted publishing:** {}",
			trusted_publishing_status_label(package.trusted_publishing.status)
		));
		lines.push(format!(
			"- **Trust message:** {}",
			package.trusted_publishing.message
		));

		push_optional_markdown_code_detail(
			&mut lines,
			"Repository",
			package.trusted_publishing.repository.as_deref(),
			color,
		);
		push_optional_markdown_code_detail(
			&mut lines,
			"Workflow",
			package.trusted_publishing.workflow.as_deref(),
			color,
		);
		push_optional_markdown_code_detail(
			&mut lines,
			"Environment",
			package.trusted_publishing.environment.as_deref(),
			color,
		);
		if let Some(setup_url) = &package.trusted_publishing.setup_url {
			lines.push(format!(
				"- **Setup:** {}",
				paint_markdown_inline(&format!("`{setup_url}`"), MarkdownStyle::Code, color,)
			));
			lines.push(
				"- **Next:** open the setup URL, configure trusted publishing for this package, then rerun `mc publish`"
					.to_string(),
			);
		}
	}

	lines
}

fn package_publish_status_label(status: package_publish::PackagePublishStatus) -> &'static str {
	match status {
		package_publish::PackagePublishStatus::Planned => "planned",
		package_publish::PackagePublishStatus::Published => "published",
		package_publish::PackagePublishStatus::SkippedExisting => "skipped-existing",
		package_publish::PackagePublishStatus::SkippedExternal => "skipped-external",
	}
}

fn trusted_publishing_status_label(
	status: package_publish::TrustedPublishingStatus,
) -> &'static str {
	match status {
		package_publish::TrustedPublishingStatus::Disabled => "disabled",
		package_publish::TrustedPublishingStatus::Planned => "planned",
		package_publish::TrustedPublishingStatus::Configured => "configured",
		package_publish::TrustedPublishingStatus::ManualActionRequired => "manual-action-required",
	}
}

pub(crate) fn render_retarget_release_report(report: &RetargetReleaseReport) -> String {
	let mut lines = vec!["repair release:".to_string()];
	lines.push(format!("  from: {}", report.from));
	lines.push(format!(
		"  resolved commit: {}",
		short_commit_sha(&report.resolved_from_commit)
	));
	lines.push(format!(
		"  record commit: {}",
		short_commit_sha(&report.record_commit)
	));
	lines.push(format!(
		"  target: {}",
		short_commit_sha(&report.target_commit)
	));
	lines.push(format!(
		"  descendant: {}",
		if report.is_descendant { "yes" } else { "no" }
	));
	lines.push(format!(
		"  force: {}",
		if report.force { "yes" } else { "no" }
	));
	if !report.git_tag_results.is_empty() {
		lines.push("  tags to move:".to_string());
		for tag_result in &report.git_tag_results {
			lines.push(format!(
				"    - {} ({} -> {}) [{}]",
				tag_result.tag_name,
				short_commit_sha(&tag_result.from_commit),
				short_commit_sha(&tag_result.to_commit),
				retarget_operation_label(tag_result.operation),
			));
		}
	}
	lines.push(format!(
		"  provider sync: {}",
		if !report.sync_provider {
			"disabled".to_string()
		} else if let Some(provider_result) = report.provider_results.first() {
			provider_result.provider.to_string()
		} else {
			"none".to_string()
		}
	));
	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines.join("\n")
}

fn push_optional_markdown_code_detail(
	lines: &mut Vec<String>,
	label: &str,
	value: Option<&str>,
	color: bool,
) {
	lines.extend(value.map(|value| {
		format!(
			"- **{label}:** {}",
			paint_markdown_inline(&format!("`{value}`"), MarkdownStyle::Code, color,)
		)
	}));
}

pub(crate) fn retarget_operation_label(operation: RetargetOperation) -> &'static str {
	match operation {
		RetargetOperation::Planned => "planned",
		RetargetOperation::Moved => "moved",
		RetargetOperation::AlreadyUpToDate => "already_up_to_date",
		RetargetOperation::Skipped => "skipped",
		RetargetOperation::Failed => "failed",
	}
}

fn interpolate_cli_command_command(
	context: &CliContext,
	command: &str,
	variables: Option<&BTreeMap<String, CommandVariable>>,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> String {
	let template_context = build_cli_template_context(context, step_inputs, variables);
	let jinja_context =
		minijinja::Value::from_serialize(serde_json::Value::Object(template_context));
	render_jinja_template(command, &jinja_context).unwrap_or_else(|_| command.to_string())
}

fn cli_command_variable_value(context: &CliContext, variable: CommandVariable) -> String {
	let version = context
		.prepared_release
		.as_ref()
		.and_then(|prepared| prepared.version.as_deref())
		.unwrap_or("");
	let group_version = context
		.prepared_release
		.as_ref()
		.and_then(|prepared| prepared.group_version.as_deref())
		.unwrap_or(version);
	match variable {
		CommandVariable::Version => version.to_string(),
		CommandVariable::GroupVersion => group_version.to_string(),
		CommandVariable::ReleasedPackages => {
			context
				.prepared_release
				.as_ref()
				.map(|prepared| prepared.released_packages.join(","))
				.unwrap_or_default()
		}
		CommandVariable::ChangedFiles => {
			context
				.prepared_release
				.as_ref()
				.map(|prepared| {
					prepared
						.changed_files
						.iter()
						.map(|path| path.display().to_string())
						.collect::<Vec<_>>()
						.join(" ")
				})
				.unwrap_or_default()
		}
		CommandVariable::Changesets => {
			context
				.prepared_release
				.as_ref()
				.map(|prepared| {
					prepared
						.changeset_paths
						.iter()
						.map(|path| path.display().to_string())
						.collect::<Vec<_>>()
						.join(" ")
				})
				.unwrap_or_default()
		}
	}
}

pub(crate) fn render_cli_command_result(
	cli_command: &CliCommandDefinition,
	context: &CliContext,
) -> String {
	if let Some(report) = &context.retarget_report {
		return render_retarget_release_report(report);
	}

	let mut lines = vec![format!(
		"command `{}` completed{}",
		cli_command.name,
		if context.dry_run { " (dry-run)" } else { "" }
	)];

	if let Some(prepared_release) = &context.prepared_release {
		render_prepared_release_summary(&mut lines, prepared_release, context);
	}

	if let Some(report) = &context.package_publish_report {
		lines.extend(render_package_publish_report(report));
	}
	if let Some(report) = &context.rate_limit_report {
		lines.push("publish rate limits:".to_string());
		if report.windows.is_empty() {
			lines.push("- no publish operations matched the current plan".to_string());
		} else {
			for window in &report.windows {
				lines.push(format!(
					"- {} {} pending={} batches={} confidence={:?}",
					window.registry,
					window.operation,
					window.pending,
					window.batches_required,
					window.confidence
				));
				if let Some(limit) = window.limit {
					lines.push(format!("  limit: {limit}"));
				}
				if let Some(window_seconds) = window.window_seconds {
					lines.push(format!("  window: {window_seconds}s"));
				}
				lines.push(format!("  notes: {}", window.notes));
			}
			if !report.batches.is_empty() {
				lines.push("planned batches:".to_string());
				for batch in &report.batches {
					lines.push(format!(
						"- {} batch {}/{} packages: {}",
						batch.registry,
						batch.batch_index,
						batch.total_batches,
						batch.packages.join(", ")
					));
					if let Some(wait_seconds) = batch.recommended_wait_seconds {
						lines.push(format!("  wait: {wait_seconds}s before this batch"));
					}
				}
			}
		}
		for warning in &report.warnings {
			lines.push(format!("- warning: {warning}"));
		}
	}
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		lines.push(format!("changeset policy: {}", evaluation.status));
		lines.push(evaluation.summary.clone());
		lines.extend((!evaluation.matched_skip_labels.is_empty()).then(|| {
			format!(
				"matched skip labels: {}",
				evaluation.matched_skip_labels.join(", ")
			)
		}));
		if !evaluation.matched_paths.is_empty() {
			lines.push("matched paths:".to_string());
			for path in &evaluation.matched_paths {
				lines.push(format!("- {path}"));
			}
		}
		if !evaluation.changeset_paths.is_empty() {
			lines.push("changeset files:".to_string());
			for path in &evaluation.changeset_paths {
				lines.push(format!("- {path}"));
			}
		}
		if !evaluation.errors.is_empty() {
			lines.push("errors:".to_string());
			for error in &evaluation.errors {
				lines.push(format!("- {error}"));
			}
		}
	}
	if !context.command_logs.is_empty() {
		lines.push("commands:".to_string());
		for log in &context.command_logs {
			lines.push(format!("- {log}"));
		}
	}
	lines.join("\n")
}

fn render_prepared_release_summary(
	lines: &mut Vec<String>,
	prepared_release: &PreparedRelease,
	context: &CliContext,
) {
	if let Some(version) = &prepared_release.version {
		lines.push(format!("version: {version}"));
	}

	if !prepared_release.released_packages.is_empty() {
		lines.push(format!(
			"released packages: {}",
			prepared_release.released_packages.join(", ")
		));
	}

	if !prepared_release.release_targets.is_empty() {
		lines.push("release targets:".to_string());
		for target in &prepared_release.release_targets {
			lines.push(format!(
				"- {} {} -> {} (tag: {}, release: {})",
				target.kind, target.id, target.tag_name, target.tag, target.release,
			));
		}
	}

	if let Some(path) = &context.release_manifest_path {
		lines.push(format!("release manifest: {}", path.display()));
	}

	if !context.release_results.is_empty() {
		lines.push("releases:".to_string());
		for release in &context.release_results {
			lines.push(format!("- {release}"));
		}
	}

	if let Some(release_commit_report) = &context.release_commit_report {
		lines.extend(render_release_commit_report(release_commit_report));
	}

	if let Some(release_request_result) = &context.release_request_result {
		lines.push("release request:".to_string());
		lines.push(format!("- {release_request_result}"));
	}

	if !context.issue_comment_results.is_empty() {
		lines.push("issue comments:".to_string());
		for issue_comment in &context.issue_comment_results {
			lines.push(format!("- {issue_comment}"));
		}
	}

	append_changed_file_lines(lines, &prepared_release.changed_files);

	if context.show_diff && !context.prepared_file_diffs.is_empty() {
		lines.push("file diffs:".to_string());
		for (index, file_diff) in context.prepared_file_diffs.iter().enumerate() {
			if index > 0 {
				lines.push(String::new());
			}
			lines.push(file_diff.display_diff.clone());
		}
	}

	if prepared_release.deleted_changesets.is_empty() {
		return;
	}

	lines.push("deleted changesets:".to_string());
	for path in &prepared_release.deleted_changesets {
		lines.push(format!("- {}", path.display()));
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ReleaseVersionSummary {
	packages: BTreeMap<String, String>,
	groups: BTreeMap<String, String>,
}

fn build_release_version_summary(prepared_release: &PreparedRelease) -> ReleaseVersionSummary {
	ReleaseVersionSummary {
		packages: prepared_release
			.plan
			.decisions
			.iter()
			.filter(|decision| decision.recommended_bump.is_release())
			.filter_map(|decision| {
				decision
					.planned_version
					.as_ref()
					.map(|version| (decision.package_id.clone(), version.to_string()))
			})
			.collect(),
		groups: prepared_release
			.plan
			.groups
			.iter()
			.filter(|group| group.recommended_bump.is_release())
			.filter_map(|group| {
				group
					.planned_version
					.as_ref()
					.map(|version| (group.group_id.clone(), version.to_string()))
			})
			.collect(),
	}
}

fn render_release_version_summary_text(summary: &ReleaseVersionSummary) -> String {
	let mut lines = Vec::new();
	if !summary.groups.is_empty() {
		lines.push("group versions:".to_string());
		for (group, version) in &summary.groups {
			lines.push(format!("- {group}: {version}"));
		}
	}
	if !summary.packages.is_empty() {
		lines.push("package versions:".to_string());
		for (package, version) in &summary.packages {
			lines.push(format!("- {package}: {version}"));
		}
	}
	if lines.is_empty() {
		return "no package or group versions were planned".to_string();
	}
	lines.join("\n")
}

fn render_release_version_summary_markdown(summary: &ReleaseVersionSummary) -> String {
	if summary.groups.is_empty() && summary.packages.is_empty() {
		return "No package or group versions were planned.".to_string();
	}
	let color = stdout_supports_color();
	let mut sections = Vec::new();
	if !summary.groups.is_empty() {
		let lines = summary
			.groups
			.iter()
			.map(|(group, version)| {
				format!(
					"- {}: {}",
					paint_markdown_inline(&format!("`{group}`"), MarkdownStyle::Code, color),
					paint_markdown_inline(&format!("`{version}`"), MarkdownStyle::Code, color),
				)
			})
			.collect::<Vec<_>>();
		sections.push(render_markdown_section("Group versions", &lines, color));
	}
	if !summary.packages.is_empty() {
		let lines = summary
			.packages
			.iter()
			.map(|(package, version)| {
				format!(
					"- {}: {}",
					paint_markdown_inline(&format!("`{package}`"), MarkdownStyle::Code, color),
					paint_markdown_inline(&format!("`{version}`"), MarkdownStyle::Code, color),
				)
			})
			.collect::<Vec<_>>();
		sections.push(render_markdown_section("Package versions", &lines, color));
	}
	sections.join("\n\n")
}

fn render_display_versions_output(
	prepared_release: &PreparedRelease,
	inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	let summary = build_release_version_summary(prepared_release);
	match cli_command_output_format(inputs)? {
		OutputFormat::Json => render_json_output(&summary, "display versions"),
		OutputFormat::Markdown => Ok(render_release_version_summary_markdown(&summary)),
		OutputFormat::Text => Ok(render_release_version_summary_text(&summary)),
	}
}

fn append_changed_file_lines(lines: &mut Vec<String>, changed_files: &[PathBuf]) {
	if !changed_files.is_empty() {
		lines.push("changed files:".to_string());
		lines.extend(
			changed_files
				.iter()
				.map(|path| format!("- {}", path.display())),
		);
	}
}

pub(crate) fn render_cli_command_markdown_result(
	cli_command: &CliCommandDefinition,
	context: &CliContext,
) -> String {
	if context.prepared_release.is_none()
		&& context.package_publish_report.is_none()
		&& context.rate_limit_report.is_none()
	{
		return render_cli_command_result(cli_command, context);
	}

	let color = stdout_supports_color();
	let mut sections = vec![format!(
		"# {}{}",
		paint_markdown_inline(
			&format!("`{}`", cli_command.name),
			MarkdownStyle::Title,
			color
		),
		if context.dry_run {
			format!(
				" {}",
				paint_markdown_inline("(dry-run)", MarkdownStyle::Muted, color)
			)
		} else {
			String::new()
		}
	)];

	if let Some(prepared_release) = &context.prepared_release {
		let mut summary = Vec::new();
		if let Some(version) = &prepared_release.version {
			summary.push(format!(
				"- **Version:** {}",
				paint_markdown_inline(&format!("`{version}`"), MarkdownStyle::Code, color)
			));
		}
		if !prepared_release.released_packages.is_empty() {
			summary.push(format!(
				"- **Released packages:** {}",
				prepared_release
					.released_packages
					.iter()
					.map(|package| {
						paint_markdown_inline(&format!("`{package}`"), MarkdownStyle::Code, color)
					})
					.collect::<Vec<_>>()
					.join(", ")
			));
		}
		if !summary.is_empty() {
			sections.push(render_markdown_section("Summary", &summary, color));
		}
		if !prepared_release.release_targets.is_empty() {
			let mut lines = Vec::new();
			for target in &prepared_release.release_targets {
				lines.push(format!(
					"- **{} {}** → {}",
					target.kind,
					paint_markdown_inline(&format!("`{}`", target.id), MarkdownStyle::Code, color),
					paint_markdown_inline(
						&format!("`{}`", target.tag_name),
						MarkdownStyle::Code,
						color,
					),
				));
				lines.push(format!(
					"  - tag: {} · release: {}",
					yes_no(target.tag),
					yes_no(target.release)
				));
			}
			sections.push(render_markdown_section("Release targets", &lines, color));
		}
		if let Some(path) = &context.release_manifest_path {
			sections.push(render_markdown_section(
				"Release manifest",
				&[format!(
					"- {}",
					paint_markdown_inline(
						&format!("`{}`", path.display()),
						MarkdownStyle::Code,
						color,
					)
				)],
				color,
			));
		}
		if !context.release_results.is_empty() {
			let lines = context
				.release_results
				.iter()
				.map(|release| format!("- {release}"))
				.collect::<Vec<_>>();
			sections.push(render_markdown_section("Releases", &lines, color));
		}
		if let Some(release_commit_report) = &context.release_commit_report {
			sections.push(render_markdown_section(
				"Release commit",
				&render_release_commit_report_markdown(release_commit_report, color),
				color,
			));
		}
		if let Some(release_request_result) = &context.release_request_result {
			sections.push(render_markdown_section(
				"Release request",
				&[format!("- {release_request_result}")],
				color,
			));
		}
		if !context.issue_comment_results.is_empty() {
			let lines = context
				.issue_comment_results
				.iter()
				.map(|issue_comment| format!("- {issue_comment}"))
				.collect::<Vec<_>>();
			sections.push(render_markdown_section("Issue comments", &lines, color));
		}
		if !prepared_release.changed_files.is_empty() {
			let lines = prepared_release
				.changed_files
				.iter()
				.map(|path| {
					format!(
						"- {}",
						paint_markdown_inline(
							&format!("`{}`", path.display()),
							MarkdownStyle::Code,
							color,
						)
					)
				})
				.collect::<Vec<_>>();
			sections.push(render_markdown_section("Changed files", &lines, color));
		}
		if context.show_diff && !context.prepared_file_diffs.is_empty() {
			let mut lines = Vec::new();
			for file_diff in &context.prepared_file_diffs {
				lines.push(format!(
					"### {}",
					paint_markdown_inline(
						&format!("`{}`", file_diff.path.display()),
						MarkdownStyle::Subtitle,
						color,
					)
				));
				lines.push("```diff".to_string());
				lines.extend(file_diff.display_diff.lines().map(ToString::to_string));
				lines.push("```".to_string());
				lines.push(String::new());
			}
			while lines.last().is_some_and(String::is_empty) {
				lines.pop();
			}
			sections.push(render_markdown_section("File diffs", &lines, color));
		}
		if !prepared_release.deleted_changesets.is_empty() {
			let lines = prepared_release
				.deleted_changesets
				.iter()
				.map(|path| {
					format!(
						"- {}",
						paint_markdown_inline(
							&format!("`{}`", path.display()),
							MarkdownStyle::Code,
							color,
						)
					)
				})
				.collect::<Vec<_>>();
			sections.push(render_markdown_section("Deleted changesets", &lines, color));
		}
	}
	if let Some(report) = &context.package_publish_report {
		let title = match report.mode {
			package_publish::PackagePublishRunMode::Placeholder => "Placeholder publishing",
			package_publish::PackagePublishRunMode::Release => "Package publishing",
		};
		sections.push(render_markdown_section(
			title,
			&render_package_publish_report_markdown(report, color),
			color,
		));
	}
	if !context.command_logs.is_empty() {
		let lines = context
			.command_logs
			.iter()
			.map(|log| format!("- {log}"))
			.collect::<Vec<_>>();
		sections.push(render_markdown_section("Commands", &lines, color));
	}
	sections.join("\n\n")
}

#[derive(Clone, Copy)]
enum MarkdownStyle {
	Title,
	Subtitle,
	Code,
	Muted,
}

fn stdout_supports_color() -> bool {
	std::io::stdout().is_terminal()
		&& std::env::var_os("NO_COLOR").is_none()
		&& std::env::var("TERM").is_ok_and(|term| term != "dumb")
}

fn paint_markdown_inline(text: &str, style: MarkdownStyle, color: bool) -> String {
	if !color {
		return text.to_string();
	}
	let code = match style {
		MarkdownStyle::Title => "36;1",
		MarkdownStyle::Subtitle => "37;1",
		MarkdownStyle::Code => "35",
		MarkdownStyle::Muted => "2",
	};
	format!("\u{1b}[{code}m{text}\u{1b}[0m")
}

fn render_markdown_section(title: &str, lines: &[String], color: bool) -> String {
	if lines.is_empty() {
		return format!(
			"## {}",
			paint_markdown_inline(title, MarkdownStyle::Subtitle, color)
		);
	}
	format!(
		"## {}\n\n{}",
		paint_markdown_inline(title, MarkdownStyle::Subtitle, color),
		lines.join("\n")
	)
}

fn render_release_commit_report_markdown(report: &CommitReleaseReport, color: bool) -> Vec<String> {
	let mut lines = vec![format!("- **Subject:** {}", report.subject)];
	if let Some(commit) = &report.commit {
		lines.push(format!(
			"- **Commit:** {}",
			paint_markdown_inline(
				&format!("`{}`", short_commit_sha(commit)),
				MarkdownStyle::Code,
				color,
			)
		));
	}
	if !report.tracked_paths.is_empty() {
		lines.push("- **Tracked paths:**".to_string());
		lines.extend(report.tracked_paths.iter().map(|path| {
			format!(
				"  - {}",
				paint_markdown_inline(&format!("`{}`", path.display()), MarkdownStyle::Code, color,)
			)
		}));
	}
	lines.push(format!("- **Status:** {}", report.status.replace('_', "-")));
	lines
}

fn yes_no(value: bool) -> &'static str {
	if value { "yes" } else { "no" }
}

fn cli_command_output_format(
	inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<OutputFormat> {
	inputs
		.get("format")
		.and_then(|values| values.first())
		.map_or(Ok(OutputFormat::Markdown), |value| {
			parse_output_format(value)
		})
}

#[must_use = "the output format result must be checked"]
pub(crate) fn parse_output_format(value: &str) -> MonochangeResult<OutputFormat> {
	match value {
		"text" => Ok(OutputFormat::Text),
		"markdown" | "md" => Ok(OutputFormat::Markdown),
		"json" => Ok(OutputFormat::Json),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported output format `{other}`"
			)))
		}
	}
}

/// Render raw markdown into terminal-styled text when stdout is a TTY.
///
/// When stdout is not an interactive terminal (e.g. piped to a file),
/// the original markdown string is returned unchanged so that downstream
/// consumers still receive valid markdown.
pub(crate) fn render_markdown_if_terminal(markdown: &str, is_terminal: bool) -> String {
	if is_terminal {
		termimad::MadSkin::default().term_text(markdown).to_string()
	} else {
		markdown.to_string()
	}
}

pub(crate) fn maybe_render_markdown_for_terminal(markdown: &str) -> String {
	render_markdown_if_terminal(markdown, std::io::stdout().is_terminal())
}

#[must_use = "the change bump result must be checked"]
pub(crate) fn parse_change_bump(value: &str) -> MonochangeResult<ChangeBump> {
	match value {
		"none" => Ok(ChangeBump::None),
		"patch" => Ok(ChangeBump::Patch),
		"minor" => Ok(ChangeBump::Minor),
		"major" => Ok(ChangeBump::Major),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported bump `{other}`"
			)))
		}
	}
}

fn execute_create_change_file_step(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	let is_interactive = step_inputs
		.get("interactive")
		.and_then(|values| values.first())
		.is_some_and(|value| value == "true");

	if is_interactive {
		let options = interactive::InteractiveOptions {
			caused_by: step_inputs.get("caused_by").cloned().unwrap_or_default(),
			reason: step_inputs
				.get("reason")
				.and_then(|values| values.first())
				.cloned(),
			details: step_inputs
				.get("details")
				.and_then(|values| values.first())
				.cloned(),
		};
		let result = interactive::run_interactive_change(configuration, &options)?;
		let output_path = step_inputs
			.get("output")
			.and_then(|values| values.first())
			.map(PathBuf::from);
		let path = add_interactive_change_file(root, &result, output_path.as_deref())?;
		Ok(format!(
			"wrote change file {}",
			root_relative(root, &path).display()
		))
	} else {
		let package_refs = step_inputs.get("package").cloned().unwrap_or_default();
		if package_refs.is_empty() {
			return Err(MonochangeError::Config(
				"command `change` requires at least one `--package` value or `--interactive` mode"
					.to_string(),
			));
		}
		let bump = if let Some(value) = step_inputs.get("bump").and_then(|values| values.first()) {
			parse_change_bump(value)?
		} else if step_inputs
			.get("type")
			.and_then(|values| values.first())
			.is_some()
		{
			ChangeBump::None
		} else {
			ChangeBump::Patch
		};
		let version = step_inputs
			.get("version")
			.and_then(|values| values.first())
			.cloned();
		let reason = step_inputs
			.get("reason")
			.and_then(|values| values.first())
			.cloned()
			.ok_or_else(|| {
				MonochangeError::Config("command `change` requires a `--reason` value".to_string())
			})?;
		let change_type = step_inputs
			.get("type")
			.and_then(|values| values.first())
			.cloned();
		let details = step_inputs
			.get("details")
			.and_then(|values| values.first())
			.cloned();
		let caused_by = step_inputs.get("caused_by").cloned().unwrap_or_default();
		let output_path = step_inputs
			.get("output")
			.and_then(|values| values.first())
			.map(PathBuf::from);
		let path = add_change_file(
			root,
			AddChangeFileRequest::builder()
				.package_refs(&package_refs)
				.bump(bump.into())
				.reason(&reason)
				.version(version.as_deref())
				.change_type(change_type.as_deref())
				.caused_by(&caused_by)
				.details(details.as_deref())
				.output(output_path.as_deref())
				.build(),
		)?;
		Ok(format!(
			"wrote change file {}",
			root_relative(root, &path).display()
		))
	}
}

fn execute_affected_packages_step(
	root: &Path,
	step_inputs: &BTreeMap<String, Vec<String>>,
	quiet: bool,
) -> MonochangeResult<ChangesetPolicyEvaluation> {
	let since = step_inputs
		.get("since")
		.and_then(|values| values.first().cloned());
	let explicit_paths = step_inputs
		.get("changed_paths")
		.cloned()
		.unwrap_or_default();
	let changed_paths = match &since {
		Some(rev) => {
			if !quiet && !explicit_paths.is_empty() {
				eprintln!("warning: --since takes priority; --changed-paths was ignored");
			}

			compute_changed_paths_since(root, rev)?
		}
		None => explicit_paths,
	};
	let labels = step_inputs.get("label").cloned().unwrap_or_default();
	let enforce = step_inputs
		.get("verify")
		.is_some_and(|values| values.iter().any(|v| v == "true"));
	let mut evaluation = affected_packages(root, &changed_paths, &labels)?;
	evaluation.enforce = enforce;
	Ok(evaluation)
}

fn report_cli_step_failure(
	progress: &mut CliProgressReporter,
	show_progress: bool,
	step_index: usize,
	step: &CliStepDefinition,
	elapsed: Duration,
	error: &MonochangeError,
) {
	if !show_progress {
		return;
	}

	let progress_error = progress_error_detail(error).to_string();
	progress.step_failed(step_index, step, elapsed, &progress_error);
}

fn maybe_fail_enforced_changeset_policy(
	evaluation: &ChangesetPolicyEvaluation,
	quiet: bool,
	rendered: String,
) -> MonochangeResult<String> {
	match (
		evaluation.enforce,
		evaluation.status == ChangesetPolicyStatus::Failed,
	) {
		(true, true) => {
			if !quiet {
				println!("{rendered}");
			}

			Err(MonochangeError::Config(evaluation.summary.clone()))
		}
		_ => Ok(rendered),
	}
}

fn save_prepared_release_artifact(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	context: &CliContext,
	prepared_release_path: Option<&Path>,
) -> MonochangeResult<()> {
	let Some(prepared_release) = &context.prepared_release else {
		return Ok(());
	};

	let save_result = save_prepared_release_execution(
		root,
		configuration,
		prepared_release,
		&context.prepared_file_diffs,
		prepared_release_path,
	);

	match (prepared_release_path.is_some(), save_result) {
		(_, Ok(())) => Ok(()),
		(true, Err(error)) => Err(error),
		(false, Err(error)) => {
			tracing::warn!(%error, "failed to save prepared release artifact");
			Ok(())
		}
	}
}

fn resolve_command_output(
	cli_command: &CliCommandDefinition,
	context: &CliContext,
	dry_run: bool,
	output: Option<String>,
) -> MonochangeResult<String> {
	if let Some(prepared_release) = &context.prepared_release {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		return match format {
			OutputFormat::Json => {
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				render_release_cli_command_json(
					&manifest,
					&ReleaseCliJsonSections {
						releases: &context.release_requests,
						release_request: context.release_request.as_ref(),
						issue_comments: &context.issue_comment_plans,
						release_commit: context.release_commit_report.as_ref(),
						package_publish: context.package_publish_report.as_ref(),
						publish_rate_limits: context.rate_limit_report.as_ref(),
						file_diffs: if context.show_diff {
							&context.prepared_file_diffs
						} else {
							&[]
						},
					},
				)
			}
			OutputFormat::Markdown => Ok(render_cli_command_markdown_result(cli_command, context)),
			OutputFormat::Text => Ok(render_cli_command_result(cli_command, context)),
		};
	}
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		let rendered = match format {
			OutputFormat::Json => render_json_output(evaluation, "changeset policy evaluation")?,
			OutputFormat::Markdown | OutputFormat::Text => {
				render_cli_command_result(cli_command, context)
			}
		};

		return maybe_fail_enforced_changeset_policy(evaluation, context.quiet, rendered);
	}
	if let Some(report) = &context.changeset_diagnostics {
		let format = context
			.inputs
			.get("format")
			.and_then(|values| values.first())
			.map_or(Ok(OutputFormat::Markdown), |value| {
				parse_output_format(value)
			})?;
		let rendered = match format {
			OutputFormat::Json => render_json_output(report, "changeset diagnostics")?,
			OutputFormat::Markdown | OutputFormat::Text => render_changeset_diagnostics(report),
		};
		return Ok(rendered);
	}
	if let Some(report) = &context.retarget_report {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		let rendered = match format {
			OutputFormat::Json => {
				serde_json::to_string_pretty(report)
					.unwrap_or_else(|error| panic!("retarget report serialization bug: {error}"))
			}
			OutputFormat::Markdown | OutputFormat::Text => render_retarget_release_report(report),
		};
		return Ok(rendered);
	}
	if let Some(report) = &context.package_publish_report {
		let format = cli_command_output_format(&context.last_step_inputs)?;
		let rendered = match format {
			OutputFormat::Json => {
				render_publish_command_json(Some(report), context.rate_limit_report.as_ref())?
			}
			OutputFormat::Markdown => render_cli_command_markdown_result(cli_command, context),
			OutputFormat::Text => render_cli_command_result(cli_command, context),
		};
		return Ok(rendered);
	}
	if let Some(report) = &context.rate_limit_report {
		if let Some(ci_renderer) = requested_ci_renderer(&context.last_step_inputs)? {
			return render_publish_rate_limit_ci_snippet(report, ci_renderer);
		}
		let format = cli_command_output_format(&context.last_step_inputs)?;
		let rendered = match format {
			OutputFormat::Json => render_publish_command_json(None, Some(report))?,
			OutputFormat::Markdown | OutputFormat::Text => {
				render_cli_command_result(cli_command, context)
			}
		};
		return Ok(rendered);
	}
	if !context.command_logs.is_empty() {
		return Ok(render_cli_command_result(cli_command, context));
	}

	Ok(output.unwrap_or_else(|| {
		format!(
			"command `{}` completed{}",
			cli_command.name,
			if dry_run { " (dry-run)" } else { "" }
		)
	}))
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::fs;
	use std::io;
	use std::path::Path;
	use std::path::PathBuf;
	use std::sync::mpsc;
	use std::time::Duration;

	use monochange_config::load_workspace_configuration;
	use monochange_core::BumpSeverity;
	use monochange_core::ChangesetPolicyEvaluation;
	use monochange_core::ChangesetPolicyStatus;
	use monochange_core::CliCommandDefinition;
	use monochange_core::CliStepDefinition;
	use monochange_core::ReleaseOwnerKind;
	use monochange_core::ReleasePlan;
	use monochange_core::ShellConfig;
	use monochange_core::VersionFormat;
	use serde::Serialize;
	use tempfile::tempdir;

	use super::*;
	use crate::TEST_ENV_LOCK;

	fn cli_context() -> CliContext {
		CliContext {
			root: PathBuf::from("."),
			dry_run: false,
			quiet: false,
			show_diff: false,
			inputs: BTreeMap::new(),
			last_step_inputs: BTreeMap::new(),
			prepared_release: None,
			prepared_file_diffs: Vec::new(),
			release_manifest_path: None,
			release_requests: Vec::new(),
			release_results: Vec::new(),
			release_request: None,
			release_request_result: None,
			release_commit_report: None,
			package_publish_report: None,
			rate_limit_report: None,
			issue_comment_plans: Vec::new(),
			issue_comment_results: Vec::new(),
			changeset_policy_evaluation: None,
			changeset_diagnostics: None,
			retarget_report: None,
			step_outputs: BTreeMap::new(),
			command_logs: Vec::new(),
		}
	}

	fn sample_configuration(root: &Path) -> monochange_core::WorkspaceConfiguration {
		monochange_core::WorkspaceConfiguration {
			root_path: root.to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults::default(),
			changelog: ChangelogSettings::default(),
			packages: Vec::new(),
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

	fn sample_prepared_release() -> PreparedRelease {
		PreparedRelease {
			plan: ReleasePlan {
				workspace_root: PathBuf::from("."),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: Vec::new(),
			package_publications: Vec::new(),
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

	fn sample_prepared_release_with_versions() -> PreparedRelease {
		PreparedRelease {
			plan: ReleasePlan {
				workspace_root: PathBuf::from("."),
				decisions: vec![
					monochange_core::ReleaseDecision {
						package_id: "core".to_string(),
						trigger_type: "changeset".to_string(),
						recommended_bump: BumpSeverity::Minor,
						planned_version: Some(semver::Version::new(1, 2, 0)),
						group_id: Some("sdk".to_string()),
						reasons: vec!["feature".to_string()],
						upstream_sources: Vec::new(),
						warnings: Vec::new(),
					},
					monochange_core::ReleaseDecision {
						package_id: "web".to_string(),
						trigger_type: "changeset".to_string(),
						recommended_bump: BumpSeverity::Patch,
						planned_version: Some(semver::Version::new(1, 2, 1)),
						group_id: Some("sdk".to_string()),
						reasons: vec!["fix".to_string()],
						upstream_sources: Vec::new(),
						warnings: Vec::new(),
					},
					monochange_core::ReleaseDecision {
						package_id: "docs".to_string(),
						trigger_type: "changeset".to_string(),
						recommended_bump: BumpSeverity::None,
						planned_version: Some(semver::Version::new(9, 9, 9)),
						group_id: None,
						reasons: Vec::new(),
						upstream_sources: Vec::new(),
						warnings: Vec::new(),
					},
				],
				groups: vec![monochange_core::PlannedVersionGroup {
					group_id: "sdk".to_string(),
					display_name: "SDK".to_string(),
					members: vec!["core".to_string(), "web".to_string()],
					mismatch_detected: false,
					planned_version: Some(semver::Version::new(2, 0, 0)),
					recommended_bump: BumpSeverity::Minor,
				}],
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: vec!["core".to_string(), "web".to_string()],
			package_publications: Vec::new(),
			version: Some("1.2.1".to_string()),
			group_version: Some("2.0.0".to_string()),
			release_targets: Vec::new(),
			changed_files: vec![PathBuf::from("Cargo.toml")],
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			dry_run: true,
		}
	}

	fn parse_validate_matches(
		root: &Path,
	) -> (monochange_core::WorkspaceConfiguration, ArgMatches) {
		let configuration = load_workspace_configuration(root)
			.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
		let matches = build_command_with_cli("mc", &configuration.cli)
			.try_get_matches_from(["mc", "step:discover"])
			.unwrap_or_else(|error| panic!("discover matches: {error}"));
		(configuration, matches)
	}

	fn default_cli_command(name: &str) -> CliCommandDefinition {
		let command_name = if name.starts_with("step:") {
			name.to_string()
		} else {
			format!("step:{name}")
		};

		synthetic_step_command_definition(&command_name)
			.unwrap_or_else(|error| panic!("expected default cli command `{name}`: {error}"))
	}

	fn read_telemetry_events(path: &Path) -> Vec<serde_json::Value> {
		fs::read_to_string(path)
			.unwrap_or_else(|error| panic!("telemetry file should be written: {error}"))
			.lines()
			.map(|line| {
				serde_json::from_str(line)
					.unwrap_or_else(|error| panic!("valid telemetry json: {error}"))
			})
			.collect()
	}

	#[test]
	fn telemetry_progress_format_uses_stable_labels() {
		assert_eq!(telemetry_progress_format(ProgressFormat::Auto), "auto");
		assert_eq!(
			telemetry_progress_format(ProgressFormat::Unicode),
			"unicode"
		);
		assert_eq!(telemetry_progress_format(ProgressFormat::Ascii), "ascii");
		assert_eq!(telemetry_progress_format(ProgressFormat::Json), "json");
	}

	#[test]
	fn default_cli_command_accepts_prefixed_step_names() {
		let command = default_cli_command("step:discover");

		assert_eq!(command.name, "step:discover");
	}

	fn sample_package_publish_outcome(
		status: package_publish::PackagePublishStatus,
		trust_status: package_publish::TrustedPublishingStatus,
	) -> package_publish::PackagePublishOutcome {
		package_publish::PackagePublishOutcome {
			package: "@scope/pkg".to_string(),
			ecosystem: Ecosystem::Npm,
			registry: "npm".to_string(),
			version: "1.2.3".to_string(),
			status,
			message: "published package to npm".to_string(),
			placeholder: false,
			trusted_publishing: package_publish::TrustedPublishingOutcome {
				status: trust_status,
				repository: Some("monochange/monochange".to_string()),
				workflow: Some("publish.yml".to_string()),
				environment: Some("release".to_string()),
				setup_url: Some("https://docs.npmjs.com/cli/v11/commands/npm-trust".to_string()),
				message: "trusted publishing already configured".to_string(),
			},
		}
	}

	fn sample_rate_limit_report() -> monochange_core::PublishRateLimitReport {
		monochange_core::PublishRateLimitReport {
			dry_run: true,
			windows: vec![monochange_core::RegistryRateLimitWindowPlan {
				registry: monochange_core::RegistryKind::PubDev,
				operation: monochange_core::RateLimitOperation::Publish,
				limit: Some(12),
				window_seconds: Some(86_400),
				pending: 13,
				batches_required: 2,
				fits_single_window: false,
				confidence: monochange_core::RateLimitConfidence::Medium,
				notes: "pub.dev limit".to_string(),
				evidence: Vec::new(),
			}],
			batches: vec![
				monochange_core::PublishRateLimitBatch {
					registry: monochange_core::RegistryKind::PubDev,
					operation: monochange_core::RateLimitOperation::Publish,
					batch_index: 1,
					total_batches: 2,
					packages: vec!["pkg-a".to_string()],
					recommended_wait_seconds: None,
				},
				monochange_core::PublishRateLimitBatch {
					registry: monochange_core::RegistryKind::PubDev,
					operation: monochange_core::RateLimitOperation::Publish,
					batch_index: 2,
					total_batches: 2,
					packages: vec!["pkg-b".to_string()],
					recommended_wait_seconds: Some(86_400),
				},
			],
			warnings: vec!["needs 2 batches".to_string()],
		}
	}

	fn git_in_dir(root: &Path, args: &[&str]) {
		let status = std::process::Command::new("git")
			.current_dir(root)
			.args(args)
			.status()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(status.success(), "git {args:?} failed");
	}

	#[test]
	fn evaluate_cli_step_condition_returns_false_for_blank_conditions() {
		assert!(
			!evaluate_cli_step_condition("   ", &cli_context(), &BTreeMap::new()).unwrap_or_else(
				|error| panic!("blank conditions should be treated as false: {error}")
			)
		);
	}

	#[test]
	fn parse_template_as_boolean_supports_number_null_and_single_item_arrays() {
		assert!(
			parse_template_as_boolean(&serde_json::json!(2), "{{ count }}")
				.unwrap_or_else(|error| panic!("non-zero numbers should be truthy: {error}"))
		);
		assert!(
			!parse_template_as_boolean(&serde_json::Value::Null, "{{ release }}")
				.unwrap_or_else(|error| panic!("null values should be falsey: {error}"))
		);
		assert!(
			!parse_template_as_boolean(&serde_json::json!([""]), "{{ items }}").unwrap_or_else(
				|error| panic!("single-item arrays should recurse into the item value: {error}")
			)
		);
	}

	#[test]
	fn parse_template_as_boolean_rejects_objects() {
		let error =
			parse_template_as_boolean(&serde_json::json!({ "nested": true }), "{{ inputs }}")
				.unwrap_err();
		assert!(error.to_string().contains("is not a scalar boolean value"));
	}

	#[test]
	fn render_helpers_cover_release_commit_and_markdown_sections() {
		let report = CommitReleaseReport {
			subject: "chore(release): publish".to_string(),
			body: "body".to_string(),
			commit: Some("1234567890abcdef".to_string()),
			tracked_paths: vec![PathBuf::from("Cargo.toml"), PathBuf::from("CHANGELOG.md")],
			dry_run: false,
			status: "already_exists".to_string(),
		};
		let text_lines = render_release_commit_report(&report);
		assert!(
			text_lines
				.iter()
				.any(|line| line.contains("subject: chore(release): publish"))
		);
		assert!(
			text_lines
				.iter()
				.any(|line| line.contains("commit: 1234567"))
		);
		assert!(
			text_lines
				.iter()
				.any(|line| line.contains("tracked paths:"))
		);
		assert!(
			text_lines
				.iter()
				.any(|line| line.contains("status: already-exists"))
		);

		let markdown_lines = render_release_commit_report_markdown(&report, true);
		assert!(
			markdown_lines
				.iter()
				.any(|line| line.contains("**Subject:**"))
		);
		assert!(
			markdown_lines
				.iter()
				.any(|line| line.contains("**Tracked paths:**"))
		);

		assert_eq!(yes_no(true), "yes");
		assert_eq!(yes_no(false), "no");
		assert_eq!(
			paint_markdown_inline("plain", MarkdownStyle::Muted, false),
			"plain"
		);
		assert!(paint_markdown_inline("code", MarkdownStyle::Code, true).contains("\u{1b}[35m"));
		assert!(render_markdown_section("Empty", &[], false).starts_with("## Empty"));
	}

	#[derive(Debug)]
	struct BrokenSerialize;

	impl Serialize for BrokenSerialize {
		fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
		where
			S: serde::Serializer,
		{
			Err(serde::ser::Error::custom("broken serialize"))
		}
	}

	#[test]
	fn render_json_output_reports_context_on_serialization_failure() {
		let error = render_json_output(&BrokenSerialize, "changeset diagnostics")
			.unwrap_err()
			.to_string();
		assert!(error.contains("failed to render changeset diagnostics as json"));
		assert!(error.contains("broken serialize"));
	}

	#[test]
	fn execute_affected_packages_step_supports_since_git_input() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();
		fs::create_dir_all(root.join("crates/core/src"))
			.unwrap_or_else(|error| panic!("create workspace directories: {error}"));
		fs::write(
			root.join("monochange.toml"),
			r#"[defaults]
package_type = "cargo"

[changesets.verify]
enabled = true
required = true

[package.core]
path = "crates/core"
"#,
		)
		.unwrap_or_else(|error| panic!("write monochange config: {error}"));
		fs::write(
			root.join("Cargo.toml"),
			"[workspace]\nmembers = [\"crates/core\"]\n",
		)
		.unwrap_or_else(|error| panic!("write workspace Cargo.toml: {error}"));
		fs::write(
			root.join("crates/core/Cargo.toml"),
			"[package]\nname = \"core\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
		)
		.unwrap_or_else(|error| panic!("write package Cargo.toml: {error}"));
		fs::write(
			root.join("crates/core/src/lib.rs"),
			"pub fn version() -> &'static str { \"v1\" }\n",
		)
		.unwrap_or_else(|error| panic!("write initial source file: {error}"));

		git_in_dir(root, &["init", "-b", "main"]);
		git_in_dir(root, &["config", "user.name", "monochange Tests"]);
		git_in_dir(root, &["config", "user.email", "monochange@example.com"]);
		git_in_dir(root, &["add", "."]);
		git_in_dir(root, &["commit", "-m", "initial"]);

		fs::write(
			root.join("crates/core/src/lib.rs"),
			"pub fn version() -> &'static str { \"v2\" }\n",
		)
		.unwrap_or_else(|error| panic!("update source file: {error}"));

		let evaluation = execute_affected_packages_step(
			root,
			&BTreeMap::from([("since".to_string(), vec!["HEAD".to_string()])]),
			true,
		)
		.unwrap_or_else(|error| panic!("execute affected packages step: {error}"));

		assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
		assert_eq!(
			evaluation.changed_paths,
			vec!["crates/core/src/lib.rs".to_string()]
		);
		assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
		assert_eq!(evaluation.uncovered_package_ids, vec!["core".to_string()]);
	}

	#[test]
	fn render_cli_command_results_include_release_details_policy_and_logs() {
		let cli_command = default_cli_command("prepare-release");
		let mut context = cli_context();
		context.show_diff = true;
		context.release_manifest_path = Some(PathBuf::from(".monochange/release.json"));
		context.release_results = vec!["published v1.2.3".to_string()];
		context.release_request_result = Some("opened release request".to_string());
		context.issue_comment_results = vec!["commented on #42".to_string()];
		context.release_commit_report = Some(CommitReleaseReport {
			subject: "chore(release): publish".to_string(),
			body: "body".to_string(),
			commit: Some("abcdef1234567890".to_string()),
			tracked_paths: vec![PathBuf::from("Cargo.toml")],
			dry_run: false,
			status: "completed".to_string(),
		});
		context.prepared_release = Some(PreparedRelease {
			plan: ReleasePlan {
				workspace_root: PathBuf::from("."),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: vec![PathBuf::from(".changeset/feature.md")],
			changesets: Vec::new(),
			released_packages: vec!["core".to_string()],
			version: Some("1.2.3".to_string()),
			group_version: None,
			release_targets: vec![ReleaseTarget {
				id: "core".to_string(),
				kind: ReleaseOwnerKind::Package,
				version: "1.2.3".to_string(),
				tag: true,
				release: true,
				version_format: VersionFormat::Primary,
				tag_name: "v1.2.3".to_string(),
				members: Vec::new(),
				rendered_title: "core v1.2.3".to_string(),
				rendered_changelog_title: "core v1.2.3".to_string(),
			}],
			changed_files: vec![PathBuf::from("Cargo.toml")],
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: vec![PathBuf::from(".changeset/feature.md")],
			package_publications: Vec::new(),
			dry_run: true,
		});
		context.prepared_file_diffs = vec![PreparedFileDiff {
			path: PathBuf::from("Cargo.toml"),
			diff: "-old\n+new".to_string(),
			display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
		}];
		context.command_logs = vec!["ran cargo check".to_string()];
		context.changeset_policy_evaluation = Some(ChangesetPolicyEvaluation {
			enforce: true,
			required: true,
			status: ChangesetPolicyStatus::Failed,
			summary: "coverage missing".to_string(),
			comment: None,
			labels: Vec::new(),
			matched_skip_labels: vec!["skip-changeset".to_string()],
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			matched_paths: vec!["crates/core/src/lib.rs".to_string()],
			ignored_paths: Vec::new(),
			changeset_paths: vec![".changeset/feature.md".to_string()],
			affected_package_ids: vec!["core".to_string()],
			covered_package_ids: Vec::new(),
			uncovered_package_ids: vec!["core".to_string()],
			errors: vec!["missing changeset".to_string()],
		});

		let text = render_cli_command_result(&cli_command, &context);
		assert!(text.contains("release manifest: .monochange/release.json"));
		assert!(text.contains("releases:"));
		assert!(text.contains("release request:"));
		assert!(text.contains("issue comments:"));
		assert!(text.contains("changed files:"));
		assert!(text.contains("file diffs:"));
		assert!(text.contains("deleted changesets:"));
		assert!(text.contains("matched paths:"));
		assert!(text.contains("changeset files:"));
		assert!(text.contains("errors:"));
		assert!(text.contains("commands:"));

		let markdown = render_cli_command_markdown_result(&cli_command, &context);
		assert!(markdown.contains("## Release targets"));
		assert!(markdown.contains("## Release manifest"));
		assert!(markdown.contains("## Release commit"));
		assert!(markdown.contains("## Changed files"));
		assert!(markdown.contains("## File diffs"));
		assert!(markdown.contains("## Deleted changesets"));
		assert!(markdown.contains("## Commands"));
	}

	#[test]
	fn render_display_versions_output_supports_text_markdown_and_json() {
		let prepared_release = sample_prepared_release_with_versions();

		let text = render_display_versions_output(
			&prepared_release,
			&BTreeMap::from([("format".to_string(), vec!["text".to_string()])]),
		)
		.unwrap_or_else(|error| panic!("versions text output: {error}"));
		insta::assert_snapshot!("display_versions_text", text);

		let markdown = render_display_versions_output(
			&prepared_release,
			&BTreeMap::from([("format".to_string(), vec!["markdown".to_string()])]),
		)
		.unwrap_or_else(|error| panic!("versions markdown output: {error}"));
		insta::assert_snapshot!("display_versions_markdown", markdown);

		let json = render_display_versions_output(
			&prepared_release,
			&BTreeMap::from([("format".to_string(), vec!["json".to_string()])]),
		)
		.unwrap_or_else(|error| panic!("versions json output: {error}"));
		let parsed: serde_json::Value = serde_json::from_str(&json)
			.unwrap_or_else(|error| panic!("parse versions json output: {error}"));
		insta::assert_json_snapshot!("display_versions_json", parsed);
	}

	#[test]
	fn release_version_summary_renderers_cover_empty_and_single_section_states() {
		let empty = ReleaseVersionSummary {
			groups: BTreeMap::new(),
			packages: BTreeMap::new(),
		};
		assert_eq!(
			render_release_version_summary_text(&empty),
			"no package or group versions were planned"
		);
		assert_eq!(
			render_release_version_summary_markdown(&empty),
			"No package or group versions were planned."
		);

		let groups_only = ReleaseVersionSummary {
			groups: BTreeMap::from([("sdk".to_string(), "2.0.0".to_string())]),
			packages: BTreeMap::new(),
		};
		assert_eq!(
			render_release_version_summary_text(&groups_only),
			"group versions:\n- sdk: 2.0.0"
		);
		assert_eq!(
			render_release_version_summary_markdown(&groups_only),
			"## Group versions\n\n- `sdk`: `2.0.0`"
		);

		let packages_only = ReleaseVersionSummary {
			groups: BTreeMap::new(),
			packages: BTreeMap::from([
				("core".to_string(), "1.2.0".to_string()),
				("web".to_string(), "1.2.1".to_string()),
			]),
		};
		assert_eq!(
			render_release_version_summary_text(&packages_only),
			"package versions:\n- core: 1.2.0\n- web: 1.2.1"
		);
		assert_eq!(
			render_release_version_summary_markdown(&packages_only),
			"## Package versions\n\n- `core`: `1.2.0`\n- `web`: `1.2.1`"
		);
	}

	#[test]
	fn render_cli_command_results_include_package_publish_reports() {
		let cli_command = CliCommandDefinition {
			name: "publish".to_string(),
			help_text: Some("publish packages".to_string()),
			inputs: vec![monochange_core::CliInputDefinition {
				name: "format".to_string(),
				kind: CliInputKind::Choice,
				help_text: Some("Output format".to_string()),
				required: false,
				default: Some("text".to_string()),
				choices: vec![
					"text".to_string(),
					"markdown".to_string(),
					"json".to_string(),
				],
				short: None,
			}],
			steps: vec![CliStepDefinition::PublishPackages {
				name: Some("publish packages".to_string()),
				when: None,
				inputs: BTreeMap::new(),
			}],
		};
		let mut context = cli_context();
		context.package_publish_report = Some(package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Release,
			dry_run: false,
			packages: vec![package_publish::PackagePublishOutcome {
				package: "@scope/pkg".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: "npm".to_string(),
				version: "1.2.3".to_string(),
				status: package_publish::PackagePublishStatus::Published,
				message: "published package to npm".to_string(),
				placeholder: false,
				trusted_publishing: package_publish::TrustedPublishingOutcome {
					status: package_publish::TrustedPublishingStatus::Configured,
					repository: Some("monochange/monochange".to_string()),
					workflow: Some("publish.yml".to_string()),
					environment: Some("release".to_string()),
					setup_url: None,
					message: "trusted publishing already configured".to_string(),
				},
			}],
		});
		context.command_logs = vec!["ran npm trust".to_string()];

		let text = render_cli_command_result(&cli_command, &context);
		assert!(text.contains("package publishing:"));
		assert!(text.contains("@scope/pkg"));
		assert!(text.contains("trusted publishing: configured"));
		assert!(text.contains("repository: monochange/monochange"));
		assert!(text.contains("commands:"));

		let markdown = render_cli_command_markdown_result(&cli_command, &context);
		assert!(markdown.contains("## Package publishing"));
		assert!(markdown.contains("**Trusted publishing:** configured"));
		assert!(markdown.contains("**Workflow:** `publish.yml`"));
		assert!(markdown.contains("## Commands"));
	}

	#[test]
	fn render_package_publish_reports_cover_empty_and_detailed_variants() {
		let empty_placeholder = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: true,
			packages: Vec::new(),
		};
		let text_lines = render_package_publish_report(&empty_placeholder);
		assert_eq!(text_lines[0], "placeholder publishing:");
		assert_eq!(
			text_lines[1],
			"- no packages matched the publishing criteria"
		);
		assert_eq!(
			render_package_publish_report_markdown(&empty_placeholder, false),
			vec!["- no packages matched the publishing criteria".to_string()]
		);

		let detailed_report = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Release,
			dry_run: false,
			packages: vec![sample_package_publish_outcome(
				package_publish::PackagePublishStatus::SkippedExternal,
				package_publish::TrustedPublishingStatus::ManualActionRequired,
			)],
		};
		let text = render_package_publish_report(&detailed_report).join("\n");
		assert!(text.contains("repository: monochange/monochange"));
		assert!(text.contains("workflow: publish.yml"));
		assert!(text.contains("environment: release"));
		assert!(text.contains("setup: https://docs.npmjs.com/cli/v11/commands/npm-trust"));

		let markdown = render_package_publish_report_markdown(&detailed_report, false).join("\n");
		assert!(markdown.contains("**Repository:** `monochange/monochange`"));
		assert!(markdown.contains("**Workflow:** `publish.yml`"));
		assert!(markdown.contains("**Environment:** `release`"));
		assert!(
			markdown.contains("**Setup:** `https://docs.npmjs.com/cli/v11/commands/npm-trust`")
		);
	}

	#[test]
	fn render_package_publish_reports_include_manual_registry_guidance() {
		let report = package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Release,
			dry_run: false,
			packages: vec![package_publish::PackagePublishOutcome {
				package: "pkg".to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: "crates_io".to_string(),
				version: "1.2.3".to_string(),
				status: package_publish::PackagePublishStatus::SkippedExternal,
				message: "skipped built-in publish".to_string(),
				placeholder: false,
				trusted_publishing: package_publish::TrustedPublishingOutcome {
					status: package_publish::TrustedPublishingStatus::ManualActionRequired,
					repository: Some("monochange/monochange".to_string()),
					workflow: Some("publish.yml".to_string()),
					environment: Some("release".to_string()),
					setup_url: Some("https://crates.io/crates/pkg".to_string()),
					message:
						"configure trusted publishing manually for `pkg` before the next built-in release publish"
							.to_string(),
				},
			}],
		};

		let text = render_package_publish_report(&report).join("\n");
		assert!(text.contains("trusted publishing: manual-action-required"));
		assert!(text.contains("trust message: configure trusted publishing manually for `pkg`"));
		assert!(text.contains("setup: https://crates.io/crates/pkg"));
		assert!(text.contains("next: open the setup URL, configure trusted publishing for this package, then rerun `mc publish`"));

		let markdown = render_package_publish_report_markdown(&report, false).join("\n");
		assert!(markdown.contains("**Trusted publishing:** manual-action-required"));
		assert!(
			markdown.contains("**Trust message:** configure trusted publishing manually for `pkg`")
		);
		assert!(markdown.contains("**Setup:** `https://crates.io/crates/pkg`"));
		assert!(markdown.contains("**Next:** open the setup URL, configure trusted publishing for this package, then rerun `mc publish`"));
	}

	#[test]
	fn package_publish_status_labels_cover_all_variants() {
		assert_eq!(
			package_publish_status_label(package_publish::PackagePublishStatus::Planned),
			"planned"
		);
		assert_eq!(
			package_publish_status_label(package_publish::PackagePublishStatus::Published),
			"published"
		);
		assert_eq!(
			package_publish_status_label(package_publish::PackagePublishStatus::SkippedExisting),
			"skipped-existing"
		);
		assert_eq!(
			package_publish_status_label(package_publish::PackagePublishStatus::SkippedExternal),
			"skipped-external"
		);
	}

	#[test]
	fn trusted_publishing_status_labels_cover_all_variants() {
		assert_eq!(
			trusted_publishing_status_label(package_publish::TrustedPublishingStatus::Disabled),
			"disabled"
		);
		assert_eq!(
			trusted_publishing_status_label(package_publish::TrustedPublishingStatus::Planned),
			"planned"
		);
		assert_eq!(
			trusted_publishing_status_label(package_publish::TrustedPublishingStatus::Configured),
			"configured"
		);
		assert_eq!(
			trusted_publishing_status_label(
				package_publish::TrustedPublishingStatus::ManualActionRequired
			),
			"manual-action-required"
		);
	}

	#[test]
	fn resolve_command_output_supports_package_publish_json_without_release_state() {
		let cli_command = CliCommandDefinition {
			name: "placeholder-publish".to_string(),
			help_text: Some("publish placeholders".to_string()),
			inputs: vec![monochange_core::CliInputDefinition {
				name: "format".to_string(),
				kind: CliInputKind::Choice,
				help_text: Some("Output format".to_string()),
				required: false,
				default: Some("text".to_string()),
				choices: vec![
					"text".to_string(),
					"markdown".to_string(),
					"json".to_string(),
				],
				short: None,
			}],
			steps: vec![CliStepDefinition::PlaceholderPublish {
				name: Some("publish placeholder packages".to_string()),
				when: None,
				inputs: BTreeMap::new(),
			}],
		};
		let mut context = cli_context();
		context.last_step_inputs =
			BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
		context.package_publish_report = Some(package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: true,
			packages: vec![package_publish::PackagePublishOutcome {
				package: "core".to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: "crates_io".to_string(),
				version: "0.0.0".to_string(),
				status: package_publish::PackagePublishStatus::Planned,
				message: "would publish placeholder package".to_string(),
				placeholder: true,
				trusted_publishing: package_publish::TrustedPublishingOutcome {
					status: package_publish::TrustedPublishingStatus::ManualActionRequired,
					repository: None,
					workflow: None,
					environment: None,
					setup_url: Some("https://crates.io/docs/trusted-publishing".to_string()),
					message: "configure trusted publishing manually after the placeholder release"
						.to_string(),
				},
			}],
		});

		let rendered = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("package publish json output: {error}"));
		let parsed: serde_json::Value = serde_json::from_str(&rendered)
			.unwrap_or_else(|error| panic!("parse package publish json output: {error}"));
		assert_eq!(
			parsed["packagePublish"]["mode"],
			serde_json::json!("placeholder")
		);
		assert_eq!(parsed["packagePublish"]["dryRun"], serde_json::json!(true));
		assert_eq!(
			parsed["packagePublish"]["packages"][0]["package"],
			serde_json::json!("core")
		);
		assert_eq!(
			parsed["packagePublish"]["packages"][0]["trustedPublishing"]["status"],
			serde_json::json!("manual_action_required")
		);
	}

	#[test]
	fn resolve_command_output_supports_package_publish_text_and_markdown_without_release_state() {
		let cli_command = CliCommandDefinition {
			name: "placeholder-publish".to_string(),
			help_text: Some("publish placeholders".to_string()),
			inputs: vec![monochange_core::CliInputDefinition {
				name: "format".to_string(),
				kind: CliInputKind::Choice,
				help_text: Some("Output format".to_string()),
				required: false,
				default: Some("text".to_string()),
				choices: vec![
					"text".to_string(),
					"markdown".to_string(),
					"json".to_string(),
				],
				short: None,
			}],
			steps: vec![CliStepDefinition::PlaceholderPublish {
				name: Some("publish placeholder packages".to_string()),
				when: None,
				inputs: BTreeMap::new(),
			}],
		};

		let mut text_context = cli_context();
		text_context.last_step_inputs =
			BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
		text_context.package_publish_report = Some(package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: true,
			packages: Vec::new(),
		});
		let text = resolve_command_output(&cli_command, &text_context, true, None)
			.unwrap_or_else(|error| panic!("package publish text output: {error}"));
		assert!(text.contains("placeholder publishing:"));
		assert!(text.contains("no packages matched the publishing criteria"));

		let mut markdown_context = cli_context();
		markdown_context.last_step_inputs =
			BTreeMap::from([("format".to_string(), vec!["markdown".to_string()])]);
		markdown_context.package_publish_report = Some(package_publish::PackagePublishReport {
			mode: package_publish::PackagePublishRunMode::Placeholder,
			dry_run: true,
			packages: Vec::new(),
		});
		let markdown = resolve_command_output(&cli_command, &markdown_context, true, None)
			.unwrap_or_else(|error| panic!("package publish markdown output: {error}"));
		assert!(markdown.contains("## Placeholder publishing"));
		assert!(markdown.contains("no packages matched the publishing criteria"));
	}

	#[test]
	fn resolve_command_output_supports_publish_rate_limit_reports_without_release_state() {
		let cli_command = CliCommandDefinition {
			name: "publish-plan".to_string(),
			help_text: Some("plan publish rate limits".to_string()),
			inputs: vec![monochange_core::CliInputDefinition {
				name: "format".to_string(),
				kind: CliInputKind::Choice,
				help_text: Some("Output format".to_string()),
				required: false,
				default: Some("text".to_string()),
				choices: vec!["text".to_string(), "json".to_string()],
				short: None,
			}],
			steps: vec![CliStepDefinition::PlanPublishRateLimits {
				name: Some("plan publish rate limits".to_string()),
				when: None,
				inputs: BTreeMap::new(),
			}],
		};

		let mut context = cli_context();
		context.last_step_inputs =
			BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
		context.rate_limit_report = Some(sample_rate_limit_report());

		let text = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("rate limit text output: {error}"));
		assert!(text.contains("publish rate limits:"));
		assert!(text.contains("batches=2"));
		assert!(text.contains("planned batches:"));
		assert!(text.contains("wait: 86400s before this batch"));

		context.last_step_inputs =
			BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
		let json = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("rate limit json output: {error}"));
		assert!(json.contains("batchesRequired"));
		assert!(json.contains("publishRateLimits"));

		context.last_step_inputs =
			BTreeMap::from([("ci".to_string(), vec!["github-actions".to_string()])]);
		let github = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("rate limit github snippet: {error}"));
		assert!(github.contains("jobs:"));
		assert!(github.contains("wait_seconds: 86400"));
		assert!(github.contains("mc publish"));

		context.last_step_inputs =
			BTreeMap::from([("ci".to_string(), vec!["gitlab-ci".to_string()])]);
		let gitlab = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("rate limit gitlab snippet: {error}"));
		assert!(gitlab.contains("publish_batches:"));
		assert!(gitlab.contains("WAIT_SECONDS: \"86400\""));

		context.last_step_inputs =
			BTreeMap::from([("format".to_string(), vec!["text".to_string()])]);
		context.rate_limit_report = Some(monochange_core::PublishRateLimitReport {
			dry_run: true,
			windows: Vec::new(),
			batches: Vec::new(),
			warnings: Vec::new(),
		});
		let empty = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("empty rate limit output: {error}"));
		assert!(empty.contains("no publish operations matched the current plan"));

		let mut windows_without_batches = sample_rate_limit_report();
		windows_without_batches.batches.clear();
		context.rate_limit_report = Some(windows_without_batches);
		let no_batches = resolve_command_output(&cli_command, &context, true, None)
			.unwrap_or_else(|error| panic!("rate limit output without batches: {error}"));
		assert!(no_batches.contains("publish rate limits:"));
		assert!(!no_batches.contains("planned batches:"));
	}

	#[test]
	fn normalize_when_expression_preserves_inequality_and_mid_token_bangs() {
		assert_eq!(
			normalize_when_expression("{{ flag != other }}"),
			"{{ flag != other }}"
		);
		assert_eq!(normalize_when_expression("{{ foo!bar }}"), "{{ foo!bar }}");
	}

	#[test]
	fn publish_rate_limit_helpers_parse_package_filters_modes_and_ci_renderers() {
		assert_eq!(
			selected_package_ids(&BTreeMap::from([(
				"package".to_string(),
				vec!["core".to_string(), "web".to_string(), "core".to_string()],
			)])),
			BTreeSet::from(["core".to_string(), "web".to_string()])
		);
		assert_eq!(
			publish_rate_limit_mode_from_inputs(&BTreeMap::new())
				.unwrap_or_else(|error| panic!("default mode: {error}")),
			publish_rate_limits::PublishRateLimitMode::Publish
		);
		assert_eq!(
			publish_rate_limit_mode_from_inputs(&BTreeMap::from([(
				"mode".to_string(),
				vec!["placeholder".to_string()],
			)]))
			.unwrap_or_else(|error| panic!("placeholder mode: {error}")),
			publish_rate_limits::PublishRateLimitMode::Placeholder
		);
		assert_eq!(
			requested_ci_renderer(&BTreeMap::from([(
				"ci".to_string(),
				vec!["gitlab-ci".to_string()],
			)]))
			.unwrap_or_else(|error| panic!("ci renderer: {error}")),
			Some("gitlab-ci")
		);

		let mode_error = publish_rate_limit_mode_from_inputs(&BTreeMap::from([(
			"mode".to_string(),
			vec!["ship-it".to_string()],
		)]))
		.expect_err("expected invalid mode error");
		assert!(
			mode_error
				.to_string()
				.contains("unsupported publish plan mode `ship-it`")
		);

		let renderer_error = requested_ci_renderer(&BTreeMap::from([(
			"ci".to_string(),
			vec!["circleci".to_string()],
		)]))
		.expect_err("expected invalid renderer error");
		assert!(
			renderer_error
				.to_string()
				.contains("unsupported publish CI renderer `circleci`")
		);

		let snippet_error =
			render_publish_rate_limit_ci_snippet(&sample_rate_limit_report(), "circleci")
				.expect_err("expected unsupported snippet renderer error");
		assert!(
			snippet_error
				.to_string()
				.contains("unsupported publish CI renderer `circleci`")
		);
	}

	#[test]
	fn build_cli_template_context_exposes_publish_rate_limits_without_publish_results() {
		let mut context = cli_context();
		context.rate_limit_report = Some(sample_rate_limit_report());

		let template_context = build_cli_template_context(&context, &BTreeMap::new(), None);
		assert_eq!(
			template_context
				.get("publish_rate_limits")
				.and_then(serde_json::Value::as_object)
				.and_then(|value| value.get("dryRun"))
				.and_then(serde_json::Value::as_bool),
			Some(true)
		);
		assert!(!template_context.contains_key("publish"));
	}

	#[test]
	fn parse_string_as_boolean_rejects_invalid_values() {
		let error = parse_string_as_boolean("maybe", "{{ inputs.run }}").unwrap_err();
		assert_eq!(
			error.to_string(),
			"config error: `when` condition `{{ inputs.run }}` must be a boolean, got `maybe`"
		);
	}

	#[test]
	fn map_process_spawn_result_reports_io_failures() {
		let error =
			map_process_spawn_result(Err(io::Error::other("boom")), "echo hello").unwrap_err();
		assert_eq!(
			error.to_string(),
			"io error: failed to run command `echo hello`: boom"
		);
	}

	#[test]
	fn render_display_versions_output_rejects_unknown_formats() {
		let error = render_display_versions_output(
			&sample_prepared_release_with_versions(),
			&BTreeMap::from([("format".to_string(), vec!["yaml".to_string()])]),
		)
		.unwrap_err();
		assert_eq!(
			error.to_string(),
			"config error: unsupported output format `yaml`"
		);
	}

	#[test]
	fn execute_matches_uses_progress_format_from_environment_and_rejects_invalid_values() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

		temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", Some("json"), || {
			let (configuration, matches) = parse_validate_matches(tempdir.path());
			let step_matches = matches
				.subcommand_matches("step:discover")
				.unwrap_or_else(|| panic!("step:discover subcommand matches"));
			execute_matches(
				tempdir.path(),
				&configuration,
				"step:discover",
				step_matches,
				false,
			)
			.unwrap_or_else(|error| panic!("step:discover with env progress format: {error}"));
		});

		temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", Some("wat"), || {
			let (configuration, matches) = parse_validate_matches(tempdir.path());
			let step_matches = matches
				.subcommand_matches("step:discover")
				.unwrap_or_else(|| panic!("step:discover subcommand matches"));
			let error = execute_matches(
				tempdir.path(),
				&configuration,
				"step:discover",
				step_matches,
				false,
			)
			.unwrap_err();
			assert_eq!(
				error.to_string(),
				"config error: unknown progress format `wat`; expected one of: auto, unicode, ascii, json"
			);
		});
	}

	#[test]
	fn run_cli_command_command_streams_output_when_progress_is_enabled() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let mut context = cli_context();
		context.root = tempdir.path().to_path_buf();
		let step_inputs = BTreeMap::new();
		let step = CliStepDefinition::Command {
			name: Some("announce release".to_string()),
			when: None,
			command: "printf 'streamed line\\n'".to_string(),
			dry_run_command: None,
			show_progress: None,
			shell: ShellConfig::Default,
			id: Some("stream".to_string()),
			variables: None,
			inputs: BTreeMap::new(),
		};
		let cli_command = CliCommandDefinition {
			name: "release".to_string(),
			help_text: Some("release".to_string()),
			inputs: Vec::new(),
			steps: vec![step.clone()],
		};
		let mut progress =
			CliProgressReporter::new(&cli_command, false, false, ProgressFormat::Json);

		run_cli_command_command(
			&mut context,
			&step,
			0,
			&mut progress,
			true,
			CommandStepOptions {
				command: "printf 'streamed line\\n'",
				dry_run_command: None,
				shell: &ShellConfig::Default,
				step_id: Some("stream"),
				variables: None,
				step_inputs: &step_inputs,
			},
		)
		.unwrap_or_else(|error| panic!("streaming command step: {error}"));

		assert_eq!(context.command_logs, vec!["streamed line".to_string()]);
		assert_eq!(
			context
				.step_outputs
				.get("stream")
				.map(|output| output.stdout.as_str()),
			Some("streamed line")
		);
	}

	#[test]
	fn take_process_stream_reports_missing_pipes() {
		let error = take_process_stream::<Vec<u8>>(None, "stdout", "echo hello").unwrap_err();
		assert_eq!(
			error.to_string(),
			"io error: failed to capture stdout for command `echo hello`"
		);
	}

	#[test]
	fn step_shows_progress_disables_interactive_change_steps_by_default() {
		let step = CliStepDefinition::CreateChangeFile {
			show_progress: None,
			name: Some("interactive change".to_string()),
			when: None,
			inputs: BTreeMap::new(),
		};
		let mut step_inputs = BTreeMap::new();
		step_inputs.insert("interactive".to_string(), vec!["true".to_string()]);
		assert!(!step_shows_progress(&step, &step_inputs));
		step_inputs.insert("interactive".to_string(), vec!["false".to_string()]);
		assert!(step_shows_progress(&step, &step_inputs));
	}

	#[test]
	fn step_shows_progress_respects_explicit_step_flags() {
		let step = CliStepDefinition::Command {
			show_progress: Some(false),
			name: Some("interactive shell".to_string()),
			when: None,
			command: "echo hello".to_string(),
			dry_run_command: None,
			shell: ShellConfig::Default,
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		};
		assert!(!step_shows_progress(&step, &BTreeMap::new()));
	}

	#[test]
	fn drain_stream_events_collects_stdout_stderr_and_handles_closed_channels() {
		let cli_command = CliCommandDefinition {
			name: "release".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		};
		let mut progress =
			CliProgressReporter::new(&cli_command, false, false, ProgressFormat::Auto);
		let step = CliStepDefinition::Command {
			show_progress: None,
			name: Some("stream output".to_string()),
			when: None,
			command: "echo hello".to_string(),
			dry_run_command: None,
			shell: ShellConfig::Default,
			id: None,
			variables: None,
			inputs: BTreeMap::new(),
		};
		let (sender, receiver) = mpsc::channel();
		sender
			.send(StreamEvent::Chunk(
				CommandStream::Stdout,
				b"hello\n".to_vec(),
			))
			.unwrap_or_else(|error| panic!("send stdout: {error}"));
		sender
			.send(StreamEvent::Chunk(
				CommandStream::Stderr,
				b"warn\n".to_vec(),
			))
			.unwrap_or_else(|error| panic!("send stderr: {error}"));
		sender
			.send(StreamEvent::Closed(CommandStream::Stdout))
			.unwrap_or_else(|error| panic!("close stdout: {error}"));
		sender
			.send(StreamEvent::Closed(CommandStream::Stderr))
			.unwrap_or_else(|error| panic!("close stderr: {error}"));
		drop(sender);
		let (stdout, stderr) = drain_stream_events(&receiver, &mut progress, 0, &step);
		assert_eq!(stdout, b"hello\n");
		assert_eq!(stderr, b"warn\n");

		let (sender, receiver) = mpsc::channel();
		drop(sender);
		let (stdout, stderr) = drain_stream_events(&receiver, &mut progress, 0, &step);
		assert!(stdout.is_empty());
		assert!(stderr.is_empty());
	}

	#[test]
	fn map_process_wait_result_reports_io_failures() {
		let error = map_process_wait_result(Err(io::Error::other("wait failed")), "echo hello")
			.unwrap_err();
		assert_eq!(
			error.to_string(),
			"io error: failed to wait for command `echo hello`: wait failed"
		);
	}

	#[test]
	fn execute_cli_command_captures_telemetry_when_step_input_resolution_fails() {
		let _guard = TEST_ENV_LOCK
			.lock()
			.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let telemetry_path = tempdir.path().join("telemetry-input-error.jsonl");
		let telemetry_path_value = telemetry_path.to_string_lossy().to_string();
		let configuration = sample_configuration(tempdir.path());
		let cli_command = CliCommandDefinition {
			name: "telemetry-input-error".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: vec![CliStepDefinition::Validate {
				name: Some("invalid input".to_string()),
				when: None,
				inputs: BTreeMap::from([(
					"target".to_string(),
					CliStepInputValue::String("{{".to_string()),
				)]),
			}],
		};

		temp_env::with_vars(
			[
				("MC_TELEMETRY", None::<&str>),
				("MC_TELEMETRY_FILE", Some(telemetry_path_value.as_str())),
			],
			|| {
				let error = execute_cli_command(
					tempdir.path(),
					&configuration,
					&cli_command,
					true,
					BTreeMap::new(),
				)
				.unwrap_err();
				assert!(matches!(error, MonochangeError::Config(_)));
			},
		);

		let events = read_telemetry_events(&telemetry_path);
		assert_eq!(events.len(), 2);
		assert_eq!(events[0]["body"]["string_value"], "command_step");
		assert_eq!(events[0]["attributes"]["outcome"], "error");
		assert_eq!(events[0]["attributes"]["error_kind"], "config_error");
		assert_eq!(events[1]["body"]["string_value"], "command_run");
		assert_eq!(events[1]["attributes"]["outcome"], "error");
		assert_eq!(events[1]["attributes"]["error_kind"], "config_error");
	}

	#[test]
	fn execute_cli_command_captures_telemetry_when_step_condition_fails() {
		let _guard = TEST_ENV_LOCK
			.lock()
			.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let telemetry_path = tempdir.path().join("telemetry-condition-error.jsonl");
		let telemetry_path_value = telemetry_path.to_string_lossy().to_string();
		let configuration = sample_configuration(tempdir.path());
		let cli_command = CliCommandDefinition {
			name: "telemetry-condition-error".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: vec![CliStepDefinition::Validate {
				name: Some("invalid condition".to_string()),
				when: Some("{{ missing.path }}".to_string()),
				inputs: BTreeMap::new(),
			}],
		};

		temp_env::with_vars(
			[
				("MC_TELEMETRY", None::<&str>),
				("MC_TELEMETRY_FILE", Some(telemetry_path_value.as_str())),
			],
			|| {
				let error = execute_cli_command(
					tempdir.path(),
					&configuration,
					&cli_command,
					true,
					BTreeMap::new(),
				)
				.unwrap_err();
				assert!(matches!(error, MonochangeError::Config(_)));
			},
		);

		let events = read_telemetry_events(&telemetry_path);
		assert_eq!(events.len(), 2);
		assert_eq!(events[0]["body"]["string_value"], "command_step");
		assert_eq!(events[0]["attributes"]["outcome"], "error");
		assert_eq!(events[0]["attributes"]["error_kind"], "config_error");
		assert_eq!(events[1]["body"]["string_value"], "command_run");
		assert_eq!(events[1]["attributes"]["outcome"], "error");
		assert_eq!(events[1]["attributes"]["error_kind"], "config_error");
	}

	#[test]
	fn execute_cli_command_reports_command_failures_after_progress_callbacks() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let cli_command = CliCommandDefinition {
			name: "fail".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: vec![CliStepDefinition::Command {
				show_progress: None,
				name: Some("fail loud".to_string()),
				when: None,
				command: "printf 'boom\\n' >&2; exit 3".to_string(),
				dry_run_command: None,
				shell: ShellConfig::Default,
				id: None,
				variables: None,
				inputs: BTreeMap::new(),
			}],
		};

		let configuration = monochange_core::WorkspaceConfiguration {
			root_path: tempdir.path().to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults::default(),
			changelog: ChangelogSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			lints: monochange_core::lint::WorkspaceLintSettings::default(),
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
		};
		let error = execute_cli_command(
			tempdir.path(),
			&configuration,
			&cli_command,
			false,
			BTreeMap::new(),
		)
		.unwrap_err();
		assert_eq!(
			error.to_string(),
			"discovery error: command `printf 'boom\\n' >&2; exit 3` failed: boom"
		);
	}

	#[test]
	fn build_release_template_value_serializes_file_diffs() {
		let mut context = cli_context();
		context.prepared_release = Some(PreparedRelease {
			plan: ReleasePlan {
				workspace_root: PathBuf::from("."),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: vec!["core".to_string()],
			version: Some("1.2.3".to_string()),
			group_version: None,
			release_targets: Vec::new(),
			changed_files: vec![PathBuf::from("Cargo.toml")],
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			package_publications: Vec::new(),
			dry_run: true,
		});
		context.prepared_file_diffs = vec![PreparedFileDiff {
			path: PathBuf::from("Cargo.toml"),
			diff: "-old\n+new".to_string(),
			display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
		}];

		let manifest = build_release_template_value(&context);

		let file_diffs = manifest
			.get("file_diffs")
			.and_then(serde_json::Value::as_array)
			.unwrap_or_else(|| panic!("release template should include file_diffs"));
		assert_eq!(file_diffs.len(), 1);
		assert_eq!(file_diffs[0]["path"], serde_json::json!("Cargo.toml"));
		assert_eq!(file_diffs[0]["diff"], serde_json::json!("-old\n+new"));
	}

	#[test]
	fn execute_cli_command_with_options_covers_final_artifact_save_call() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let cli_command = CliCommandDefinition {
			name: "noop".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		};

		let output = execute_cli_command_with_options(
			tempdir.path(),
			&sample_configuration(tempdir.path()),
			&cli_command,
			ExecuteCliCommandOptions {
				dry_run: false,
				quiet: true,
				show_diff: false,
				inputs: BTreeMap::new(),
				prepared_release_path: None,
				progress_format: ProgressFormat::Auto,
			},
		)
		.unwrap_or_else(|error| panic!("execute noop command: {error}"));

		assert_eq!(output, "command `noop` completed");
	}

	#[test]
	fn execute_cli_command_with_options_reuses_prepared_release_artifact_for_versions() {
		let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
			.unwrap_or_else(|error| panic!("workspace root: {error}"));
		let configuration = sample_configuration(&root);
		let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let artifact_path = artifact_dir.path().join("prepared-release.json");
		save_prepared_release_execution(
			&root,
			&configuration,
			&sample_prepared_release_with_versions(),
			&[],
			Some(artifact_path.as_path()),
		)
		.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

		let output = execute_cli_command_with_options(
			&root,
			&configuration,
			&default_cli_command("display-versions"),
			ExecuteCliCommandOptions {
				dry_run: false,
				quiet: false,
				show_diff: false,
				inputs: BTreeMap::from([("format".to_string(), vec!["json".to_string()])]),
				prepared_release_path: Some(artifact_path),
				progress_format: ProgressFormat::Auto,
			},
		)
		.unwrap_or_else(|error| panic!("execute versions command: {error}"));
		let parsed: serde_json::Value = serde_json::from_str(&output)
			.unwrap_or_else(|error| panic!("parse versions output: {error}"));

		assert_eq!(parsed["groups"]["sdk"], serde_json::json!("2.0.0"));
		assert_eq!(parsed["packages"]["core"], serde_json::json!("1.2.0"));
		assert_eq!(parsed["packages"]["web"], serde_json::json!("1.2.1"));
	}

	#[test]
	fn execute_cli_command_with_options_reports_invalid_versions_artifacts() {
		let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
			.unwrap_or_else(|error| panic!("workspace root: {error}"));
		let configuration = sample_configuration(&root);
		let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let invalid_artifact_path = artifact_dir.path().join("prepared-release.json");
		fs::write(&invalid_artifact_path, "not json")
			.unwrap_or_else(|error| panic!("write invalid artifact: {error}"));

		let error = execute_cli_command_with_options(
			&root,
			&configuration,
			&default_cli_command("display-versions"),
			ExecuteCliCommandOptions {
				dry_run: false,
				quiet: false,
				show_diff: false,
				inputs: BTreeMap::new(),
				prepared_release_path: Some(invalid_artifact_path),
				progress_format: ProgressFormat::Auto,
			},
		)
		.expect_err("invalid prepared release artifact should fail");

		assert!(
			error
				.to_string()
				.contains("failed to parse prepared release artifact")
		);
	}

	#[test]
	fn execute_cli_command_with_options_reports_invalid_versions_output_formats() {
		let root = fs::canonicalize(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."))
			.unwrap_or_else(|error| panic!("workspace root: {error}"));
		let configuration = sample_configuration(&root);
		let artifact_dir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let artifact_path = artifact_dir.path().join("prepared-release.json");
		save_prepared_release_execution(
			&root,
			&configuration,
			&sample_prepared_release_with_versions(),
			&[],
			Some(artifact_path.as_path()),
		)
		.unwrap_or_else(|error| panic!("save prepared release artifact: {error}"));

		let error = execute_cli_command_with_options(
			&root,
			&configuration,
			&default_cli_command("display-versions"),
			ExecuteCliCommandOptions {
				dry_run: false,
				quiet: false,
				show_diff: false,
				inputs: BTreeMap::from([("format".to_string(), vec!["yaml".to_string()])]),
				prepared_release_path: Some(artifact_path),
				progress_format: ProgressFormat::Auto,
			},
		)
		.expect_err("unsupported versions output format should fail");

		assert_eq!(
			error.to_string(),
			"config error: unsupported output format `yaml`"
		);
	}

	#[test]
	fn record_skipped_and_failure_helpers_cover_silent_paths() {
		let cli_command = default_cli_command("validate");
		let step = CliStepDefinition::Validate {
			name: Some("validate".to_string()),
			when: None,
			inputs: BTreeMap::new(),
		};
		let mut context = cli_context();
		let mut progress =
			CliProgressReporter::new(&cli_command, false, true, ProgressFormat::Auto);

		record_skipped_cli_step(&mut context, &step, 0, &mut progress, false);
		report_cli_step_failure(
			&mut progress,
			false,
			0,
			&step,
			Duration::from_millis(1),
			&MonochangeError::Config("boom".to_string()),
		);

		assert!(context.command_logs.is_empty());
	}

	#[test]
	fn render_cli_command_result_includes_release_results_and_changed_files() {
		let cli_command = default_cli_command("prepare-release");
		let mut context = cli_context();
		let mut prepared_release = sample_prepared_release();
		prepared_release.changed_files = vec![PathBuf::from("Cargo.toml")];
		context.prepared_release = Some(prepared_release);
		context.release_manifest_path = Some(PathBuf::from(".monochange/prepared-release.json"));
		context.release_results = vec!["published core".to_string()];

		let rendered = render_cli_command_result(&cli_command, &context);

		assert!(rendered.contains("release manifest: .monochange/prepared-release.json"));
		assert!(rendered.contains("releases:"));
		assert!(rendered.contains("- published core"));
		assert!(rendered.contains("changed files:"));
		assert!(rendered.contains("- Cargo.toml"));
	}

	#[test]
	fn save_prepared_release_artifact_returns_explicit_errors() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let mut context = cli_context();
		context.prepared_release = Some(sample_prepared_release());

		let error = save_prepared_release_artifact(
			tempdir.path(),
			&sample_configuration(tempdir.path()),
			&context,
			Some(tempdir.path().join("prepared-release.json").as_path()),
		)
		.err()
		.unwrap_or_else(|| panic!("expected explicit artifact save error"));

		assert!(!error.to_string().is_empty());
	}

	#[test]
	fn append_changed_file_lines_returns_early_when_no_files_changed() {
		let mut lines = vec!["start".to_string()];
		append_changed_file_lines(&mut lines, &[]);
		assert_eq!(lines, vec!["start".to_string()]);
	}

	#[test]
	fn step_references_release_file_diffs_detects_all_supported_locations() {
		let from_when = CliStepDefinition::Validate {
			name: Some("validate".to_string()),
			when: Some("{{ file_diffs }}".to_string()),
			inputs: BTreeMap::new(),
		};
		assert!(step_references_release_file_diffs(&from_when));

		let mut inputs = BTreeMap::new();
		inputs.insert(
			"paths".to_string(),
			CliStepInputValue::List(vec!["{{ file_diffs }}".to_string()]),
		);
		let from_inputs = CliStepDefinition::PublishRelease {
			name: Some("publish".to_string()),
			when: None,
			inputs,
		};
		assert!(step_references_release_file_diffs(&from_inputs));

		let from_variables = CliStepDefinition::Command {
			name: Some("command".to_string()),
			when: None,
			command: "echo done".to_string(),
			dry_run_command: None,
			show_progress: None,
			shell: ShellConfig::Default,
			id: None,
			variables: Some(BTreeMap::from([(
				"file_diffs_payload".to_string(),
				CommandVariable::ChangedFiles,
			)])),
			inputs: BTreeMap::new(),
		};
		assert!(step_references_release_file_diffs(&from_variables));

		let without_file_diffs = CliStepDefinition::Command {
			name: Some("command".to_string()),
			when: None,
			command: "echo done".to_string(),
			dry_run_command: None,
			show_progress: None,
			shell: ShellConfig::Default,
			id: None,
			variables: None,
			inputs: BTreeMap::from([("confirmed".to_string(), CliStepInputValue::Boolean(true))]),
		};
		assert!(!step_references_release_file_diffs(&without_file_diffs));
	}

	#[test]
	fn render_cli_command_result_and_markdown_cover_empty_and_fallback_paths() {
		let cli_command = default_cli_command("prepare-release");
		let mut context = cli_context();
		context.command_logs = vec!["ran command".to_string()];
		let text = render_cli_command_result(&cli_command, &context);
		assert!(text.contains("commands:"));
		assert!(!text.contains("changed files:"));

		let markdown = render_cli_command_markdown_result(&cli_command, &context);
		assert_eq!(markdown, text);
	}

	#[test]
	fn render_cli_command_result_and_markdown_include_release_target_details_without_diffs() {
		let cli_command = default_cli_command("prepare-release");
		let mut context = cli_context();
		context.prepared_release = Some(PreparedRelease {
			plan: ReleasePlan {
				workspace_root: PathBuf::from("."),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			},
			changeset_paths: Vec::new(),
			changesets: Vec::new(),
			released_packages: vec!["core".to_string(), "utils".to_string()],
			version: Some("1.2.3".to_string()),
			group_version: None,
			release_targets: vec![ReleaseTarget {
				id: "core".to_string(),
				kind: ReleaseOwnerKind::Package,
				version: "1.2.3".to_string(),
				tag: true,
				release: false,
				version_format: VersionFormat::Primary,
				tag_name: "v1.2.3".to_string(),
				members: Vec::new(),
				rendered_title: "core v1.2.3".to_string(),
				rendered_changelog_title: "core v1.2.3".to_string(),
			}],
			changed_files: vec![PathBuf::from("Cargo.toml")],
			changelogs: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			package_publications: Vec::new(),
			dry_run: true,
		});
		context.changeset_policy_evaluation = Some(ChangesetPolicyEvaluation {
			enforce: false,
			required: true,
			status: ChangesetPolicyStatus::Skipped,
			summary: "skip label matched".to_string(),
			comment: None,
			labels: vec!["docs-only".to_string()],
			matched_skip_labels: vec!["docs-only".to_string()],
			changed_paths: vec!["docs/readme.md".to_string()],
			matched_paths: Vec::new(),
			ignored_paths: Vec::new(),
			changeset_paths: Vec::new(),
			affected_package_ids: Vec::new(),
			covered_package_ids: Vec::new(),
			uncovered_package_ids: Vec::new(),
			errors: Vec::new(),
		});

		let text = render_cli_command_result(&cli_command, &context);
		assert!(text.contains("release targets:"));
		assert!(text.contains("tag: true, release: false"));
		assert!(text.contains("changed files:"));
		assert!(!text.contains("file diffs:"));
		assert!(text.contains("matched skip labels: docs-only"));

		let markdown = render_cli_command_markdown_result(&cli_command, &context);
		assert!(markdown.contains("## Release targets"));
		assert!(markdown.contains("tag: yes"));
		assert!(markdown.contains("release: no"));
		assert!(markdown.contains("## Changed files"));
		assert!(!markdown.contains("## Commands"));
	}

	#[test]
	fn markdown_painting_covers_title_subtitle_and_muted_styles() {
		assert!(paint_markdown_inline("title", MarkdownStyle::Title, true).contains("[36;1m"));
		assert!(
			paint_markdown_inline("subtitle", MarkdownStyle::Subtitle, true).contains("[37;1m")
		);
		assert!(paint_markdown_inline("muted", MarkdownStyle::Muted, true).contains("[2m"));

		let _env_lock = TEST_ENV_LOCK
			.lock()
			.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
		temp_env::with_vars(
			[("NO_COLOR", Some("1")), ("TERM", Some("xterm-256color"))],
			|| {
				assert!(!stdout_supports_color());
			},
		);
	}
}
