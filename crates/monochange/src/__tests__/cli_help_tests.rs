use super::*;

#[test]
fn paint_returns_plain_when_no_color_env_set() {
	// paint() is tested without color because tests don't run in a TTY
	assert_eq!(paint("hello", accent()), "hello");
}

#[test]
fn color_enabled_impl_all_combinations() {
	// no_color=true → false regardless of other inputs
	assert!(!color_enabled_impl(true, false, true));
	assert!(!color_enabled_impl(true, false, false));
	assert!(!color_enabled_impl(true, true, true));
	assert!(!color_enabled_impl(true, true, false));
	// term_dumb=true, no_color=false → false
	assert!(!color_enabled_impl(false, true, true));
	assert!(!color_enabled_impl(false, true, false));
	// both false, is_terminal=true → true
	assert!(color_enabled_impl(false, false, true));
	// both false, is_terminal=false → false
	assert!(!color_enabled_impl(false, false, false));
}

#[test]
fn paint_impl_color_on_and_off() {
	let style = accent();
	// When enabled, ANSI codes are present
	let colored = paint_impl("hello", style, true);
	assert!(colored.contains('\u{1b}'));
	// When disabled, plain text is returned
	let plain = paint_impl("hello", style, false);
	assert_eq!(plain, "hello");
}

#[test]
fn render_single_command_help_minimal() {
	let help = CommandHelp {
		name: "minimal",
		summary: "A minimal command.",
		description: "A minimal command description.",
		usage: "mc minimal",
		options: &[],
		examples: &[],
		tips: &[],
		see_also: &[],
	};
	let out = render_single_command_help("mc", &help);
	assert!(out.contains("minimal"));
	assert!(out.contains("A minimal command description."));
	assert!(!out.contains("Examples"));
	assert!(!out.contains("Tips"));
	assert!(!out.contains("See Also"));
	assert!(!out.contains("Options"));
}

#[test]
fn render_single_command_help_with_options() {
	let help = CommandHelp {
		name: "test",
		summary: "A test command.",
		description: "A test command.",
		usage: "mc test [OPTIONS]",
		options: &[
			("-f", "STRING", "A flag with type"),
			("-v", "", "A bare flag"),
		],
		examples: &[("Do it:", "mc test -f x")],
		tips: &["Be careful."],
		see_also: &["mc help test"],
	};
	let out = render_single_command_help("mc", &help);
	assert!(out.contains("A test command."));
	assert!(out.contains("-f"));
	assert!(out.contains("STRING"));
	assert!(out.contains("-v"));
	assert!(out.contains("Do it:"));
	assert!(out.contains("Be careful."));
	assert!(out.contains("help"));
}

#[test]
fn render_unknown_command_help_skips_matched_name() {
	let helps = vec![
		CommandListItem {
			name: "change".to_string(),
			summary: "Create a change file".to_string(),
		},
		CommandListItem {
			name: "release".to_string(),
			summary: "Prepare a release".to_string(),
		},
	];
	let out = render_unknown_command_help("mc", "change", &helps);
	// Should contain error and suggestion text
	assert!(out.contains("Unknown command"));
	assert!(out.contains("change")); // in the error message
	// "change" should appear in the overview because we filter it out,
	// but since it's an unknown command the help shows ALL commands
	assert!(out.contains("release"));
}

#[test]
fn bordered_header_with_long_description() {
	let very_long = "a".repeat(200);
	let out = bordered_header("cmd", &very_long, 50);
	assert!(out.contains("cmd"));
	// Description should be truncated to fit
	for line in out.lines() {
		assert!(line.chars().count() <= 52, "line too wide: {line}"); // chars count for Unicode
	}
}

#[test]
fn render_overview_help_includes_global_flags() {
	let out = render_overview_help("mc");
	assert!(out.contains("Global Flags"));
	assert!(out.contains("--quiet"));
	assert!(out.contains("--progress-format"));
	assert!(out.contains("mc help <command>"));
}

#[test]
fn bordered_header_includes_command_and_description() {
	let out = bordered_header("test", "A test command description", 50);
	// Check that it contains the command name and description
	assert!(out.contains("test"));
	assert!(out.contains("A test command description"));
	// Check border characters are present
	assert!(out.contains("╭"));
	assert!(out.contains("╮"));
	assert!(out.contains("╰"));
	assert!(out.contains("╯"));
	assert!(out.contains("│"));
}

#[test]
fn section_heading_includes_title() {
	let out = section_heading("Description");
	assert!(out.contains("Description"));
}

