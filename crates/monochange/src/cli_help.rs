//! Beautiful, colored CLI help renderer for `mc help <command>`.
//!
//! This module provides a custom help experience that goes beyond clap's
//! built-in `--help` output. It adds detailed descriptions, multiple examples
//! per command, tips, and cross-references — all rendered with ANSI colors
//! when the terminal supports them.

#![allow(clippy::format_push_string, clippy::single_char_add_str)]

use std::io::IsTerminal;

// ---------------------------------------------------------------------------
// Color theme
// ---------------------------------------------------------------------------

/// Whether ANSI colors should be emitted.
fn color_enabled() -> bool {
	color_enabled_impl(
		std::env::var_os("NO_COLOR").is_some(),
		std::env::var_os("TERM").is_some_and(|v| v == "dumb"),
		std::io::stdout().is_terminal(),
	)
}

/// Testable implementation for 100% branch coverage.
fn color_enabled_impl(no_color: bool, term_dumb: bool, is_terminal: bool) -> bool {
	if no_color {
		return false;
	}
	if term_dumb {
		return false;
	}
	is_terminal
}

/// Apply an `anstyle::Style` to `text` when colors are enabled.
fn paint(text: &str, style: anstyle::Style) -> String {
	paint_impl(text, style, color_enabled())
}

/// Testable implementation for 100% branch coverage.
fn paint_impl(text: &str, style: anstyle::Style, enabled: bool) -> String {
	if enabled {
		format!("{style}{text}{style:#}")
	} else {
		text.to_string()
	}
}

// Shorthand style constructors using the monochange palette.
// These delegate to `crate::cli_theme` so clap `--help` and
// `mc help` share the exact same ANSI styles.

fn accent() -> anstyle::Style {
	crate::cli_theme::header()
}

fn header() -> anstyle::Style {
	crate::cli_theme::usage()
}

fn flag_style() -> anstyle::Style {
	crate::cli_theme::literal()
}

fn value_style() -> anstyle::Style {
	crate::cli_theme::placeholder()
}

fn muted() -> anstyle::Style {
	crate::cli_theme::muted()
}

fn error_style() -> anstyle::Style {
	crate::cli_theme::error()
}

fn code_style() -> anstyle::Style {
	crate::cli_theme::valid()
}

// ---------------------------------------------------------------------------
// Bordered header
// ---------------------------------------------------------------------------

/// Render a bordered header like:
///
/// ```text
/// ╭──────────────────────────────────────────────╮
/// │  mc change                                     │
/// │  Create a change file for one or more packages │
/// ╰──────────────────────────────────────────────╯
/// ```
fn bordered_header(command: &str, description: &str, width: usize) -> String {
	let inner = width - 4; // account for │ and spaces
	let name_line = format!("  {command}");
	let desc_line = if description.len() > inner {
		&description[..inner]
	} else {
		description
	};

	let mut lines = Vec::new();
	lines.push(format!("╭{}╮", "─".repeat(width.saturating_sub(2))));
	let name_pad = width.saturating_sub(2).saturating_sub(name_line.len());
	lines.push(format!("│{name_line}{}│", " ".repeat(name_pad)));
	let desc_pad = width.saturating_sub(4).saturating_sub(desc_line.len());
	lines.push(format!("│  {desc_line}{}│", " ".repeat(desc_pad)));
	lines.push(format!("╰{}╯", "─".repeat(width.saturating_sub(2))));
	lines.join("\n")
}

// ---------------------------------------------------------------------------
// Section helpers
// ---------------------------------------------------------------------------

fn section_heading(title: &str) -> String {
	format!("{} {}", paint("▸", accent()), paint(title, header()))
}

fn example_block(description: &str, command: &str) -> String {
	let desc = paint(description, muted());
	let cmd = paint(command, code_style());
	format!("  {desc}\n    {cmd}")
}

// ---------------------------------------------------------------------------
// Per-command detailed help content
// ---------------------------------------------------------------------------

struct CommandHelp {
	name: &'static str,
	summary: &'static str,
	description: &'static str,
	usage: &'static str,
	options: &'static [(&'static str, &'static str, &'static str)], // (flag, type, help)
	examples: &'static [(&'static str, &'static str)],              // (description, command)
	tips: &'static [&'static str],
	see_also: &'static [&'static str],
}

