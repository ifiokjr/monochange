use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use clap::Arg;
use clap::ArgAction;
use clap::ColorChoice;
use clap::Command;
use monochange_config::load_workspace_configuration;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::default_cli_commands;

/// Build the top-level Clap command for the `monochange` binary.
///
/// The returned command includes built-in subcommands such as `init`, `subagents`,
/// `mcp`, and `release-record`, plus any config-defined commands resolved from
/// the current working directory.
pub fn build_command(bin_name: &'static str) -> Command {
	let root = current_dir_or_dot();
	build_command_for_root(bin_name, &root)
}

pub(crate) fn configured_change_type_choices(
	configuration: &monochange_core::WorkspaceConfiguration,
) -> Vec<String> {
	configuration
		.changelog
		.types
		.keys()
		.cloned()
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

	let Some(change_command) = cli.iter_mut().find(|command| command.name == "change") else {
		return;
	};
	let Some(change_type_input) = change_command
		.inputs
		.iter_mut()
		.find(|input| input.name == "type" && input.choices.is_empty())
	else {
		return;
	};

	change_type_input.kind = CliInputKind::Choice;
	change_type_input.choices = choices;
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
	let Ok(configuration) = configuration else {
		return default_cli_commands();
	};

	let mut cli = configuration.cli.clone();
	apply_runtime_change_type_choices(&mut cli, configuration);
	apply_runtime_prepare_release_markdown_defaults(&mut cli);

	cli
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

		let has_markdown = format_input
			.choices
			.iter()
			.any(|choice| choice == "markdown");
		if !has_markdown {
			format_input.choices.insert(0, "markdown".to_string());
		}

		let has_md = format_input.choices.iter().any(|choice| choice == "md");
		if !has_md {
			format_input.choices.push("md".to_string());
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

/// Color theme for monochange CLI help output.
fn monochange_styles() -> clap::builder::Styles {
	clap::builder::Styles::styled()
		.header(crate::cli_theme::header())
		.usage(crate::cli_theme::usage())
		.literal(crate::cli_theme::literal())
		.placeholder(crate::cli_theme::placeholder())
		.error(crate::cli_theme::error())
		.valid(crate::cli_theme::valid())
		.invalid(crate::cli_theme::error())
}

pub(crate) fn build_command_with_cli(
	bin_name: &'static str,
	cli: &[CliCommandDefinition],
) -> Command {
	let mut command =
		Command::new(bin_name)
			.about("Manage versions and releases for your multiplatform, multilanguage monorepo")
			.styles(monochange_styles())
			.color(ColorChoice::Auto)
			.disable_help_subcommand(true)
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
							.long_help(
								"Configure release automation for the specified provider. \
When provided, the generated config includes:\n\
\n\
- [source] section with the provider configured\n\
- Release and pull request settings for the provider\n\
- CLI commands for commit-release and release-pr\n\
- GitHub Actions workflows (for --provider=github)\n\
\nSupported providers: github, gitlab, gitea",
							)
							.value_parser(["github", "gitlab", "gitea"]),
					),
			)
			.subcommand(Command::new("populate").about(
				"Add any missing built-in CLI commands to monochange.toml so you can customize them",
			))
			.subcommand(build_skill_subcommand())
			.subcommand(build_subagents_subcommand())
			.subcommand(build_analyze_subcommand())
			.subcommand(build_release_record_subcommand())
			.subcommand(build_tag_release_subcommand())
			.subcommand(build_merge_release_pr_subcommand())
			.subcommand(build_lint_subcommand())
			.subcommand(Command::new("mcp").about(
				"Start the monochange MCP (Model Context Protocol) server over stdin/stdout",
			))
			.subcommand(build_check_subcommand())
			.subcommand(build_help_subcommand());

	for cli_command in cli {
		command = command.subcommand(build_cli_command_subcommand(cli_command));
	}

	command
}