#[test]
fn example_block_includes_description_and_command() {
	let out = example_block("Do a thing:", "mc thing");
	assert!(out.contains("Do a thing:"));
	assert!(out.contains("mc thing"));
}

#[test]
fn render_overview_help_lists_all_commands() {
	let out = render_overview_help("mc");
	// Should contain overview header
	assert!(out.contains("mc"));
	// Should list several known commands
	assert!(out.contains("change"));
	assert!(out.contains("release"));
	assert!(out.contains("init"));
	assert!(out.contains("help"));
	// Should have global flags section
	assert!(out.contains("Global Flags"));
}

#[test]
fn render_overview_help_with_cli_lists_user_defined_commands() {
	let cli = vec![CliCommandDefinition {
		name: "ship-it".to_string(),
		help_text: Some("Ship the workspace".to_string()),
		inputs: vec![],
		steps: vec![],
		dry_run: false,
	}];
	let out = render_overview_help_with_cli("mc", &cli);

	assert!(out.contains("Built-in Commands"));
	assert!(out.contains("Step Commands"));
	assert!(out.contains("User-defined Commands"));
	assert!(out.contains("ship-it"));
	assert!(out.contains("Ship the workspace"));
}

#[test]
fn render_command_help_for_publish_release_step_is_detailed() {
	let out = render_command_help("mc", "step:publish-release");

	assert!(out.contains("hosted provider release operations"));
	assert!(out.contains("does not publish package artifacts"));
	assert!(out.contains("publish-readiness"));
}

#[test]
fn render_command_help_for_other_step_commands_uses_specific_and_generic_details() {
	let prepare = render_command_help("mc", "step:prepare-release");
	assert!(prepare.contains("PrepareRelease reads pending changesets"));
	assert!(prepare.contains("step:commit-release"));

	let affected = render_command_help("mc", "step:affected-packages");
	assert!(affected.contains("compares changed paths"));
	assert!(affected.contains("--changed-paths"));

	let create = render_command_help("mc", "step:create-change-file");
	assert!(create.contains("writes a structured markdown changeset"));
	assert!(create.contains("--reason"));

	let discover = render_command_help("mc", "step:discover");
	assert!(discover.contains("runs one built-in monochange workflow step directly"));
	assert!(discover.contains("step commands for CI jobs"));
}

#[test]
fn render_command_help_with_cli_documents_user_defined_commands() {
	let discover_step = monochange_core::all_step_variants()
		.into_iter()
		.find(|step| step.step_kebab_name() == "discover")
		.expect("discover step");
	let cli = vec![CliCommandDefinition {
		name: "ship-it".to_string(),
		help_text: None,
		inputs: vec![
			CliInputDefinition {
				name: "format".to_string(),
				kind: CliInputKind::Choice,
				help_text: None,
				required: false,
				default: Some("json".to_string()),
				choices: vec!["json".to_string(), "text".to_string()],
				short: None,
			},
			CliInputDefinition {
				name: "output".to_string(),
				kind: CliInputKind::Path,
				help_text: None,
				required: false,
				default: None,
				choices: vec![],
				short: None,
			},
			CliInputDefinition {
				name: "verify".to_string(),
				kind: CliInputKind::Boolean,
				help_text: Some("Require verification".to_string()),
				required: false,
				default: None,
				choices: vec![],
				short: None,
			},
		],
		steps: vec![discover_step],
		dry_run: false,
	}];
	let out = render_command_help_with_cli("mc", "ship-it", &cli);

	assert!(out.contains("Run configured workflow steps: Discover"));
	assert!(out.contains("loaded from `[cli.ship-it]`"));
	assert!(out.contains("Discover (Discover)"));
	assert!(out.contains("--format"));
	assert!(out.contains("json, text"));
	assert!(out.contains("--output"));
	assert!(out.contains("<PATH>"));
	assert!(out.contains("Require verification"));
	assert!(out.contains("User-defined commands come from monochange.toml"));
	assert!(out.contains("step:discover"));
}

#[test]
fn render_command_help_with_cli_uses_rich_help_for_configured_legacy_commands() {
	let cli = vec![CliCommandDefinition {
		name: "release".to_string(),
		help_text: Some("Configured release workflow".to_string()),
		inputs: vec![],
		steps: vec![],
		dry_run: false,
	}];
	let out = render_command_help_with_cli("mc", "release", &cli);

	assert!(out.contains("Prepare a release from discovered change files"));
	assert!(out.contains("mc release --dry-run"));
}