fn builtin_command_helps() -> Vec<CommandHelp> {
	vec![
		CommandHelp {
			name: "init",
			summary: "Generate monochange.toml with detected packages",
			description: "Scans the workspace for supported package manifests (Cargo.toml, package.json, \
				deno.json, pubspec.yaml) and generates a monochange.toml configuration file with \
				discovered packages, version groups, and default CLI commands.\n\n\
				Use --provider to scaffold source-control integration (GitHub, GitLab, Gitea) \
				with release automation CLI commands.",
			usage: "mc init [OPTIONS]",
			options: &[
				("--force", "", "Overwrite an existing monochange.toml file"),
				(
					"--provider",
					"<PROVIDER>",
					"Source-control provider (github, gitlab, gitea)",
				),
			],
			examples: &[
				("Initialize a fresh workspace:", "mc init"),
				("Overwrite existing config:", "mc init --force"),
				(
					"Initialize with GitHub integration:",
					"mc init --provider github",
				),
				(
					"Initialize with GitLab integration:",
					"mc init --provider gitlab",
				),
				(
					"Initialize with Gitea integration:",
					"mc init --provider gitea",
				),
			],
			tips: &[
				"Run mc init at the root of your monorepo.",
				"The generated config is a starting point — customize packages, groups, and CLI commands in monochange.toml.",
				"Use --provider=github to get GitHub Actions workflow templates included.",
			],
			see_also: &["populate", "validate", "discover"],
		},
		CommandHelp {
			name: "populate",
			summary: "Add missing built-in CLI commands to monochange.toml",
			description: "Compares the built-in default CLI commands against what is defined in \
				monochange.toml and appends any missing commands so you can customize them. \
				Existing command definitions are never modified.",
			usage: "mc populate",
			options: &[],
			examples: &[("Add any missing default commands:", "mc populate")],
			tips: &[
				"Run this after upgrading monochange to pick up new commands.",
				"This is a safe, additive-only operation.",
			],
			see_also: &["init", "validate"],
		},
		CommandHelp {
			name: "skill",
			summary: "Install the monochange skill bundle for AI agents",
			description: "Installs monochange-specific skills into the current project using the \
				`skills` CLI package manager. Skills enable AI coding agents to understand \
				and use monochange effectively.\n\n\
				All arguments after `mc skill` are forwarded to `skills add <monochange-source>`.",
			usage: "mc skill [FLAGS...]",
			options: &[("(forwarded)", "", "All args are forwarded to `skills add`")],
			examples: &[
				("List available skills:", "mc skill --list"),
				("Install for Claude Code:", "mc skill -a claude-code"),
				("Install for pi globally:", "mc skill -g -a pi -y"),
				(
					"Install specific skill with copy:",
					"mc skill --skill monochange --copy -y",
				),
				(
					"Install for multiple agents:",
					"mc skill -a claude-code -a codex",
				),
			],
			tips: &[
				"Runner selection is automatic: npx → pnpm dlx → bunx.",
				"Use --copy instead of symlinks for immutable installs.",
				"Use -y to skip confirmation prompts.",
			],
			see_also: &["subagents"],
		},
		CommandHelp {
			name: "subagents",
			summary: "Generate repo-local monochange subagents and agent guidance",
			description: "Generates AI agent configuration files (markdown instructions, MCP config, \
				agent definitions) for supported coding assistant platforms. Generated agents \
				are CLI-first and prefer `mc` over library APIs.",
			usage: "mc subagents <TARGET(S)> [OPTIONS]",
			options: &[
				("<TARGET>", "", "claude, vscode, copilot, pi, codex, cursor"),
				("--all", "", "Generate for all supported targets"),
				("--force", "", "Overwrite existing files"),
				("--dry-run", "", "Preview without writing"),
				("--format", "<FORMAT>", "Output format (text, json)"),
				("--no-mcp", "", "Skip MCP config files"),
			],
			examples: &[
				("Generate for Claude:", "mc subagents claude"),
				("Generate for multiple targets:", "mc subagents pi codex"),
				("Generate for all targets:", "mc subagents --all"),
				("Preview without writing:", "mc subagents --all --dry-run"),
				(
					"Generate without MCP config:",
					"mc subagents vscode copilot --no-mcp",
				),
			],
			tips: &[
				"Target mapping:\n    • claude  → .claude/agents/*.md and .mcp.json\n    • vscode  → .github/agents/*.agent.md and .vscode/mcp.json\n    • copilot → .github/agents/*.agent.md and .vscode/mcp.json\n    • pi      → .pi/agents/*.md\n    • codex   → .codex/agents/*.toml\n    • cursor  → .cursor/rules/*.mdc",
				"Generated agents prefer: mc → monochange → npx -y @monochange/cli",
			],
			see_also: &["skill", "mcp"],
		},
		CommandHelp {
			name: "analyze",
			summary: "Analyze semantic changes for a package",
			description: "Runs package-scoped semantic analysis comparing a package's public API across \
				main, head, and optional release baselines. Produces a structured assessment of \
				what changed and the implied semver bump.\n\n\
				Defaults --release-ref to the newest tag for the package or its version group. \
				If no prior release tag exists, falls back to first-release analysis using \
				only main → head.",
			usage: "mc analyze --package <PACKAGE> [OPTIONS]",
			options: &[
				(
					"--package",
					"<PACKAGE>",
					"Package id, name, manifest path, or directory (required)",
				),
				("--release-ref", "<REF>", "Explicit release baseline ref"),
				(
					"--main-ref",
					"<REF>",
					"Base branch ref (default: detected default branch)",
				),
				("--head-ref", "<REF>", "Head ref to analyze (default: HEAD)"),
				(
					"--detection-level",
					"<LEVEL>",
					"basic, signature, semantic (default: signature)",
				),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				("Analyze a package:", "mc analyze --package core"),
				("JSON output:", "mc analyze --package core --format json"),
				(
					"Against a specific release tag:",
					"mc analyze --package core --release-ref core/v1.2.3",
				),
				(
					"Custom main and head refs:",
					"mc analyze --package core --main-ref main --head-ref HEAD",
				),
			],
			tips: &[
				"Use the package id from monochange.toml for the most reliable resolution.",
				"Add --format json for scripting and LLM consumption.",
			],
			see_also: &["release", "versions", "change"],
		},
		CommandHelp {
			name: "change",
			summary: "Create a change file for one or more packages",
			description: "Creates a structured changeset markdown file in .changeset/ that describes \
				what changed, which packages are affected, and the requested semver bump. \
				These changeset files are consumed during release planning to produce \
				version bumps, changelogs, and release manifests.\n\n\
				You can target individual packages or entire version groups. Dependents and \
				group members are propagated automatically during planning. Use --caused-by \
				to mark dependency-only follow-ups.",
			usage: "mc change [OPTIONS]",
			options: &[
				(
					"-i, --interactive",
					"",
					"Select packages, bumps, and options interactively",
				),
				(
					"--package",
					"<PACKAGE>",
					"Package or group to include (repeatable)",
				),
				(
					"--bump",
					"<BUMP>",
					"none, patch, minor, major (default: patch)",
				),
				(
					"--version",
					"<VERSION>",
					"Pin an explicit version for this release",
				),
				("--reason", "<REASON>", "Short release-note summary"),
				(
					"--type",
					"<TYPE>",
					"Release-note type (feat, fix, security, etc.)",
				),
				(
					"--caused-by",
					"<CAUSED_BY>",
					"Ids that caused this dependent change (repeatable)",
				),
				("--details", "<DETAILS>", "Multi-line release-note details"),
				(
					"--output",
					"<PATH>",
					"Write the change file to a specific path",
				),
			],
			examples: &[
				(
					"Quick patch for a single package:",
					r#"mc change --package core --bump patch --reason "fix null pointer""#,
				),
				(
					"Minor feature with output path:",
					r#"mc change --package api --bump minor --reason "add pagination" --output .changeset/api-pagination.md"#,
				),
				(
					"Group-level change:",
					r#"mc change --package sdk --bump minor --reason "coordinated release""#,
				),
				(
					"Dependency-only follow-up:",
					r#"mc change --package utils --bump patch --caused-by core --reason "bump for core compat""#,
				),
				("Interactive mode:", "mc change --interactive"),
				(
					"Explicit version pin:",
					r#"mc change --package core --bump major --version 2.0.0 --reason "promote to stable""#,
				),
			],
			tips: &[
				"Prefer configured package ids over manifest paths.",
				"Use a group id only when the change is intentionally owned by the whole group.",
				"Dependents and grouped members propagate automatically during planning.",
				"--caused-by marks a package as only changing because another moved first.",
				"Legacy manifest paths resolve during migration, but declared ids are the stable interface.",
			],
			see_also: &["release", "versions", "affected"],
		},
		CommandHelp {
			name: "release",
			summary: "Prepare a release from discovered change files",
			description: "Reads all changeset files in .changeset/, plans version bumps and changelog \
				updates, and prepares the release artifacts. By default, output is rendered in \
				markdown format.\n\n\
				In dry-run mode, no files are modified. Use --diff to see unified file diffs \
				for the planned changes. Use --prepared-release to read or write a cached \
				release artifact for multi-step workflows.",
			usage: "mc release [OPTIONS]",
			options: &[
				("--dry-run", "", "Preview without modifying files"),
				("--diff", "", "Show unified file diffs for the release"),
				(
					"--format",
					"<FORMAT>",
					"markdown, text, json (default: markdown)",
				),
				(
					"--prepared-release",
					"<PATH>",
					"Read/write prepared release artifact path",
				),
			],
			examples: &[
				(
					"Dry-run preview in text format:",
					"mc release --dry-run --format text",
				),
				(
					"Dry-run with JSON for scripting:",
					"mc release --dry-run --format json",
				),
				("Preview with file diffs:", "mc release --dry-run --diff"),
				("Execute the release:", "mc release"),
			],
			tips: &[
				"Direct package changes propagate to dependents using defaults.parent_bump.",
				"Group synchronization happens before final output is rendered.",
				"Explicit versions on grouped members propagate to the whole group.",
				"Use --prepared-release to cache the release for multi-step workflows.",
			],
			see_also: &["change", "versions", "commit-release", "release-pr"],
		},
		CommandHelp {
			name: "versions",
			summary: "Display planned versions without modifying files",
			description: "Computes the same planned versions used by mc release but renders them without \
				mutating any manifests, changelogs, or changesets. This is a read-only preview \
				of what a release would produce.",
			usage: "mc versions [OPTIONS]",
			options: &[(
				"--format",
				"<FORMAT>",
				"text, markdown, json (default: text)",
			)],
			examples: &[
				("Show planned versions:", "mc versions"),
				("Markdown output:", "mc versions --format markdown"),
				("JSON for scripting:", "mc versions --format json"),
			],
			tips: &[
				"This command is read-only — it does not update manifests or changelogs.",
				"It computes the same planned versions used by monochange release workflows.",
			],
			see_also: &["release", "change"],
		},
		CommandHelp {
			name: "commit-release",
			summary: "Create a local release commit with an embedded release record",
			description: "Creates a git commit that embeds a durable monochange release record in the \
				commit body. The release record allows later steps (tag-release, repair-release) \
				to reconstruct the full release tag set from the commit alone.\n\n\
				Requires a previous PrepareRelease step or a prepared-release artifact.",
			usage: "mc commit-release [OPTIONS]",
			options: &[
				("--dry-run", "", "Preview the commit without creating it"),
				("--diff", "", "Show file diffs for the release"),
				(
					"--format",
					"<FORMAT>",
					"markdown, text, json (default: markdown)",
				),
			],
			examples: &[
				("Preview the commit:", "mc commit-release --dry-run"),
				("Preview with diffs:", "mc commit-release --dry-run --diff"),
				("JSON preview:", "mc commit-release --dry-run --format json"),
				("Execute the commit:", "mc commit-release"),
			],
			tips: &[
				"Reuses the standard monochange release commit subject/body contract.",
				"Embeds a durable release record block in the commit body.",
				"Can run before OpenReleaseRequest in the same workflow.",
			],
			see_also: &["release", "tag-release", "release-pr"],
		},
		CommandHelp {
			name: "release-pr",
			summary: "Open or update a hosted release pull request",
			description: "Opens (or updates an existing) pull request on the configured source host \
				(GitHub, GitLab, Gitea) with the prepared release changes. Requires [source] \
				configuration in monochange.toml.",
			usage: "mc release-pr [OPTIONS]",
			options: &[
				("--dry-run", "", "Preview without creating the PR"),
				("--diff", "", "Show file diffs for the release"),
				(
					"--format",
					"<FORMAT>",
					"markdown, text, json (default: markdown)",
				),
			],
			examples: &[
				("Preview the PR:", "mc release-pr --dry-run"),
				("Preview with markdown diff:", "mc release-pr --dry-run"),
				("Create the PR:", "mc release-pr"),
			],
			tips: &[
				"Requires [source] configuration with provider, owner, and repo.",
				"Labels and auto-merge settings come from [source.pull_requests].",
			],
			see_also: &["commit-release", "release"],
		},
		CommandHelp {
			name: "affected",
			summary: "Evaluate affected packages and changeset coverage",
			description: "CI-oriented command that evaluates whether changed paths are adequately covered \
				by changeset files. Useful in pull request checks to verify that every touched \
				package has a corresponding changeset.\n\n\
				Returns exit code 0 when coverage passes, non-zero otherwise.",
			usage: "mc affected [OPTIONS]",
			options: &[
				(
					"--changed-paths",
					"<PATHS>",
					"File paths changed in the PR (repeatable)",
				),
				(
					"--since",
					"<REF>",
					"Git ref to diff against (e.g. origin/main)",
				),
				(
					"--verify",
					"",
					"Verify changeset coverage for affected packages",
				),
				(
					"--label",
					"<LABELS>",
					"PR labels that may skip verification (repeatable)",
				),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				(
					"Check specific changed paths:",
					"mc affected --changed-paths crates/core/src/lib.rs --format json",
				),
				(
					"Compare against a branch:",
					"mc affected --since origin/main --verify",
				),
				(
					"In CI with labels:",
					"mc affected --since origin/main --label skip-changeset",
				),
			],
			tips: &[
				"Prefer package ids in .changeset files.",
				"Group-owned changesets cover all members of that group.",
				"Ignored paths and skip labels are configured in [changesets.verify].",
			],
			see_also: &["change", "check"],
		},
		CommandHelp {
			name: "diagnostics",
			summary: "Inspect parsed changeset data, provenance, and metadata",
			description: "Dumps detailed structured information about changeset files including: \
				target packages/groups, requested bumps, the commit SHA that introduced \
				and last updated each changeset, linked review requests, and related issue \
				references.",
			usage: "mc diagnostics [OPTIONS]",
			options: &[
				(
					"--changeset",
					"<PATH>",
					"Specific changeset file(s) to diagnose (repeatable)",
				),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				("Diagnose all changesets:", "mc diagnostics"),
				("JSON output:", "mc diagnostics --format json"),
				(
					"Specific changeset:",
					"mc diagnostics --changeset .changeset/feature.md",
				),
			],
			tips: &[
				"Use --format json for LLM and scripting consumption.",
				"When [source] is configured, diagnostics include hosted metadata (PR links, issue refs).",
			],
			see_also: &["affected", "change"],
		},
		CommandHelp {
			name: "repair-release",
			summary: "Repair a recent release by retargeting its tag set",
			description: "Finds the release record from history and moves the full release tag set to \
				a new target commit. Defaults to descendant-only retargets for safety; use \
				--force to retarget to non-descendant commits.\n\n\
				Can also sync hosted releases on GitHub/GitLab/Gitea when source is configured.",
			usage: "mc repair-release --from <REF> [OPTIONS]",
			options: &[
				(
					"--from",
					"<REF>",
					"Tag or commit-ish locating the release (required)",
				),
				("--target", "<REF>", "Target commit (default: HEAD)"),
				("--force", "", "Allow retarget to non-descendant commits"),
				(
					"--sync-provider",
					"=BOOL",
					"Sync hosted release (default: true)",
				),
				("--dry-run", "", "Preview without modifying tags"),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				(
					"Dry-run repair:",
					"mc repair-release --from v1.2.3 --dry-run",
				),
				(
					"Repair to HEAD:",
					"mc repair-release --from v1.2.3 --target HEAD",
				),
				(
					"Force retarget:",
					"mc repair-release --from v1.2.3 --target HEAD --force",
				),
				(
					"Skip provider sync:",
					"mc repair-release --from v1.2.3 --sync-provider=false",
				),
			],
			tips: &[
				"Defaults to descendant-only retargets unless --force is set.",
				"Hosted release sync runs by default; disable with --sync-provider=false.",
				"Use mc tag-release to create tags from a fresh release commit instead.",
			],
			see_also: &["tag-release", "release-record", "release"],
		},
		CommandHelp {
			name: "tag-release",
			summary: "Create and push release tags from an embedded release record",
			description: "Reads the monochange release record embedded in a commit's body and creates \
				the full tag set declared by that record. Pushes tags to origin by default. \
				Reruns on the same commit are treated as already up to date.",
			usage: "mc tag-release --from <REF> [OPTIONS]",
			options: &[
				("--from", "<REF>", "Release commit ref (required)"),
				("--push", "=BOOL", "Push tags to origin (default: true)"),
				("--dry-run", "", "Preview without creating/pushing tags"),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				("Create and push tags:", "mc tag-release --from HEAD"),
				("Dry-run preview:", "mc tag-release --from HEAD --dry-run"),
				(
					"Create without pushing:",
					"mc tag-release --from HEAD --push=false",
				),
				(
					"JSON output:",
					"mc tag-release --from HEAD --dry-run --format json",
				),
			],
			tips: &[
				"Requires the resolved ref itself to be the monochange release commit.",
				"Creates the full tag set declared by that release record.",
				"Reruns on the same commit are treated as already up to date.",
				"Use mc repair-release to move existing tags later.",
			],
			see_also: &["repair-release", "release-record", "commit-release"],
		},
		CommandHelp {
			name: "release-record",
			summary: "Inspect the monochange release record for a tag or commit",
			description: "Resolves the supplied ref to a commit, then walks first-parent ancestry until \
				it finds a monochange release record embed. Renders the full release record \
				including targets, versions, changed files, and changelogs.",
			usage: "mc release-record --from <REF> [OPTIONS]",
			options: &[
				("--from", "<REF>", "Tag or commit-ish to locate (required)"),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				("Inspect by tag:", "mc release-record --from v1.2.3"),
				("Inspect by commit:", "mc release-record --from HEAD"),
				(
					"JSON output:",
					"mc release-record --from v1.2.3 --format json",
				),
			],
			tips: &[
				"Fails loudly if it encounters a malformed release record block.",
				"Walks first-parent ancestry to find the record.",
			],
			see_also: &["tag-release", "repair-release"],
		},
		CommandHelp {
			name: "check",
			summary: "Validate configuration, changesets, and run manifest lint rules",
			description: "Validates monochange.toml, changeset files, and runs ecosystem-specific \
				manifest lint rules (e.g., Cargo.toml sorting, package.json constraints). \
				Use --fix to auto-fix issues where possible.",
			usage: "mc check [OPTIONS]",
			options: &[
				("-f, --fix", "", "Auto-fix lint issues where possible"),
				(
					"-e, --ecosystem",
					"<ECOSYSTEMS>",
					"Limit to specific ecosystem suites (comma-sep)",
				),
				(
					"--only",
					"<RULES>",
					"Run only specific lint rule ids (comma-sep)",
				),
				("--format", "<FORMAT>", "text, json (default: text)"),
			],
			examples: &[
				("Run all checks:", "mc check"),
				("Auto-fix issues:", "mc check --fix"),
				("Specific ecosystem:", "mc check --ecosystem cargo,npm"),
				(
					"Specific rule:",
					"mc check --only cargo/sorted-dependencies",
				),
			],
			tips: &[
				"Lint rules are configured in [lints] of monochange.toml.",
				"Use mc lint list to see available rules and presets.",
			],
			see_also: &["lint", "validate", "affected"],
		},
		CommandHelp {
			name: "lint",
			summary: "Inspect and scaffold manifest lint rules",
			description: "Subcommand group for listing, explaining, and creating lint rules that \
				enforce manifest quality standards across your monorepo.",
			usage: "mc lint <SUBCOMMAND>",
			options: &[
				("list", "", "List registered lint rules and presets"),
				("explain <ID>", "", "Explain a lint rule or preset"),
				("new <ID>", "", "Scaffold a new lint rule (ecosystem/name)"),
			],
			examples: &[
				("List all rules:", "mc lint list"),
				(
					"Explain a rule:",
					"mc lint explain cargo/sorted-dependencies",
				),
				(
					"Create a new rule:",
					"mc lint new cargo/no-path-dependencies",
				),
				(
					"Create npm rule:",
					"mc lint new npm/require-package-manager",
				),
			],
			tips: &[
				"Rule ids follow the <ecosystem>/<name> pattern.",
				"Use mc check to run lint rules, mc lint to manage them.",
			],
			see_also: &["check", "validate"],
		},
		CommandHelp {
			name: "mcp",
			summary: "Start the monochange MCP server over stdin/stdout",
			description: "Starts a Model Context Protocol (MCP) server that exposes monochange \
				capabilities as tools for AI assistants. The server communicates over \
				stdin/stdout using the MCP protocol.\n\n\
				AI agents can use the MCP server to discover packages, create changes, \
				plan releases, and more — all through structured tool calls.",
			usage: "mc mcp",
			options: &[],
			examples: &[("Start the MCP server:", "mc mcp")],
			tips: &[
				"The MCP server is designed for AI agent consumption, not direct human use.",
				"Configure your agent's MCP settings to point to this command.",
			],
			see_also: &["subagents", "skill"],
		},
		CommandHelp {
			name: "validate",
			summary: "Validate monochange configuration and changesets",
			description: "Validates the monochange.toml configuration, package manifests, version \
				groups, changeset files, and workspace consistency. This is the same \
				validation step that runs at the start of release commands.",
			usage: "mc validate",
			options: &[],
			examples: &[("Validate the workspace:", "mc validate")],
			tips: &[
				"Runs automatically before release commands.",
				"Standalone use is for pre-commit hooks or CI gates.",
			],
			see_also: &["check", "discover"],
		},
		CommandHelp {
			name: "discover",
			summary: "Discover packages across supported ecosystems",
			description: "Scans the workspace for packages across Cargo, npm/pnpm/Bun, Deno, and \
				Dart/Flutter ecosystems and renders a structured discovery report.",
			usage: "mc discover [OPTIONS]",
			options: &[("--format", "<FORMAT>", "text, json (default: text)")],
			examples: &[
				("Discover all packages:", "mc discover"),
				("JSON output:", "mc discover --format json"),
			],
			tips: &[
				"Discovery is read-only and does not modify any files.",
				"JSON output is useful for scripting and LLM consumption.",
			],
			see_also: &["validate", "init"],
		},
		CommandHelp {
			name: "placeholder-publish",
			summary: "Publish placeholder versions for missing registry packages",
			description: "Packages that have never been published to their target registry (crates.io, \
				npm, pub.dev, JSR) need an initial placeholder version before automated \
				publishing can work. This command publishes those placeholders.",
			usage: "mc placeholder-publish [OPTIONS]",
			options: &[
				(
					"--format",
					"<FORMAT>",
					"text, markdown, json (default: text)",
				),
				(
					"--package",
					"<PACKAGE>",
					"Restrict to specific package ids (repeatable)",
				),
				("--dry-run", "", "Preview without publishing"),
			],
			examples: &[
				("Dry-run all:", "mc placeholder-publish --dry-run"),
				(
					"Specific package:",
					"mc placeholder-publish --package core --dry-run",
				),
				(
					"JSON output:",
					"mc placeholder-publish --dry-run --format json",
				),
			],
			tips: &[
				"Placeholder versions are 0.0.0 by default.",
				"Only unpublished packages are included.",
			],
			see_also: &["release", "publish-packages"],
		},
		CommandHelp {
			name: "publish-packages",
			summary: "Publish package versions from release state",
			description: "Publishes package versions to their target registries using the prepared \
				release data. Supports trusted publishing on supported registries.",
			usage: "mc publish-packages [OPTIONS]",
			options: &[
				(
					"--format",
					"<FORMAT>",
					"text, markdown, json (default: text)",
				),
				(
					"--package",
					"<PACKAGE>",
					"Restrict to specific package ids (repeatable)",
				),
				("--dry-run", "", "Preview without publishing"),
			],
			examples: &[
				("Dry-run all:", "mc publish-packages --dry-run"),
				("Specific package:", "mc publish-packages --package core"),
				(
					"JSON format:",
					"mc publish-packages --dry-run --format json",
				),
			],
			tips: &[
				"Requires a prepared release from a previous release step.",
				"Trusted publishing is used when running in GitHub Actions with OIDC.",
			],
			see_also: &["placeholder-publish", "release"],
		},
	]
}