pub(crate) fn build_skill_subcommand() -> Command {
	Command::new("skill")
		.about("Install the monochange skill bundle into the current project with the skills CLI")
		.after_help(
			r"Examples:
  mc help skill
  mc skill
  mc skill --list
  mc skill -a claude-code -a codex
  mc skill --skill monochange --copy -y
  mc skill -g -a pi -y

This command forwards all remaining arguments to:
  skills add <monochange-source>

Common forwarded flags from the upstream `skills add` command include:
  -g, --global            install to the user-level agent directories
  -a, --agent <AGENT>     target specific agent harnesses
  -s, --skill <SKILL>     install specific skills from the source
  -l, --list              list the available skills without installing
      --copy              copy files instead of symlinking
  -y, --yes               skip confirmation prompts
      --all               install all skills to all supported agents

Runner selection is automatic. monochange prefers:
  1. npx
  2. pnpm dlx
  3. bunx",
		)
		.arg(
			Arg::new("args")
				.help("Arguments forwarded to `skills add` after the monochange skill source")
				.num_args(0..)
				.action(ArgAction::Append)
				.trailing_var_arg(true)
				.allow_hyphen_values(true),
		)
}

pub(crate) fn build_subagents_subcommand() -> Command {
	Command::new("subagents")
		.about("Generate repo-local monochange subagents and agent guidance files")
		.after_help(
			r"Examples:
  mc help subagents
  mc subagents claude
  mc subagents pi codex
  mc subagents --all --dry-run --format json
  mc subagents vscode copilot --no-mcp

Targets:
  - claude  -> .claude/agents/*.md and .mcp.json
  - vscode  -> .github/agents/*.agent.md and .vscode/mcp.json
  - copilot -> .github/agents/*.agent.md and .vscode/mcp.json
  - pi      -> .pi/agents/*.md
  - codex   -> .codex/agents/*.toml
  - cursor  -> .cursor/rules/*.mdc

Generated agents are CLI-first. They should prefer:
  1. mc
  2. monochange
  3. npx -y @monochange/cli

Use `--no-mcp` to skip MCP config files for targets that support repo-local MCP config.",
		)
		.arg(
			Arg::new("target")
				.help("Subagent target(s) to generate")
				.value_name("TARGET")
				.num_args(1..)
				.required_unless_present("all")
				.value_parser(["claude", "vscode", "copilot", "pi", "codex", "cursor"]),
		)
		.arg(
			Arg::new("all")
				.long("all")
				.help("Generate files for all supported targets")
				.conflicts_with("target")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("force")
				.long("force")
				.help("Overwrite generated files that already exist with different contents")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Preview the generated files without writing them")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format for the generated subagent plan")
				.default_value("markdown")
				.value_parser(["text", "json", "markdown", "md"]),
		)
		.arg(
			Arg::new("no-mcp")
				.long("no-mcp")
				.help("Skip repo-local MCP config files for supported targets")
				.action(ArgAction::SetTrue),
		)
}

pub(crate) fn build_analyze_subcommand() -> Command {
	Command::new("analyze")
		.about("Analyze semantic changes for one package across main, head, and optional release baselines")
		.after_help(
			r"Examples:
  mc analyze --package core
  mc analyze --package core --format json
  mc analyze --package core --release-ref core/v1.2.3
  mc analyze --package core --main-ref main --head-ref HEAD

Analysis notes:
  - Runs package-scoped semantic analysis using the selected package's configured release identity.
  - Defaults `--release-ref` to the newest tag for the package or the version group that owns it.
  - If no prior release tag exists, falls back to first-release analysis using only `main -> head`.",
		)
		.arg(
			Arg::new("package")
				.long("package")
				.required(true)
				.value_name("PACKAGE")
				.help("Configured package id, discovered package id, package name, manifest path, or package directory"),
		)
		.arg(
			Arg::new("release-ref")
				.long("release-ref")
				.value_name("REF")
				.help("Explicit release baseline ref. Defaults to the latest tag for the package or owning version group"),
		)
		.arg(
			Arg::new("main-ref")
				.long("main-ref")
				.value_name("REF")
				.help("Base branch or ref to compare against. Defaults to the detected default branch"),
		)
		.arg(
			Arg::new("head-ref")
				.long("head-ref")
				.value_name("REF")
				.help("Head ref to analyze. Defaults to HEAD"),
		)
		.arg(
			Arg::new("detection-level")
				.long("detection-level")
				.default_value("signature")
				.value_parser(["basic", "signature", "semantic"])
				.help("Level of semantic detail to request from analyzers"),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.default_value("markdown")
				.value_parser(["text", "json", "markdown", "md"])
				.help("Output format"),
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
				.default_value("markdown")
				.value_parser(["text", "json", "markdown", "md"]),
		)
}

pub(crate) fn build_tag_release_subcommand() -> Command {
	Command::new("tag-release")
		.about("Create and push release tags from the monochange release record on a commit")
		.after_help(
			r"Examples:
  mc tag-release --from HEAD
  mc tag-release --from HEAD --dry-run --format json
  mc tag-release --from HEAD --push=false

Tagging notes:
  - Resolves the supplied ref to a commit and requires that commit itself to be the release commit.
  - Creates the full tag set declared by the embedded monochange release record.
  - Pushes tags to `origin` by default and treats reruns on the same commit as already up to date.",
		)
		.arg(
			Arg::new("from")
				.long("from")
				.required(true)
				.value_name("REF")
				.help("Release commit ref used to create the declared tag set"),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Preview release tag creation without mutating local or remote refs")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("push")
				.long("push")
				.help("Push created tags to origin")
				.value_name("PUSH")
				.num_args(0..=1)
				.default_value("true")
				.default_missing_value("true")
				.require_equals(true)
				.value_parser(["true", "false"]),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format")
				.default_value("markdown")
				.value_parser(["text", "json", "markdown", "md"]),
		)
}

pub(crate) fn build_merge_release_pr_subcommand() -> Command {
	Command::new("merge-release-pr")
		.about("Merge a release pull request and publish releases")
		.after_help(
			r"Examples:
  mc merge-release-pr --pr-number 42 --author githubuser
  mc merge-release-pr --pr-number 42 --dry-run --format json

Process:
  1. Authorize the triggering user against the configured slash-command policy.
  2. Squash-merge the PR using the provider API with a computed release commit message.
  3. Fetch origin so tags point to the API-created merge commit.
  4. Create and push release tags, then publish provider releases.",
		)
		.arg(
			Arg::new("pr-number")
				.long("pr-number")
				.required(true)
				.value_name("NUMBER")
				.help("The pull request number to merge")
				.value_parser(clap::value_parser!(u64)),
		)
		.arg(
			Arg::new("author")
				.long("author")
				.value_name("USER")
				.help("The user triggering the merge (for authorization)"),
		)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Preview merge without executing")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format")
				.default_value("markdown")
				.value_parser(["text", "json", "markdown", "md"]),
		)
}

pub(crate) fn build_check_subcommand() -> Command {
	Command::new("check")
		.about("Validate configuration, changesets, and run manifest lint rules")
		.after_help(
			"Examples:\n  mc check\n  mc check --fix\n  mc check --ecosystem cargo,npm\n  mc check --only cargo/sorted-dependencies\n\n\
			 Lint rules are configured in the top-level [lints] section of monochange.toml:\n\n\
			 [lints]\n  use = [\"cargo/recommended\", \"npm/recommended\"]\n\n\
			 [lints.rules]\n  \"cargo/internal-dependency-workspace\" = \"error\"",
		)
		.arg(
			Arg::new("fix")
				.long("fix")
				.short('f')
				.help("Automatically fix lint issues where possible")
				.action(ArgAction::SetTrue),
		)
		.arg(
			Arg::new("ecosystem")
				.long("ecosystem")
				.short('e')
				.help("Limit linting to specific lint suites")
				.value_name("ECOSYSTEMS")
				.value_delimiter(','),
		)
		.arg(
			Arg::new("only")
				.long("only")
				.help("Run only the specified lint rule ids")
				.value_name("RULES")
				.value_delimiter(','),
		)
		.arg(
			Arg::new("format")
				.long("format")
				.help("Output format")
				.default_value("markdown")
				.value_parser(["text", "json", "markdown", "md"]),
		)
}

pub(crate) fn build_lint_subcommand() -> Command {
	Command::new("lint")
		.about("Inspect and scaffold manifest lint rules")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("list")
				.about("List registered lint rules and presets")
				.arg(
					Arg::new("format")
						.long("format")
						.help("Output format")
						.default_value("markdown")
						.value_parser(["text", "json", "markdown", "md"]),
				),
		)
		.subcommand(
			Command::new("explain")
				.about("Explain a lint rule or preset")
				.arg(
					Arg::new("id")
						.required(true)
						.help("Lint rule id or preset id to explain"),
				)
				.arg(
					Arg::new("format")
						.long("format")
						.help("Output format")
						.default_value("markdown")
						.value_parser(["text", "json", "markdown", "md"]),
				),
		)
		.subcommand(
			Command::new("new")
				.about("Scaffold a new lint rule in an ecosystem crate")
				.after_help(
					"Examples:\n  mc lint new cargo/no-path-dependencies\n  mc lint new npm/require-package-manager",
				)
				.arg(
					Arg::new("id")
						.required(true)
						.help("New lint id in the form <ecosystem>/<rule-name>"),
				),
		)
}

pub(crate) fn build_help_subcommand() -> Command {
	Command::new("help")
		.about("Show detailed help for a command")
		.long_about(
			"Show detailed help, examples, and tips for any monochange command. \
			 Run `mc help` to list all commands, or `mc help <command>` for \
			 detailed usage information with examples.",
		)
		.arg(
			Arg::new("command")
				.help("Command name to get help for (e.g. change, release, init)")
				.value_name("COMMAND"),
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
	let help_text = cli_command
		.help_text
		.clone()
		.unwrap_or_else(|| format!("Run the `{}` command", cli_command.name));

	let mut command = Command::new(leak_string(cli_command.name.clone()))
		.about(help_text)
		.arg(
			Arg::new("dry-run")
				.long("dry-run")
				.help("Run the command in dry-run mode when supported")
				.action(ArgAction::SetTrue),
		);

	if command_supports_release_diff_preview(cli_command) {
		command = command
			.arg(
				Arg::new("diff")
					.long("diff")
					.help("Show unified file diffs for prepared release changes")
					.action(ArgAction::SetTrue),
			)
			.arg(
				Arg::new("prepared-release")
					.long("prepared-release")
					.help("Read or write the prepared release artifact at a specific path")
					.value_name("PATH"),
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
  mc change --package sdk-config --bump none --caused-by sdk-core --reason "dependency-only follow-up"

Rules:
  - Prefer configured package ids in change files whenever a leaf package changed.
  - Use a group id only when the change is intentionally owned by the whole group.
  - Dependents and grouped members are propagated automatically during planning.
  - Use `--caused-by` when a package is only changing because another package or group moved first.
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
		"versions" => {
			Some(
				r"Examples:
  mc versions
  mc versions --format markdown
  mc versions --format json

Summary notes:
  - This command is read-only and does not update manifests or changelogs.
  - It computes the same planned versions used by monochange release workflows.",
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
		"tag-release" => {
			Some(
				r"Examples:
  mc tag-release --from HEAD
  mc tag-release --from HEAD --dry-run --format json
  mc tag-release --from HEAD --push=false

Tagging notes:
  - Requires the resolved ref itself to be the monochange release commit.
  - Creates the full tag set declared by that release record.
  - Treats reruns on the same commit as already up to date.
  - Use `mc repair-release` if you need to move existing tags later.",
			)
		}
		_ => None,
	}
}

fn build_cli_command_input_arg(input: &CliInputDefinition) -> Arg {
	let long_name = leak_string(input.name.replace('_', "-"));
	let value_name = leak_string(input.name.to_uppercase());
	let help_text = input.help_text.clone().unwrap_or_default();

	let mut arg = Arg::new(leak_string(input.name.clone()))
		.long(long_name)
		.required(input.required)
		.help(help_text);

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
			let possible_values: Vec<_> = input.choices.iter().cloned().map(leak_string).collect();

			arg.value_name(value_name)
				.value_parser(clap::builder::PossibleValuesParser::new(possible_values))
		}
	};

	if let Some(short) = input.short {
		arg = arg.short(short);
	}

	let should_apply_default = input
		.default
		.as_ref()
		.is_some_and(|default| !matches!(input.kind, CliInputKind::Boolean) || default == "true");

	if should_apply_default {
		arg = arg.default_value(leak_string(input.default.clone().unwrap()));
	}

	arg
}

/// Intentionally leaks a string to obtain a `&'static str` for clap arguments.
///
/// This is acceptable only in CLI binaries where the process lifetime is short
/// and the leaked strings are never freed. Do not use this in library code.
fn leak_string(value: impl Into<String>) -> &'static str {
	Box::leak(value.into().into_boxed_str())
}

pub(crate) fn current_dir_or_dot() -> PathBuf {
	std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}