#[test]
fn render_command_help_with_cli_documents_empty_user_defined_commands() {
	let cli = vec![CliCommandDefinition {
		name: "noop".to_string(),
		help_text: None,
		inputs: vec![],
		steps: vec![],
		dry_run: false,
	}];
	let out = render_command_help_with_cli("mc", "noop", &cli);

	assert!(out.contains("Run a monochange workflow command from monochange.toml"));
	assert!(out.contains("This user-defined command is loaded from `[cli.*]`"));
	assert!(out.contains("mc noop"));
}

#[test]
fn available_command_items_include_builtins_steps_and_configured_commands() {
	let cli = vec![
		CliCommandDefinition {
			name: "init".to_string(),
			help_text: Some("Override built-in init".to_string()),
			inputs: vec![],
			steps: vec![],
			dry_run: false,
		},
		CliCommandDefinition {
			name: "step:discover".to_string(),
			help_text: Some("Override step".to_string()),
			inputs: vec![],
			steps: vec![],
			dry_run: false,
		},
		CliCommandDefinition {
			name: "custom".to_string(),
			help_text: Some("Custom workflow".to_string()),
			inputs: vec![],
			steps: vec![],
			dry_run: false,
		},
	];
	let items = available_command_items(&cli);

	assert!(items.iter().any(|item| item.name == "init"));
	assert!(items.iter().any(|item| item.name == "step:discover"));
	assert!(items.iter().any(|item| item.name == "custom"));
	assert!(
		!configured_command_items(&cli)
			.iter()
			.any(|item| item.name == "init" || item.name == "step:discover")
	);
}

#[test]
fn input_options_document_common_input_names() {
	let names = [
		"package",
		"from",
		"from-ref",
		"target",
		"force",
		"changed_paths",
		"label",
		"from",
		"draft",
		"readiness",
		"resume",
		"mode",
		"ci",
		"interactive",
		"bump",
		"version",
		"reason",
		"type",
		"details",
		"changeset",
		"fix",
		"no_verify",
		"auto-close-issues",
		"custom_value",
	];
	let inputs = names
		.iter()
		.map(|name| {
			CliInputDefinition {
				name: (*name).to_string(),
				kind: if *name == "changed_paths" {
					CliInputKind::StringList
				} else {
					CliInputKind::String
				},
				help_text: None,
				required: false,
				default: None,
				choices: vec![],
				short: None,
			}
		})
		.collect::<Vec<_>>();
	let options = input_options(&inputs);
	let joined = options
		.iter()
		.map(|(flag, type_name, description)| format!("{flag} {type_name} {description}"))
		.collect::<Vec<_>>()
		.join("\n");

	assert!(joined.contains("Limit the command to one or more package ids"));
	assert!(joined.contains("Git ref, branch, tag, or commit used as input"));
	assert!(joined.contains("Allow an otherwise unsafe operation"));
	assert!(joined.contains("Changed paths to evaluate"));
	assert!(joined.contains("Close linked issues after commenting"));
	assert!(joined.contains("Value for `custom-value`"));
}

#[test]
fn step_summary_for_kind_covers_command_and_fallback_labels() {
	assert_eq!(
		step_summary_for_kind("Command"),
		"Run an arbitrary configured shell command step"
	);
	assert_eq!(
		step_summary_for_kind("FutureStep"),
		"Run the built-in FutureStep step"
	);
}

#[test]
fn step_command_items_cover_all_generated_step_summaries() {
	let items = step_command_items();
	let joined = items
		.iter()
		.map(|item| format!("{} {}", item.name, item.summary))
		.collect::<Vec<_>>()
		.join("\n");

	assert!(joined.contains("step:config"));
	assert!(joined.contains("Render resolved monochange configuration"));
	assert!(joined.contains("step:validate"));
	assert!(joined.contains("step:display-versions"));
	assert!(joined.contains("step:plan-publish-rate-limits"));
	assert!(joined.contains("step:retarget-release"));
	assert!(joined.contains("Publish package versions from a publish plan"));
}

#[test]
fn render_command_help_for_change() {
	let out = render_command_help("mc", "change");
	assert!(out.contains("change"));
	assert!(out.contains("Description"));
	assert!(out.contains("Usage"));
	assert!(out.contains("Options"));
	assert!(out.contains("Examples"));
	assert!(out.contains("Tips"));
	assert!(out.contains("See Also"));
}

#[test]
fn render_command_help_for_release() {
	let out = render_command_help("mc", "release");
	assert!(out.contains("release"));
	assert!(out.contains("Description"));
	assert!(out.contains("Usage"));
}

