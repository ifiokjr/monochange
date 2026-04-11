use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use clap::Arg;
use clap::ArgAction;
use clap::Command;
use monochange_config::load_workspace_configuration;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::default_cli_commands;

pub fn build_command(bin_name: &'static str) -> Command {
	let root = current_dir_or_dot();
	build_command_for_root(bin_name, &root)
}

pub(crate) fn configured_change_type_choices(
	configuration: &monochange_core::WorkspaceConfiguration,
) -> Vec<String> {
	configuration
		.packages
		.iter()
		.flat_map(|package| package.extra_changelog_sections.iter())
		.chain(
			configuration
				.groups
				.iter()
				.flat_map(|group| group.extra_changelog_sections.iter()),
		)
		.flat_map(|section| section.types.iter())
		.map(|value| value.trim().to_string())
		.filter(|value| !value.is_empty())
		.collect::<BTreeSet<_>>()
		.into_iter()
		.collect()
}

pub(crate) fn apply_runtime_change_type_choices(
	cli: &mut [CliCommandDefinition],
	configuration: &monochange_core::WorkspaceConfiguration,
) {
	let choices = configured_change_type_choices(configuration);
	if choices.is_empty() {
		return;
	}
	if let Some(change_command) = cli.iter_mut().find(|command| command.name == "change")
		&& let Some(change_type_input) = change_command
			.inputs
			.iter_mut()
			.find(|input| input.name == "type" && input.choices.is_empty())
	{
		change_type_input.kind = CliInputKind::Choice;
		change_type_input.choices = choices;
	}
}

pub(crate) fn cli_commands_for_root(root: &Path) -> Vec<CliCommandDefinition> {
	let configuration = load_workspace_configuration(root);
	cli_commands_from_config(&configuration)
}

/// Extract CLI commands from an already-loaded configuration result, avoiding
/// a redundant config load when the caller has already parsed the config.
pub(crate) fn cli_commands_from_config(
	configuration: &Result<
		monochange_core::WorkspaceConfiguration,
		monochange_core::MonochangeError,
	>,
) -> Vec<CliCommandDefinition> {
	match configuration {
		Ok(configuration) => {
			let mut cli = configuration.cli.clone();
			apply_runtime_change_type_choices(&mut cli, configuration);
			apply_runtime_prepare_release_markdown_defaults(&mut cli);
			cli
		}
		Err(_) => default_cli_commands(),
	}
}

pub(crate) fn apply_runtime_prepare_release_markdown_defaults(cli: &mut [CliCommandDefinition]) {
	for cli_command in cli {
		if !command_supports_release_diff_preview(cli_command) {
			continue;
		}
		let Some(format_input) = cli_command
			.inputs
			.iter_mut()
			.find(|input| input.name == "format")
		else {
			continue;
		};
		if !format_input
			.choices
			.iter()
			.any(|choice| choice == "markdown")
		{
			format_input.choices.insert(0, "markdown".to_string());
		}
		if format_input.default.as_deref() == Some("text") {
			format_input.default = Some("markdown".to_string());
		}
	}
}

pub(crate) fn build_command_for_root(bin_name: &'static str, root: &Path) -> Command {
	let cli = cli_commands_for_root(root);
	build_command_with_cli(bin_name, &cli)
}

