//! `mc migrate knope` — translate a `knope.toml` to `monochange.toml`.
//!
//! This module reads a knope configuration, generates a monochange configuration
//! draft, converts existing changeset files from knope format, and optionally
//! generates GitHub Actions workflow files.

use std::fmt::Write as _;
use std::fs;
use std::path::Path;

use clap::ArgMatches;
use serde::{Deserialize, Serialize};

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

use crate::OutputFormat;
use crate::parse_output_format;

// ---------------------------------------------------------------------------
// Knope config types
// ---------------------------------------------------------------------------

/// Top-level shape of a `knope.toml`.
#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct KnopeConfig {
	/// Single-package declaration (knope `[package]` table).
	#[serde(default)]
	package: Option<KnopeSinglePackage>,
	/// Multi-package declarations (knope `[packages.<name>]` tables).
	#[serde(default)]
	packages: std::collections::HashMap<String, KnopePackageEntry>,
	/// GitHub integration.
	#[serde(default)]
	github: Option<KnopeGitHub>,
	/// Workflow definitions.
	#[serde(default)]
	workflows: Vec<KnopeWorkflow>,
	/// Conventional-commit settings.
	#[serde(default)]
	changes: Option<KnopeChanges>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KnopeSinglePackage {
	#[serde(default)]
	versioned_files: Vec<toml::Value>,
	#[serde(default)]
	changelog: Option<String>,
	#[serde(default)]
	scopes: Vec<String>,
	#[serde(default)]
	extra_changelog_sections: Vec<KnopeChangelogSection>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KnopePackageEntry {
	#[serde(default)]
	versioned_files: Vec<toml::Value>,
	#[serde(default)]
	changelog: Option<String>,
	#[serde(default)]
	scopes: Vec<String>,
	#[serde(default)]
	extra_changelog_sections: Vec<KnopeChangelogSection>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KnopeGitHub {
	owner: String,
	repo: String,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KnopeWorkflow {
	name: String,
	#[serde(default)]
	steps: Vec<KnopeStep>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KnopeStep {
	#[serde(rename = "type")]
	step_type: String,
	#[serde(default)]
	command: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub(crate) struct KnopeChanges {
	#[serde(default)]
	ignore_conventional_commits: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KnopeChangelogSection {
	name: String,
	#[serde(default)]
	types: Vec<String>,
}

// ---------------------------------------------------------------------------
// Migration report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct KnopeMigrationReport {
	dry_run: bool,
	config_translated: bool,
	changesets_scanned: usize,
	changesets_converted: usize,
	changesets_unchanged: usize,
	ci_generated: bool,
	messages: Vec<String>,
}

// ---------------------------------------------------------------------------
// Core logic
// ---------------------------------------------------------------------------

pub(crate) fn run_knope_migration(
	root: &Path,
	quiet: bool,
	knope_matches: &ArgMatches,
) -> MonochangeResult<String> {
	if quiet {
		return Ok(String::new());
	}

	let dry_run = knope_matches.get_flag("dry-run");
	let generate_ci = knope_matches.get_flag("ci");
	let format = knope_matches
		.get_one::<String>("format")
		.map_or(OutputFormat::Text, |v| parse_output_format(v).unwrap_or(OutputFormat::Text));

	let knope_path = root.join("knope.toml");
	if !knope_path.exists() {
		return Err(MonochangeError::Config(
			"no knope.toml found in the repository root".to_string(),
		));
	}

	let knope_text = fs::read_to_string(&knope_path).map_err(|error| {
		MonochangeError::Io(format!("read knope.toml: {error}"))
	})?;

	let knope_config: KnopeConfig = toml::from_str(&knope_text).map_err(|error| {
		MonochangeError::Config(format!("parse knope.toml: {error}"))
	})?;

	let mut messages = Vec::new();

	// 1. Translate config
	let monochange_toml = translate_knope_config(&knope_config, &mut messages);

	if !dry_run {
		fs::write(root.join("monochange.toml"), &monochange_toml).map_err(|error| {
			MonochangeError::Io(format!("write monochange.toml: {error}"))
		})?;
	}

	// 2. Convert changesets
	let (scanned, converted, unchanged) = convert_changesets(root, dry_run, &mut messages)?;

	// 3. Optionally generate CI
	let ci_generated = if generate_ci {
		let ci_files = generate_ci_workflows(&knope_config, &mut messages);
		if !dry_run {
			for (rel_path, content) in &ci_files {
				let full_path = root.join(rel_path);
				if let Some(parent) = full_path.parent() {
					fs::create_dir_all(parent).map_err(|error| {
						MonochangeError::Io(format!("create dir {}: {error}", parent.display()))
					})?;
				}
				fs::write(&full_path, content).map_err(|error| {
					MonochangeError::Io(format!("write {}: {error}", full_path.display()))
				})?;
			}
		}
		true
	} else {
		false
	};

	let report = KnopeMigrationReport {
		dry_run,
		config_translated: true,
		changesets_scanned: scanned,
		changesets_converted: converted,
		changesets_unchanged: unchanged,
		ci_generated,
		messages,
	};

	Ok(render_knope_migration_report(&report, format))
}

fn translate_knope_config(config: &KnopeConfig, messages: &mut Vec<String>) -> String {
	let mut out = String::new();

	// Defaults
	if let Some(ref pkg) = config.package {
		let eco = detect_ecosystem_from_versioned_files(&pkg.versioned_files);
		let _ = writeln!(out, "[defaults]");
		let _ = writeln!(out, "package_type = \"{eco}\"");
		let _ = writeln!(out, "parent_bump = \"patch\"");
		let _ = writeln!(out);

		if let Some(ref changelog) = pkg.changelog {
			let _ = writeln!(out, "[defaults.changelog]");
			let _ = writeln!(out, "path = \"{{ path }}/{changelog}\"");
			let _ = writeln!(out, "format = \"keep_a_changelog\"");
			let _ = writeln!(out);
		}
	} else if !config.packages.is_empty() {
		// Detect from first package
		let first = config.packages.values().next();
		if let Some(pkg) = first {
			let eco = detect_ecosystem_from_versioned_files(&pkg.versioned_files);
			let _ = writeln!(out, "[defaults]");
			let _ = writeln!(out, "package_type = \"{eco}\"");
			let _ = writeln!(out, "parent_bump = \"patch\"");
			let _ = writeln!(out);
		}
	}

	// Source
	if let Some(ref gh) = config.github {
		let _ = writeln!(out, "[source]");
		let _ = writeln!(out, "provider = \"github\"");
		let _ = writeln!(out, "owner = \"{}\"", gh.owner);
		let _ = writeln!(out, "repo = \"{}\"", gh.repo);
		let _ = writeln!(out);
	}

	// Packages
	let mut package_ids: Vec<String> = Vec::new();
	if let Some(ref pkg) = config.package {
		// Single-package knope config — derive from versioned files
		let ids = derive_package_ids_from_versioned_files(&pkg.versioned_files);
		for id in &ids {
			package_ids.push(id.clone());
			let _ = writeln!(out, "[package.{id}]");
			let _ = writeln!(out, "path = \"crates/{id}\"");
			let _ = writeln!(out);
		}
		messages.push(format!(
			"derived {} package(s) from single [package] table: {}",
			ids.len(),
			ids.join(", ")
		));
	}

	for (name, pkg) in &config.packages {
		package_ids.push(name.clone());
		let _ = writeln!(out, "[package.{name}]");
		let path = derive_path_from_versioned_files(&pkg.versioned_files);
		let _ = writeln!(out, "path = \"{path}\"");
		if !pkg.extra_changelog_sections.is_empty() {
			let _ = writeln!(out, "extra_changelog_sections = [");
			for section in &pkg.extra_changelog_sections {
				let _ = writeln!(
					out,
					"\t{{ name = \"{}\", types = {:?} }},",
					section.name, section.types
				);
			}
			let _ = writeln!(out, "]");
		}
		let _ = writeln!(out);
	}

	// Group
	if package_ids.len() > 1 {
		let _ = writeln!(out, "[group.main]");
		let _ = writeln!(out, "packages = [{}]", format_toml_array(&package_ids));
		let _ = writeln!(out, "tag = true");
		let _ = writeln!(out, "release = true");
		let _ = writeln!(out, "version_format = \"primary\"");
		let _ = writeln!(out);
	}

	// Workflows → CLI commands
	for workflow in &config.workflows {
		let _ = writeln!(out, "[cli.{}]", workflow.name);
		let _ = writeln!(out, "help_text = \"{} workflow\"", workflow.name);
		for step in &workflow.steps {
			match step.step_type.as_str() {
				"PrepareRelease" => {
					let _ = writeln!(out, "[[cli.{}.steps]]", workflow.name);
					let _ = writeln!(out, "type = \"PrepareRelease\"");
				}
				"Release" => {
					let _ = writeln!(out, "[[cli.{}.steps]]", workflow.name);
					let _ = writeln!(out, "type = \"PublishRelease\"");
				}
				"CreateChangeFile" => {
					let _ = writeln!(out, "[[cli.{}.steps]]", workflow.name);
					let _ = writeln!(out, "type = \"CreateChangeFile\"");
				}
				"Command" => {
					if let Some(ref cmd) = step.command {
						// Skip manual git steps — monochange handles git internally
						if cmd.starts_with("git ")
							&& (cmd.contains("add") || cmd.contains("commit") || cmd.contains("push"))
						{
							continue;
						}
						let _ = writeln!(out, "[[cli.{}.steps]]", workflow.name);
						let _ = writeln!(out, "type = \"Command\"");
						let _ = writeln!(out, "command = \"{cmd}\"");
					}
				}
				_ => {
					messages.push(format!(
						"skipping unknown knope step type: {}",
						step.step_type
					));
				}
			}
		}
		let _ = writeln!(out);
	}

	out
}

fn detect_ecosystem_from_versioned_files(files: &[toml::Value]) -> &'static str {
	for file in files {
		let s = file.as_str().unwrap_or("");
		if s.contains("Cargo.toml") {
			return "cargo";
		}
		if s.contains("package.json") {
			return "npm";
		}
		if s.contains("pubspec.yaml") {
			return "dart";
		}
	}
	"cargo"
}

fn derive_package_ids_from_versioned_files(files: &[toml::Value]) -> Vec<String> {
	let mut ids = Vec::new();
	for file in files {
		let s = if let Some(str_val) = file.as_str() {
			str_val.to_string()
		} else if let Some(map) = file.as_table() {
			map.get("dependency")
				.and_then(|v| v.as_str())
				.unwrap_or("")
				.to_string()
		} else {
			continue;
		};
		if !s.is_empty() && !ids.iter().any(|id| s.contains(id)) {
			// Try to extract a crate name from the path
			if let Some(name) = s.strip_suffix("/Cargo.toml") {
				if let Some(crate_name) = name.split('/').next_back() {
					if !ids.contains(&crate_name.to_string()) {
						ids.push(crate_name.to_string());
					}
				}
			}
		}
	}
	if ids.is_empty() {
		ids.push("main".to_string());
	}
	ids
}

fn derive_path_from_versioned_files(files: &[toml::Value]) -> String {
	for file in files {
		if let Some(s) = file.as_str() {
			if s.ends_with("/Cargo.toml") || s.ends_with("/package.json") || s.ends_with("/pubspec.yaml") {
				if let Some(parent) = s.rsplit_once('/') {
					return parent.0.to_string();
				}
			}
		}
	}
	".".to_string()
}

fn convert_changesets(
	root: &Path,
	dry_run: bool,
	messages: &mut Vec<String>,
) -> MonochangeResult<(usize, usize, usize)> {
	let changeset_dir = root.join(".changeset");
	if !changeset_dir.exists() {
		return Ok((0, 0, 0));
	}

	let mut scanned = 0usize;
	let mut converted = 0usize;
	let mut unchanged = 0usize;

	for entry in fs::read_dir(&changeset_dir).map_err(|error| {
		MonochangeError::Io(format!("read .changeset dir: {error}"))
	})? {
		let entry = entry.map_err(|error| {
			MonochangeError::Io(format!("read .changeset entry: {error}"))
		})?;
		let path = entry.path();
		if path.extension().and_then(|e| e.to_str()) != Some("md") {
			continue;
		}
		if path.file_name().and_then(|n| n.to_str()) == Some("README.md") {
			continue;
		}

		scanned += 1;
		let original = fs::read_to_string(&path).map_err(|error| {
			MonochangeError::Io(format!("read changeset {}: {error}", path.display()))
		})?;

		let migrated = convert_changeset_text(&original);
		if migrated == original {
			unchanged += 1;
		} else {
			converted += 1;
			if !dry_run {
				fs::write(&path, &migrated).map_err(|error| {
					MonochangeError::Io(format!("write changeset {}: {error}", path.display()))
				})?;
			}
		}
	}

	if converted > 0 {
		messages.push(format!(
			"converted {converted} changeset(s) ({unchanged} unchanged)"
		));
	}

	Ok((scanned, converted, unchanged))
}

/// Convert a single changeset file from knope format to monochange format.
///
/// Key differences:
/// - knope `default: minor` → monochange needs group or package IDs
/// - knope bodies may not start with `# heading` — monochange requires it
pub(crate) fn convert_changeset_text(text: &str) -> String {
	let Some((frontmatter, body)) = split_frontmatter(text) else {
		return text.to_string();
	};

	let mut new_frontmatter = frontmatter.clone();

	// Replace `default:` with `main:` (most common group ID)
	if new_frontmatter.contains("\ndefault:") || new_frontmatter.starts_with("default:") {
		new_frontmatter = new_frontmatter.replace("default:", "main:");
	}

	// Ensure body starts with a heading
	let trimmed_body = body.trim_start();
	let new_body = if trimmed_body.starts_with('#') {
		body.to_string()
	} else {
		let heading = extract_heading_from_frontmatter(&new_frontmatter);
		format!("# {heading}\n\n{body}")
	};

	format!("---\n{new_frontmatter}\n---\n\n{new_body}")
}

fn split_frontmatter(text: &str) -> Option<(String, String)> {
	let text = text.trim_start();
	if !text.starts_with("---") {
		return None;
	}
	let rest = text.strip_prefix("---")?;
	if let Some(end) = rest.find("\n---") {
		let frontmatter = &rest[..end];
		let body = rest[end + 4..].trim_start_matches('\n');
		Some((frontmatter.to_string(), body.to_string()))
	} else {
		None
	}
}

fn extract_heading_from_frontmatter(frontmatter: &str) -> String {
	// Try to extract a bump reason from the frontmatter values
	for line in frontmatter.lines() {
		let trimmed = line.trim();
		if trimmed.starts_with('#') || trimmed.is_empty() {
			continue;
		}
		if let Some((_key, bump)) = trimmed.split_once(':') {
			let bump = bump.trim();
			if ["minor", "patch", "major"].contains(&bump) {
				// Return a generic heading based on bump level
				return match bump {
					"major" => "Breaking change".to_string(),
					"minor" => "New feature".to_string(),
					"patch" => "Bug fix".to_string(),
					_ => "Release update".to_string(),
				};
			}
		}
	}
	"Release update".to_string()
}

fn generate_ci_workflows(
	config: &KnopeConfig,
	messages: &mut Vec<String>,
) -> Vec<(String, String)> {
	let mut files = Vec::new();

	// release.yml
	files.push((
		".github/workflows/release.yml".to_string(),
		generate_release_yml(config),
	));

	// publish.yml
	files.push((
		".github/workflows/publish.yml".to_string(),
		generate_publish_yml(config),
	));

	// changeset-policy.yml
	files.push((
		".github/workflows/changeset-policy.yml".to_string(),
		generate_changeset_policy_yml(),
	));

	// setup-mc action
	files.push((
		".github/actions/setup-mc/action.yml".to_string(),
		generate_setup_mc_action(),
	));

	messages.push("generated 4 CI workflow files".to_string());
	files
}

fn generate_release_yml(_config: &KnopeConfig) -> String {
	r#"name: release

on:
  push:
    branches: [main]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  release-pr:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: ./.github/actions/setup-mc
      - run: mc release-pr
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
"#.to_string()
}

fn generate_publish_yml(config: &KnopeConfig) -> String {
	let eco = if let Some(ref pkg) = config.package {
		detect_ecosystem_from_versioned_files(&pkg.versioned_files)
	} else if !config.packages.is_empty() {
		let first = config.packages.values().next().unwrap();
		detect_ecosystem_from_versioned_files(&first.versioned_files)
	} else {
		"cargo"
	};

	let publish_steps = match eco {
		"cargo" => r#"      - uses: rust-lang/crates-io-auth-action@v1
        id: crates-oidc
      - run: mc step:publish-readiness --from HEAD --format json
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}
      - run: mc step:publish-packages --all --format json
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}"#,
		"npm" => r#"      - uses: pnpm/action-setup@v6
        with:
          version: 10
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          registry-url: https://registry.npmjs.org
      - run: pnpm install --frozen-lockfile
      - run: pnpm -r publish --access public --no-git-checks
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
          NPM_CONFIG_PROVENANCE: true"#,
		"dart" => r#"      - uses: dart-lang/setup-dart@v1
        with:
          sdk: stable
      - run: dart pub get
      - run: melos publish --no-dry-run"#,
		_ => "      # TODO: add publish steps for your ecosystem",
	};

	format!(
		r#"name: publish

on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag to publish (e.g. v0.1.0)"
        required: true
        type: string

jobs:
  publish:
    environment: publisher
    permissions:
      contents: read
      id-token: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{{{ inputs.tag }}}}
          fetch-depth: 0
      - uses: ./.github/actions/setup-mc
{publish_steps}
"#
	)
}

fn generate_changeset_policy_yml() -> String {
	r#"name: changeset-policy

on:
  pull_request:
    types: [opened, synchronize, reopened, labeled, unlabeled]

jobs:
  check:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      issues: write
      pull-requests: read
    steps:
      - uses: actions/checkout@v6
      - uses: ./.github/actions/setup-mc
      - uses: tj-actions/changed-files@v46
        id: changed
      - name: run changeset policy
        env:
          CHANGED_FILES: ${{ steps.changed.outputs.all_changed_files }}
          PR_LABELS_JSON: ${{ toJson(github.event.pull_request.labels.*.name) }}
        run: |
          set -euo pipefail
          mapfile -t labels < <(jq -r '.[]' <<<"$PR_LABELS_JSON")
          args=(step:affected-packages --format json --verify)
          for path in $CHANGED_FILES; do args+=(--changed-paths "$path"); done
          for label in "${labels[@]}"; do args+=(--label "$label"); done
          mc "${args[@]}"
"#.to_string()
}

fn generate_setup_mc_action() -> String {
	r#"name: setup-mc
description: Install the monochange CLI
runs:
  using: composite
  steps:
    - name: install mc
      shell: bash
      run: |
        set -euo pipefail
        curl -fsSL https://get.monochange.dev/install.sh | sh -s -- -y
        mc --version
"#.to_string()
}

fn format_toml_array(items: &[String]) -> String {
	format!(
		"[{}]",
		items.iter().map(|s| format!("\"{s}\"")).collect::<Vec<_>>().join(", ")
	)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

fn render_knope_migration_report(report: &KnopeMigrationReport, format: OutputFormat) -> String {
	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(report).unwrap_or_default()
		}
		OutputFormat::Markdown | OutputFormat::Text => {
			text_knope_migration_report(report)
		}
	}
}

fn text_knope_migration_report(report: &KnopeMigrationReport) -> String {
	let mut out = String::new();
	let action = if report.dry_run { "would migrate" } else { "migrated" };
	let _ = writeln!(
		out,
		"knope migration: {action} config + {}/{} changeset(s)",
		report.changesets_converted,
		report.changesets_scanned,
	);
	let _ = writeln!(out, "  unchanged: {}", report.changesets_unchanged);
	if report.ci_generated {
		let ci_action = if report.dry_run { "would generate" } else { "generated" };
		let _ = writeln!(out, "  ci: {ci_action} 4 workflow files");
	}
	for msg in &report.messages {
		let _ = writeln!(out, "  - {msg}");
	}
	out.trim_end().to_string()
}

#[cfg(test)]
#[path = "__tests__/migrate_knope_tests.rs"]
mod tests;