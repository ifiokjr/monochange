use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command as ProcessCommand;

use clap::ArgMatches;
use clap::parser::ValueSource;
use monochange_config::load_workspace_configuration;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::CommandVariable;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ShellConfig;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;

use crate::cli::command_supports_release_diff_preview;
use crate::*;

pub(crate) fn execute_matches(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command_name: &str,
	cli_command_matches: &ArgMatches,
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

	let dry_run = cli_command_matches.get_flag("dry-run");
	let show_diff =
		command_supports_release_diff_preview(cli_command) && cli_command_matches.get_flag("diff");
	let inputs = collect_cli_command_inputs(cli_command, cli_command_matches);
	if show_diff {
		execute_cli_command_with_options(root, configuration, cli_command, dry_run, true, inputs)
	} else {
		execute_cli_command(root, configuration, cli_command, dry_run, inputs)
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

pub(crate) fn execute_cli_command(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command: &CliCommandDefinition,
	dry_run: bool,
	inputs: BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	execute_cli_command_with_options(root, configuration, cli_command, dry_run, false, inputs)
}

#[tracing::instrument(skip_all, fields(command = cli_command.name))]
pub(crate) fn execute_cli_command_with_options(
	root: &Path,
	configuration: &monochange_core::WorkspaceConfiguration,
	cli_command: &CliCommandDefinition,
	dry_run: bool,
	show_diff: bool,
	inputs: BTreeMap<String, Vec<String>>,
) -> MonochangeResult<String> {
	let mut context = CliContext {
		root: root.to_path_buf(),
		dry_run,
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

	for step in &cli_command.steps {
		let step_inputs = resolve_step_inputs(&context, step)?;
		context.last_step_inputs = step_inputs.clone();

		if !should_execute_cli_step(step, &context, &step_inputs)? {
			if let Some(condition) = step.when() {
				tracing::debug!(step = step.kind_name(), condition = %condition, "skipped CLI step");
				context.command_logs.push(format!(
					"skipped step `{}` because when condition `{condition}` is false",
					step.kind_name()
				));
			}
			continue;
		}

		tracing::debug!(step = step.kind_name(), "executing CLI step");
		match step {
			CliStepDefinition::Validate { .. } => {
				validate_workspace(root)?;
				validate_cargo_workspace_version_groups(root)?;
				let warnings = validate_versioned_files_content(root)?;
				for warning in &warnings {
					eprintln!("warning: {warning}");
				}
				output = Some(format!(
					"workspace validation passed for {}",
					root_relative(root, root).display()
				));
			}
			CliStepDefinition::Discover { .. } => {
				let format = step_inputs
					.get("format")
					.and_then(|values| values.first())
					.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
				output = Some(render_discovery_report(&discover_workspace(root)?, format)?);
			}
			CliStepDefinition::CreateChangeFile { .. } => {
				let is_interactive = step_inputs
					.get("interactive")
					.and_then(|values| values.first())
					.is_some_and(|value| value == "true");

				if is_interactive {
					let configuration = load_workspace_configuration(root)?;
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
					let result = interactive::run_interactive_change(&configuration, &options)?;
					let output_path = step_inputs
						.get("output")
						.and_then(|values| values.first())
						.map(PathBuf::from);
					let path = add_interactive_change_file(root, &result, output_path.as_deref())?;
					output = Some(format!(
						"wrote change file {}",
						root_relative(root, &path).display()
					));
				} else {
					let package_refs = step_inputs.get("package").cloned().unwrap_or_default();
					if package_refs.is_empty() {
						return Err(MonochangeError::Config(
							"command `change` requires at least one `--package` value or `--interactive` mode".to_string(),
						));
					}
					let bump = if let Some(value) =
						step_inputs.get("bump").and_then(|values| values.first())
					{
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
							MonochangeError::Config(
								"command `change` requires a `--reason` value".to_string(),
							)
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
					output = Some(format!(
						"wrote change file {}",
						root_relative(root, &path).display()
					));
				}
			}
			CliStepDefinition::PrepareRelease { .. } => {
				let prepared_execution = prepare_release_execution(root, dry_run)?;
				context.prepared_file_diffs = prepared_execution.file_diffs;
				context.prepared_release = Some(prepared_execution.prepared_release);
				output = None;
			}
			CliStepDefinition::RenderReleaseManifest { path, .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`RenderReleaseManifest` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				if let Some(path) = path {
					let resolved_path = resolve_config_path(root, path);
					let rendered = render_release_manifest_json(&manifest)?;
					apply_file_updates(&[FileUpdate {
						path: resolved_path.clone(),
						content: rendered.into_bytes(),
					}])?;
					context.release_manifest_path = Some(root_relative(root, &resolved_path));
				}
				output = None;
			}
			CliStepDefinition::PublishRelease { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`PublishRelease` requires a previous `PrepareRelease` step".to_string(),
					)
				})?;
				let source = load_workspace_configuration(root)?.source.ok_or_else(|| {
					MonochangeError::Config(
						"`PublishRelease` requires `[source]` configuration".to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				context.release_requests = build_source_release_requests(&source, &manifest);
				if context.dry_run {
					context.release_results = context
						.release_requests
						.iter()
						.map(|request| {
							format!(
								"dry-run {} {} ({}) via {}",
								request.repository,
								request.tag_name,
								request.name,
								request.provider
							)
						})
						.collect();
				} else {
					context.release_results =
						publish_source_release_requests(&source, &context.release_requests)?
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
							.collect();
				}
				output = None;
			}
			CliStepDefinition::CommitRelease { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`CommitRelease` requires a previous `PrepareRelease` step".to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				#[rustfmt::skip]
				let release_commit_report = commit_release(root, &context, configuration.source.as_ref(), &manifest)?;
				context.release_commit_report = Some(release_commit_report);
				output = None;
			}
			CliStepDefinition::OpenReleaseRequest { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`OpenReleaseRequest` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let source = load_workspace_configuration(root)?.source.ok_or_else(|| {
					MonochangeError::Config(
						"`OpenReleaseRequest` requires `[source]` configuration".to_string(),
					)
				})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				let request = build_source_change_request(&source, &manifest);
				if context.dry_run {
					context.release_request_result = Some(format!(
						"dry-run {} {} -> {} via {}",
						request.repository,
						request.head_branch,
						request.base_branch,
						request.provider
					));
				} else {
					let tracked_paths = tracked_release_pull_request_paths(&context, &manifest);
					let result =
						publish_source_change_request(&source, root, &request, &tracked_paths)?;
					context.release_request_result = Some(format!(
						"{} #{} ({}) via {}",
						result.repository,
						result.number,
						format_change_request_operation(&result.operation),
						result.provider
					));
				}
				context.release_request = Some(request);
				output = None;
			}
			CliStepDefinition::CommentReleasedIssues { .. } => {
				let prepared_release = context.prepared_release.as_ref().ok_or_else(|| {
					MonochangeError::Config(
						"`CommentReleasedIssues` requires a previous `PrepareRelease` step"
							.to_string(),
					)
				})?;
				let source = load_workspace_configuration(root)?
					.source
					.filter(|source| source.provider == SourceProvider::GitHub)
					.ok_or_else(|| {
						MonochangeError::Config(
							"`CommentReleasedIssues` requires `[source].provider = \"github\"` configuration"
								.to_string(),
						)
					})?;
				let manifest =
					build_release_manifest(cli_command, prepared_release, &context.command_logs);
				context.issue_comment_plans =
					github_provider::plan_released_issue_comments(&source, &manifest);
				if context.dry_run {
					context.issue_comment_results = context
						.issue_comment_plans
						.iter()
						.map(|plan| format!("dry-run {} {}", plan.repository, plan.issue_id))
						.collect();
				} else {
					context.issue_comment_results =
						github_provider::comment_released_issues(&source, &manifest)?
							.into_iter()
							.map(|result| {
								format!(
									"{} {} ({})",
									result.repository,
									result.issue_id,
									match result.operation {
										monochange_github::GitHubIssueCommentOperation::Created => "created",
										monochange_github::GitHubIssueCommentOperation::SkippedExisting => {
											"skipped_existing"
										}
									}
								)
							})
							.collect();
				}
				output = None;
			}
			CliStepDefinition::AffectedPackages { .. } => {
				let since = step_inputs
					.get("since")
					.and_then(|values| values.first().cloned());
				let explicit_paths = step_inputs
					.get("changed_paths")
					.cloned()
					.unwrap_or_default();
				let changed_paths = if let Some(rev) = &since {
					if !explicit_paths.is_empty() {
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
				context.changeset_policy_evaluation = Some(evaluation);
				output = None;
			}
			CliStepDefinition::DiagnoseChangesets { .. } => {
				let requested = step_inputs.get("changeset").cloned().unwrap_or_default();
				let report = diagnose_changesets(root, &requested)?;
				context.changeset_diagnostics = Some(report);
				output = None;
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
					command,
					dry_run_command.as_deref(),
					shell,
					id.as_deref(),
					variables.as_ref(),
					&step_inputs,
				)?;
			}
		}
	}

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
			OutputFormat::Text => Ok(render_cli_command_result(cli_command, &context)),
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
			OutputFormat::Text => render_cli_command_result(cli_command, &context),
		};
		if evaluation.enforce && evaluation.status == ChangesetPolicyStatus::Failed {
			println!("{rendered}");
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
			OutputFormat::Text => render_changeset_diagnostics(report),
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
			OutputFormat::Text => render_retarget_release_report(report),
		};
		return Ok(rendered);
	}
	if !context.command_logs.is_empty() {
		return Ok(render_cli_command_result(cli_command, &context));
	}

	Ok(output.unwrap_or_else(|| {
		format!(
			"command `{}` completed{}",
			cli_command.name,
			if dry_run { " (dry-run)" } else { "" }
		)
	}))
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

fn run_cli_command_command(
	context: &mut CliContext,
	command: &str,
	dry_run_command: Option<&str>,
	shell: &ShellConfig,
	step_id: Option<&str>,
	variables: Option<&BTreeMap<String, CommandVariable>>,
	step_inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<()> {
	let command_to_run = if context.dry_run {
		if let Some(command) = dry_run_command {
			command
		} else {
			let skipped = interpolate_cli_command_command(context, command, variables, step_inputs);
			context
				.command_logs
				.push(format!("skipped command `{skipped}` (dry-run)"));
			return Ok(());
		}
	} else {
		command
	};
	let interpolated =
		interpolate_cli_command_command(context, command_to_run, variables, step_inputs);

	let output = if let Some(shell_binary) = shell.shell_binary() {
		ProcessCommand::new(shell_binary)
			.arg("-c")
			.arg(&interpolated)
			.current_dir(&context.root)
			.output()
	} else {
		let parts = shlex::split(&interpolated).ok_or_else(|| {
			MonochangeError::Config(format!("failed to parse command `{interpolated}`"))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(
				"command must not be empty".to_string(),
			));
		};
		ProcessCommand::new(program)
			.args(args)
			.current_dir(&context.root)
			.output()
	};
	let output = output.map_err(|error| {
		MonochangeError::Io(format!("failed to run command `{interpolated}`: {error}"))
	})?;
	if !output.status.success() {
		let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
		let details = if stderr.is_empty() {
			format!("exit status {}", output.status)
		} else {
			stderr
		};
		return Err(MonochangeError::Discovery(format!(
			"command `{interpolated}` failed: {details}"
		)));
	}

	let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
	let stderr_text = String::from_utf8_lossy(&output.stderr).trim().to_string();

	if let Some(id) = step_id {
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
	lines.join(
		"
",
	)
}

fn cli_command_output_format(
	inputs: &BTreeMap<String, Vec<String>>,
) -> MonochangeResult<OutputFormat> {
	inputs
		.get("format")
		.and_then(|values| values.first())
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))
}

pub(crate) fn parse_output_format(value: &str) -> MonochangeResult<OutputFormat> {
	match value {
		"text" => Ok(OutputFormat::Text),
		"json" => Ok(OutputFormat::Json),
		other => {
			Err(MonochangeError::Config(format!(
				"unsupported output format `{other}`"
			)))
		}
	}
}

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

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::path::PathBuf;

	use super::*;

	fn cli_context() -> CliContext {
		CliContext {
			root: PathBuf::from("."),
			dry_run: false,
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
}