// ---------------------------------------------------------------------------
// Render full help for a named command
// ---------------------------------------------------------------------------

/// Render beautiful, detailed help for the named command.
pub fn render_command_help(bin_name: &str, command_name: &str) -> String {
	let helps = builtin_command_helps();
	let Some(help) = helps.iter().find(|h| h.name == command_name) else {
		return render_unknown_command_help(bin_name, command_name, &helps);
	};
	render_single_command_help(bin_name, help)
}

/// Render top-level help listing all commands.
pub fn render_overview_help(bin_name: &str) -> String {
	let helps = builtin_command_helps();
	let mut out = String::new();

	// Header
	out.push_str(&bordered_header(
		bin_name,
		"monochange — versioning & releases for your monorepo",
		60,
	));
	out.push_str("\n\n");

	// Description
	out.push_str(&section_heading("Description"));
	out.push_str("\n\n");
	out.push_str(&paint(
		"monochange discovers packages across Cargo, npm/pnpm/Bun, Deno, and Dart/Flutter, \
		then coordinates version bumps, changelogs, and release automation from a single \
		monochange.toml config.\n\n\
		Run `mc help <command>` for detailed examples and usage tips for any command.",
		muted(),
	));
	out.push_str("\n\n");

	// Commands listing
	out.push_str(&section_heading("Commands"));
	out.push_str("\n\n");

	let name_width = helps.iter().map(|h| h.name.len()).max().unwrap_or(20);
	for help in &helps {
		let padded = format!("{:width$}", help.name, width = name_width);
		out.push_str(&format!(
			"  {}  {}\n",
			paint(&padded, flag_style()),
			paint(help.summary, muted()),
		));
	}

	out.push_str("\n");
	out.push_str(&section_heading("Global Flags"));
	out.push_str("\n\n");
	out.push_str(&format!(
		"  {}   {}\n",
		paint("--quiet  ", flag_style()),
		paint("Suppress output, run in dry-run mode", muted()),
	));
	out.push_str(&format!(
		"  {} {}\n",
		paint("--progress-format", flag_style()),
		paint("<FORMAT>  auto, unicode, ascii, json", muted()),
	));
	out.push_str(&format!(
		"  {}\n",
		paint(
			"Use `mc help <command>` for detailed command help.",
			accent()
		),
	));

	out
}

