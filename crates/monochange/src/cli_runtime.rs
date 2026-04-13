use std::collections::BTreeMap;
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
	let Some(cli_command) = configuration
		.cli
		.iter()
		.find(|cli_command| cli_command.name == cli_command_name)
	else {
		return Err(MonochangeError::Config(format!(
			"unknown command `{cli_command_name}`"
		)));
	};

	let dry_run = quiet || cli_command_matches.get_flag("dry-run");
	let show_diff =
		command_supports_release_diff_preview(cli_command) && cli_command_matches.get_flag("diff");
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
	let prepared_release_path = command_supports_release_diff_preview(cli_command)
		.then(|| cli_command_matches.get_one::<String>("prepared-release"))
		.flatten()
		.map(PathBuf::from);
	let inputs = collect_cli_command_inputs(cli_command, cli_command_matches);
	if show_diff {
		execute_cli_command_with_options(
			root,
			configuration,
			cli_command,
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
			cli_command,
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

pub(crate) fn write_release_manifest_file(
	root: &Path,
	path: &Path,
	manifest: &ReleaseManifest,
) -> MonochangeResult<PathBuf> {
	let resolved_path = resolve_config_path(root, path);
	let rendered = render_release_manifest_json(manifest)?;
	let update = FileUpdate {
		path: resolved_path.clone(),
		content: rendered.into_bytes(),
	};
	apply_file_updates(&[update])?;
	Ok(root_relative(root, &resolved_path))
}

fn resolve_release_manifest_path(
	root: &Path,
	path: Option<&Path>,
	manifest: &ReleaseManifest,
) -> MonochangeResult<Option<PathBuf>> {
	path.map(|path| write_release_manifest_file(root, path, manifest))
		.transpose()
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
) -> MonochangeResult<String> {
	#[rustfmt::skip]
	let result = build_release_request_result(dry_run, request, || publish_source_change_request(source, root, request, tracked_paths));
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

	for (step_index, step) in cli_command.steps.iter().enumerate() {
		let step_started_at = Instant::now();
		let step_inputs = resolve_step_inputs(&context, step)?;
		context.last_step_inputs = step_inputs.clone();
		let show_progress = step_shows_progress(step, &step_inputs);

		if !should_execute_cli_step(step, &context, &step_inputs)? {
			if show_progress {
				progress.step_skipped(step_index, step, step.when());
			}
			if let Some(condition) = step.when() {
				tracing::debug!(step = step.kind_name(), condition = %condition, "skipped CLI step");
				context.command_logs.push(format!(
					"skipped step `{}` because when condition `{condition}` is false",
					step.display_name()
				));
			}
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
						.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
					output = Some(render_discovery_report(&discover_workspace(root)?, format)?);
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
					output = None;
					Ok(())
				}
				CliStepDefinition::RenderReleaseManifest { path, .. } => {
					let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
						MonochangeError::Config(
							"`RenderReleaseManifest` requires a previous `PrepareRelease` step"
								.to_string(),
						)
					})?;
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					context.release_manifest_path =
						resolve_release_manifest_path(root, path.as_deref(), &manifest)?;
					output = None;
					Ok(())
				}
				CliStepDefinition::PublishRelease { .. } => {
					let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
						MonochangeError::Config(
							"`PublishRelease` requires a previous `PrepareRelease` step"
								.to_string(),
						)
					})?;
					let source = configuration.source.clone().ok_or_else(|| {
						MonochangeError::Config(
							"`PublishRelease` requires `[source]` configuration".to_string(),
						)
					})?;
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					context.release_requests = build_source_release_requests(&source, &manifest);
					#[rustfmt::skip]
						let results = build_release_results_for_source(context.dry_run, &source, &context.release_requests)?;
					context.release_results = results;
					output = None;
					Ok(())
				}
				CliStepDefinition::CommitRelease { .. } => {
					let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
						MonochangeError::Config(
							"`CommitRelease` requires a previous `PrepareRelease` step".to_string(),
						)
					})?;
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					#[rustfmt::skip]
				let release_commit_report = commit_release(root, &context, configuration.source.as_ref(), &manifest)?;
					context.release_commit_report = Some(release_commit_report);
					output = None;
					Ok(())
				}
				CliStepDefinition::OpenReleaseRequest { .. } => {
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
					#[rustfmt::skip]
						let result = build_release_request_result_for_source(dry_run, &source, root, &request, &tracked_paths)?;
					context.release_request_result = Some(result);
					context.release_request = Some(request);
					output = None;
					Ok(())
				}
				CliStepDefinition::CommentReleasedIssues { .. } => {
					let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
						MonochangeError::Config(
							"`CommentReleasedIssues` requires a previous `PrepareRelease` step"
								.to_string(),
						)
					})?;
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
					let manifest = build_release_manifest(
						cli_command,
						prepared_release,
						&context.command_logs,
					);
					context.issue_comment_plans =
						adapter.plan_released_issue_comments(&source, &manifest);
					let dry_run = context.dry_run;
					let plans = &context.issue_comment_plans;
					let results =
						build_issue_comment_results_for_source(dry_run, &source, &manifest, plans)?;
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
			if show_progress {
				let progress_error = progress_error_detail(&error).to_string();
				progress.step_failed(step_index, step, step_started_at.elapsed(), &progress_error);
			}
			return Err(error);
		}
		if show_progress {
			progress.step_finished(
				step_index,
				step,
				step_started_at.elapsed(),
				&step_phase_timings,
			);
		}
	}

	progress.command_finished(command_started_at.elapsed());

	if let Some(prepared_release) = &context.prepared_release {
		if let Err(error) = save_prepared_release_execution(
			root,
			configuration,
			prepared_release,
			&context.prepared_file_diffs,
			prepared_release_path.as_deref(),
		) {
			if prepared_release_path.is_some() {
				return Err(error);
			}
			tracing::warn!(%error, "failed to save prepared release artifact");
		}
	}

	resolve_command_output(cli_command, &context, dry_run, output)
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
		CliStepDefinition::RenderReleaseManifest { path, .. } => {
			path.as_ref()
				.and_then(|path| path.to_str())
				.is_some_and(mentions_file_diffs)
		}
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
	let command_to_run = if context.dry_run {
		if let Some(command) = options.dry_run_command {
			command
		} else {
			let skipped = interpolate_cli_command_command(
				context,
				options.command,
				options.variables,
				options.step_inputs,
			);
			context
				.command_logs
				.push(format!("skipped command `{skipped}` (dry-run)"));
			return Ok(());
		}
	} else {
		options.command
	};
	let interpolated = interpolate_cli_command_command(
		context,
		command_to_run,
		options.variables,
		options.step_inputs,
	);
	let mut process_command = if let Some(shell_binary) = options.shell.shell_binary() {
		let mut process_command = ProcessCommand::new(shell_binary);
		process_command.arg("-c").arg(&interpolated);
		process_command
	} else {
		let parts = shlex::split(&interpolated).ok_or_else(|| {
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
	process_command.current_dir(&context.root);

	let output = if progress.is_enabled() && show_progress {
		let streamed_output = run_process_with_streaming(
			&mut process_command,
			progress,
			step_index,
			step,
			&interpolated,
		);
		streamed_output?
	} else {
		let output = process_command.output().map_err(|error| {
			MonochangeError::Io(format!("failed to run command `{interpolated}`: {error}"))
		})?;
		PreparedProcessOutput {
			status: output.status,
			stdout: output.stdout,
			stderr: output.stderr,
		}
	};
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let details = if stderr.is_empty() {
			format!("exit status {}", output.status)
		} else {
			stderr
		};
		let rendered_command = render_command_for_error(&interpolated);
		return Err(MonochangeError::Discovery(format!(
			"command `{rendered_command}` failed: {details}"
		)));
	}

	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let stderr_text = String::from_utf8_lossy(&output.stderr).trim().to_string();

	if let Some(id) = options.step_id {
		context.step_outputs.insert(
			id.to_string(),
			CommandStepOutput {
				stdout: stdout.clone(),
				stderr: stderr_text,
			},
		);
	}

	if stdout.is_empty() {
		context.command_logs.push(format!("ran `{interpolated}`"));
	} else {
		context.command_logs.push(stdout);
	}
	Ok(())
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

	let input_context = cli_inputs_template_value(inputs);
	for (input_name, input_value) in &input_context {
		template_context.insert(input_name.clone(), input_value.clone());
	}
	template_context.insert(
		"inputs".to_string(),
		serde_json::Value::Object(input_context),
	);

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
	if let Some(commit) = &report.commit {
		lines.push(format!("  commit: {}", short_commit_sha(commit)));
	}
	lines.extend((!report.tracked_paths.is_empty()).then_some("  tracked paths:".to_string()));
	#[rustfmt::skip]
	lines.extend(report.tracked_paths.iter().map(|path| format!("    - {}", path.display())));
	lines.push(format!("  status: {}", report.status.replace('_', "-")));
	lines
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
		if !prepared_release.changed_files.is_empty() {
			lines.push("changed files:".to_string());
			for path in &prepared_release.changed_files {
				lines.push(format!("- {}", path.display()));
			}
		}
		if context.show_diff && !context.prepared_file_diffs.is_empty() {
			lines.push("file diffs:".to_string());
			for (index, file_diff) in context.prepared_file_diffs.iter().enumerate() {
				if index > 0 {
					lines.push(String::new());
				}
				lines.push(file_diff.display_diff.clone());
			}
		}
		if !prepared_release.deleted_changesets.is_empty() {
			lines.push("deleted changesets:".to_string());
			for path in &prepared_release.deleted_changesets {
				lines.push(format!("- {}", path.display()));
			}
		}
	}
	if let Some(evaluation) = &context.changeset_policy_evaluation {
		lines.push(format!("changeset policy: {}", evaluation.status));
		lines.push(evaluation.summary.clone());
		if !evaluation.matched_skip_labels.is_empty() {
			lines.push(format!(
				"matched skip labels: {}",
				evaluation.matched_skip_labels.join(", ")
			));
		}
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

pub(crate) fn render_cli_command_markdown_result(
	cli_command: &CliCommandDefinition,
	context: &CliContext,
) -> String {
	if context.prepared_release.is_none() {
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
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))
}

#[must_use = "the output format result must be checked"]
pub(crate) fn parse_output_format(value: &str) -> MonochangeResult<OutputFormat> {
	match value {
		"text" => Ok(OutputFormat::Text),
		"markdown" => Ok(OutputFormat::Markdown),
		"json" => Ok(OutputFormat::Json),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported output format `{other}`"
			)))
		}
	}
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
	let changed_paths = if let Some(rev) = &since {
		if !quiet && !explicit_paths.is_empty() {
			eprintln!("warning: --since takes priority; --changed-paths was ignored");
		}
		compute_changed_paths_since(root, rev)?
	} else {
		explicit_paths
	};
	let labels = step_inputs.get("label").cloned().unwrap_or_default();
	let enforce = step_inputs
		.get("verify")
		.is_some_and(|values| values.iter().any(|v| v == "true"));
	let mut evaluation = affected_packages(root, &changed_paths, &labels)?;
	evaluation.enforce = enforce;
	Ok(evaluation)
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
					&context.release_requests,
					context.release_request.as_ref(),
					&context.issue_comment_plans,
					context.release_commit_report.as_ref(),
					if context.show_diff {
						&context.prepared_file_diffs
					} else {
						&[]
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
			OutputFormat::Json => {
				serde_json::to_string_pretty(evaluation).map_err(|error| {
					MonochangeError::Config(format!(
						"failed to render changeset policy evaluation as json: {error}"
					))
				})?
			}
			OutputFormat::Markdown | OutputFormat::Text => {
				render_cli_command_result(cli_command, context)
			}
		};
		if evaluation.enforce && evaluation.status == ChangesetPolicyStatus::Failed {
			if !context.quiet {
				println!("{rendered}");
			}
			return Err(MonochangeError::Config(evaluation.summary.clone()));
		}
		return Ok(rendered);
	}
	if let Some(report) = &context.changeset_diagnostics {
		let format = context
			.inputs
			.get("format")
			.and_then(|values| values.first())
			.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
		let rendered = match format {
			OutputFormat::Json => {
				serde_json::to_string_pretty(report).map_err(|error| {
					MonochangeError::Config(format!(
						"failed to render changeset diagnostics as json: {error}"
					))
				})?
			}
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
	use std::io;
	use std::path::PathBuf;
	use std::sync::mpsc;

	use monochange_config::load_workspace_configuration;
	use monochange_core::CliCommandDefinition;
	use monochange_core::CliStepDefinition;
	use monochange_core::ShellConfig;
	use tempfile::tempdir;

	use super::*;

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
			issue_comment_plans: Vec::new(),
			issue_comment_results: Vec::new(),
			changeset_policy_evaluation: None,
			changeset_diagnostics: None,
			retarget_report: None,
			step_outputs: BTreeMap::new(),
			command_logs: Vec::new(),
		}
	}

	fn parse_validate_matches(
		root: &Path,
	) -> (monochange_core::WorkspaceConfiguration, ArgMatches) {
		let configuration = load_workspace_configuration(root)
			.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
		let matches = build_command_with_cli("mc", &configuration.cli)
			.try_get_matches_from(["mc", "validate"])
			.unwrap_or_else(|error| panic!("validate matches: {error}"));
		(configuration, matches)
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
	fn normalize_when_expression_preserves_inequality_and_mid_token_bangs() {
		assert_eq!(
			normalize_when_expression("{{ flag != other }}"),
			"{{ flag != other }}"
		);
		assert_eq!(normalize_when_expression("{{ foo!bar }}"), "{{ foo!bar }}");
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
	fn execute_matches_uses_progress_format_from_environment_and_rejects_invalid_values() {
		let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));

		temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", Some("json"), || {
			let (configuration, matches) = parse_validate_matches(tempdir.path());
			let validate_matches = matches
				.subcommand_matches("validate")
				.unwrap_or_else(|| panic!("validate subcommand matches"));
			execute_matches(
				tempdir.path(),
				&configuration,
				"validate",
				validate_matches,
				false,
			)
			.unwrap_or_else(|error| panic!("validate with env progress format: {error}"));
		});

		temp_env::with_var("MONOCHANGE_PROGRESS_FORMAT", Some("wat"), || {
			let (configuration, matches) = parse_validate_matches(tempdir.path());
			let validate_matches = matches
				.subcommand_matches("validate")
				.unwrap_or_else(|| panic!("validate subcommand matches"));
			let error = execute_matches(
				tempdir.path(),
				&configuration,
				"validate",
				validate_matches,
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
			release_notes: monochange_core::ReleaseNotesSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
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
}