pub(crate) fn build_command_with_cli(
	bin_name: &'static str,
	cli: &[CliCommandDefinition],
) -> Command {
	let mut command =
		Command::new(bin_name)
			.about("Manage versions and releases for your multiplatform, multilanguage monorepo")
			.subcommand_required(true)
			.arg_required_else_help(true)
			.arg(
				Arg::new("log-level")
					.long("log-level")
					.global(true)
					.help("Set tracing filter (e.g. debug, monochange=trace)")
					.value_name("FILTER")
					.hide(true),
			)
			.arg(
				Arg::new("quiet")
					.long("quiet")
					.short('q')
					.global(true)
					.help("Suppress stdout/stderr output and run in dry-run mode when supported")
					.action(ArgAction::SetTrue),
			)
			.arg(
				Arg::new("progress-format")
					.long("progress-format")
					.global(true)
					.help("Control progress output on stderr")
					.value_name("FORMAT")
					.value_parser(["auto", "unicode", "ascii", "json"]),
			)
			.subcommand(
				Command::new("init")
					.about(
						"Generate monochange.toml with detected packages, groups, and default CLI commands",
					)
					.arg(
						Arg::new("force")
							.long("force")
							.help("Overwrite an existing monochange.toml file")
							.action(ArgAction::SetTrue),
					)
					.arg(
						Arg::new("provider")
							.long("provider")
							.help("Source-control provider for release automation workflows")
							.value_parser(["github", "gitlab", "gitea"]),
					),
			)
			.subcommand(Command::new("populate").about(
				"Add any missing built-in CLI commands to monochange.toml so you can customize them",
			))
			.subcommand(build_assist_subcommand())
			.subcommand(build_release_record_subcommand())
			.subcommand(Command::new("mcp").about(
				"Start the monochange MCP (Model Context Protocol) server over stdin/stdout",
			));

	for cli_command in cli {
		command = command.subcommand(build_cli_command_subcommand(cli_command));
	}

	command
}

pub(crate) fn build_assist_subcommand() -> Command {
	Command::new("assist")
		.about("Print assistant setup guidance, install steps, and MCP configuration")
		.arg(
			Arg::new("assistant")
				.help("Assistant profile to print")
				.required(true)
				.value_parser(["generic", "claude", "cursor", "copilot", "pi"]),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format for the assistant setup profile")
				.default_value("text")
				.value_parser(["text", "json"]),
		)
}

pub(crate) fn build_release_record_subcommand() -> Command {
	Command::new("release-record")
		.about("Inspect the monochange release record associated with a tag or commit")
		.after_help(
			r"Examples:
  mc release-record --from v1.2.3
  mc release-record --from HEAD --format json

Inspection notes:
  - Resolves the supplied ref to a commit.
  - Walks first-parent ancestry until it finds a monochange release record.
  - Fails loudly if it encounters a malformed release record block on the path.",
		)
		.arg(
			Arg::new("from")
				.long("from")
				.required(true)
				.value_name("REF")
				.help("Tag or commit-ish used to locate the release record"),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format")
				.default_value("text")
				.value_parser(["text", "json"]),
		)
}

pub(crate) fn command_supports_release_diff_preview(cli_command: &CliCommandDefinition) -> bool {
	cli_command.steps.iter().any(|step| {
		matches!(
			step,
			monochange_core::CliStepDefinition::PrepareRelease { .. }
		)
	})
}

pub(crate) fn build_cli_command_subcommand(cli_command: &CliCommandDefinition) -> Command {
	let mut command = Command::new(leak_string(cli_command.name.clone()))
		.about(
			cli_command
				.help_text
				.clone()
				.unwrap_or_else(|| format!("Run the `{}` command", cli_command.name)),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Run the command in dry-run mode when supported")
				.action(ArgAction::SetTrue),
		);

	if command_supports_release_diff_preview(cli_command) {
		command = command.arg(
			Arg::new("diff")
				.long("diff")
				.help("Show unified file diffs for prepared release changes")
				.action(ArgAction::SetTrue),
		);
	}

	if let Some(after_help) = cli_command_after_help(cli_command) {
		command = command.after_help(after_help);
	}

	for input in &cli_command.inputs {
		command = command.arg(build_cli_command_input_arg(input));
	}

	command
}