fn render_single_command_help(bin_name: &str, help: &CommandHelp) -> String {
	let mut out = String::new();

	// Bordered header
	out.push_str(&bordered_header(
		&format!("{} {}", bin_name, help.name),
		help.summary,
		60,
	));
	out.push_str("\n\n");

	// Description
	out.push_str(&section_heading("Description"));
	out.push_str("\n\n");
	for line in help.description.lines() {
		if line.is_empty() {
			out.push('\n');
		} else {
			out.push_str(&format!("  {line}\n"));
		}
	}
	out.push('\n');

	// Usage
	out.push_str(&section_heading("Usage"));
	out.push_str("\n\n");
	out.push_str(&format!("  {}\n\n", paint(help.usage, accent())));

	// Options
	if !help.options.is_empty() {
		out.push_str(&section_heading("Options"));
		out.push_str("\n\n");
		let flag_width = help
			.options
			.iter()
			.map(|(f, t, _)| format!("{f} {t}").len())
			.max()
			.unwrap_or(20);
		for (flag, type_name, desc) in help.options {
			let flag_part = paint(flag, flag_style());
			let type_part = if type_name.is_empty() {
				String::new()
			} else {
				format!(" {}", paint(type_name, value_style()))
			};
			let padded_len = format!("{flag} {type_name}").len();
			let padding = flag_width.saturating_sub(padded_len);
			out.push_str(&format!(
				"  {flag_part}{type_part}{}  {}\n",
				" ".repeat(padding),
				paint(desc, muted()),
			));
		}
		out.push('\n');
	}

	// Examples
	if !help.examples.is_empty() {
		out.push_str(&section_heading("Examples"));
		out.push_str("\n\n");
		for (desc, cmd) in help.examples {
			out.push_str(&example_block(desc, cmd));
			out.push_str("\n\n");
		}
	}

	// Tips
	if !help.tips.is_empty() {
		out.push_str(&section_heading("Tips"));
		out.push_str("\n\n");
		for tip in help.tips {
			out.push_str(&format!(
				"  {} {}\n",
				paint("•", accent()),
				multiline_indent(tip, 4),
			));
		}
		out.push('\n');
	}

	// See also
	if !help.see_also.is_empty() {
		out.push_str(&section_heading("See Also"));
		out.push_str("\n\n");
		let linked: Vec<String> = help
			.see_also
			.iter()
			.map(|name| paint(&format!("mc help {name}"), accent()))
			.collect();
		out.push_str(&format!("  {}\n", linked.join("  ")));
	}

	out
}