#[test]
fn render_command_help_for_init() {
	let out = render_command_help("mc", "init");
	assert!(out.contains("init"));
	assert!(out.contains("Examples"));
}

#[test]
fn render_command_help_for_subagents() {
	let out = render_command_help("mc", "subagents");
	assert!(out.contains("subagents"));
	assert!(out.contains("Tips"));
}

#[test]
fn render_command_help_for_analyze() {
	let out = render_command_help("mc", "analyze");
	assert!(out.contains("analyze"));
	assert!(out.contains("Options"));
}

#[test]
fn render_command_help_for_versions() {
	let out = render_command_help("mc", "versions");
	assert!(out.contains("versions"));
}

#[test]
fn render_command_help_for_repair_release() {
	let out = render_command_help("mc", "repair-release");
	assert!(out.contains("repair-release"));
	assert!(out.contains("Options"));
}

#[test]
fn render_command_help_for_tag_release() {
	let out = render_command_help("mc", "tag-release");
	assert!(out.contains("tag-release"));
	assert!(out.contains("Examples"));
}

#[test]
fn render_command_help_for_check() {
	let out = render_command_help("mc", "check");
	assert!(out.contains("check"));
	assert!(out.contains("Options"));
}

#[test]
fn render_command_help_for_lint() {
	let out = render_command_help("mc", "lint");
	assert!(out.contains("lint"));
	assert!(out.contains("Options"));
}

#[test]
fn render_command_help_for_mcp() {
	let out = render_command_help("mc", "mcp");
	assert!(out.contains("mcp"));
	assert!(out.contains("Description"));
}

#[test]
fn render_command_help_for_skill() {
	let out = render_command_help("mc", "skill");
	assert!(out.contains("skill"));
}

#[test]
fn render_command_help_for_populate() {
	let out = render_command_help("mc", "populate");
	assert!(out.contains("populate"));
}

#[test]
fn render_command_help_for_validate() {
	let out = render_command_help("mc", "validate");
	assert!(out.contains("validate"));
}

#[test]
fn render_command_help_for_discover() {
	let out = render_command_help("mc", "discover");
	assert!(out.contains("discover"));
}

#[test]
fn render_command_help_for_commit_release() {
	let out = render_command_help("mc", "commit-release");
	assert!(out.contains("commit-release"));
}

#[test]
fn render_command_help_for_release_pr() {
	let out = render_command_help("mc", "release-pr");
	assert!(out.contains("release-pr"));
}

#[test]
fn render_command_help_for_affected() {
	let out = render_command_help("mc", "affected");
	assert!(out.contains("affected"));
}

#[test]
fn render_command_help_for_diagnostics() {
	let out = render_command_help("mc", "diagnostics");
	assert!(out.contains("diagnostics"));
}

#[test]
fn render_command_help_for_release_record() {
	let out = render_command_help("mc", "release-record");
	assert!(out.contains("release-record"));
}

#[test]
fn render_command_help_for_publish_readiness() {
	let out = render_command_help("mc", "publish-readiness");
	assert!(out.contains("publish-readiness"));
	assert!(out.contains("readiness artifact"));
}

#[test]
fn render_command_help_for_publish_bootstrap() {
	let out = render_command_help("mc", "publish-bootstrap");
	assert!(out.contains("publish-bootstrap"));
	assert!(out.contains("bootstrap result artifact"));
}

#[test]
fn render_command_help_for_placeholder_publish() {
	let out = render_command_help("mc", "placeholder-publish");
	assert!(out.contains("placeholder-publish"));
}

#[test]
fn render_command_help_for_publish_packages() {
	let out = render_command_help("mc", "publish-packages");
	assert!(out.contains("publish-packages"));
}

#[test]
fn render_command_help_for_unknown_shows_error() {
	let out = render_command_help("mc", "nonexistent");
	assert!(out.contains("error:"));
	assert!(out.contains("Unknown command"));
	assert!(out.contains("mc help"));
	// Should list available commands
	assert!(out.contains("change"));
}

#[test]
fn multiline_indent_indents_continuation_lines() {
	let text = "first line\nsecond line\nthird line";
	let out = multiline_indent(text, 4);
	let lines: Vec<&str> = out.lines().collect();
	assert_eq!(lines[0], "first line");
	assert_eq!(lines[1], "    second line");
	assert_eq!(lines[2], "    third line");
}

#[test]
fn multiline_indent_with_single_line() {
	assert_eq!(multiline_indent("hello", 4), "hello");
}