pub(crate) fn cli_command_after_help(cli_command: &CliCommandDefinition) -> Option<&'static str> {
	match cli_command.name.as_str() {
		"change" => {
			Some(
				r#"Examples:
  mc change --package sdk-core --bump patch --reason "fix panic"
  mc change --package sdk-core --bump minor --reason "add API" --output .changeset/sdk-core.md
  mc change --package sdk --bump minor --reason "coordinated release"

Rules:
  - Prefer configured package ids in change files whenever a leaf package changed.
  - Use a group id only when the change is intentionally owned by the whole group.
  - Dependents and grouped members are propagated automatically during planning.
  - Legacy manifest paths may still resolve during migration, but declared ids are the stable interface."#,
			)
		}
		"release" => {
			Some(
				r"Examples:
  mc release --dry-run --format text
  mc release --dry-run --format json
  mc release --dry-run --diff
  mc release

Planning reminders:
  - Direct package changes propagate to dependents using defaults.parent_bump.
  - Group synchronization happens before final output is rendered.
  - Explicit versions on grouped members propagate to the whole group.",
			)
		}
		"commit-release" => {
			Some(
				r"Examples:
  mc commit-release --dry-run --format json
  mc commit-release --dry-run --diff
  mc commit-release

Commit notes:
  - Reuses the standard monochange release commit subject/body contract.
  - Embeds a durable release record block in the commit body.
  - Can run before OpenReleaseRequest in the same workflow.",
			)
		}
		"affected" => {
			Some(
				r"Examples:
  mc affected --changed-paths crates/core/src/lib.rs --format json
  mc affected --since origin/main --verify

Verification reminders:
  - Prefer package ids in .changeset files.
  - Group-owned changesets cover all members of that group.
  - Ignored paths and skip labels are controlled from [changesets.verify].",
			)
		}
		"diagnostics" => {
			Some(
				r"Examples:
  mc diagnostics --format json
  mc diagnostics --changeset .changeset/feature.md

Diagnostics include:
  - Target packages/groups and requested bump
  - commit SHA that introduced and last updated each changeset
  - linked review request (when detected)
  - related issue references",
			)
		}
		"repair-release" => {
			Some(
				r"Examples:
  mc repair-release --from v1.2.3 --dry-run
  mc repair-release --from v1.2.3 --target HEAD --format json

Repair notes:
  - Finds the release record from history using the supplied ref.
  - Moves the full release tag set together.
  - Defaults to descendant-only retargets unless --force is set.
  - Hosted release sync runs by default and can be disabled with --sync-provider=false.",
			)
		}
		_ => None,
	}
}

fn build_cli_command_input_arg(input: &CliInputDefinition) -> Arg {
	let long_name = leak_string(input.name.replace('_', "-"));
	let value_name = leak_string(input.name.to_uppercase());
	let mut arg = Arg::new(leak_string(input.name.clone()))
		.long(long_name)
		.required(input.required)
		.help(input.help_text.clone().unwrap_or_default());

	arg = match input.kind {
		CliInputKind::String => arg.value_name(value_name),
		CliInputKind::StringList => arg.value_name(value_name).action(ArgAction::Append),
		CliInputKind::Path => arg.value_name("PATH"),
		CliInputKind::Boolean => {
			if input.default.as_deref() == Some("true") {
				arg.value_name(value_name)
					.num_args(0..=1)
					.default_missing_value("true")
					.require_equals(true)
					.value_parser(["true", "false"])
			} else {
				arg.action(ArgAction::SetTrue)
			}
		}
		CliInputKind::Choice => {
			arg.value_name(value_name)
				.value_parser(clap::builder::PossibleValuesParser::new(
					input
						.choices
						.iter()
						.cloned()
						.map(leak_string)
						.collect::<Vec<_>>(),
				))
		}
	};

	if let Some(short) = input.short {
		arg = arg.short(short);
	}

	if let Some(default) = &input.default
		&& (!matches!(input.kind, CliInputKind::Boolean) || default == "true")
	{
		arg = arg.default_value(leak_string(default.clone()));
	}

	arg
}

fn leak_string(value: impl Into<String>) -> &'static str {
	Box::leak(value.into().into_boxed_str())
}

pub(crate) fn current_dir_or_dot() -> PathBuf {
	std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