fn render_unknown_command_help(
	bin_name: &str,
	command_name: &str,
	helps: &[CommandHelp],
) -> String {
	let mut out = String::new();
	out.push_str(&format!(
		"{} Unknown command `{}`\n\n",
		paint("error:", error_style()),
		paint(command_name, flag_style()),
	));
	out.push_str(&format!(
		"  Run {} to see available commands.\n",
		paint(&format!("{bin_name} help"), accent()),
	));

	let name_width = helps.iter().map(|h| h.name.len()).max().unwrap_or(20);
	for help in helps {
		if help.name == command_name {
			continue;
		}
		let padded = format!("{:width$}", help.name, width = name_width);
		out.push_str(&format!(
			"  {}  {}\n",
			paint(&padded, flag_style()),
			paint(help.summary, muted()),
		));
	}

	out
}

/// Indent continuation lines for a multi-line tip string.
fn multiline_indent(text: &str, indent: usize) -> String {
	let prefix = " ".repeat(indent);
	text.lines()
		.enumerate()
		.map(|(i, line)| {
			if i == 0 {
				line.to_string()
			} else {
				format!("{prefix}{line}")
			}
		})
		.collect::<Vec<_>>()
		.join("\n")
}

#[cfg(test)]
mod tests {
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
		let helps = builtin_command_helps();
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
}
