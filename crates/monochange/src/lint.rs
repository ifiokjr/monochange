//! Check command implementation for monochange CLI.
//!
//! `mc check` combines workspace validation with manifest lint enforcement.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use clap::ArgMatches;
use monochange_config::load_workspace_configuration;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintReport;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_lint::LintSelection;
use monochange_lint::Linter;

use crate::OutputFormat;

#[allow(clippy::vec_init_then_push)]
fn lint_suites() -> Vec<Box<dyn LintSuite>> {
	let mut suites: Vec<Box<dyn LintSuite>> = Vec::new();
	#[cfg(feature = "cargo")]
	suites.push(Box::new(monochange_cargo::lints::lint_suite()));
	#[cfg(feature = "npm")]
	suites.push(Box::new(monochange_npm::lints::lint_suite()));
	suites
}

fn build_linter(
	configuration: &monochange_core::WorkspaceConfiguration,
	selection: LintSelection,
) -> Linter {
	Linter::new(lint_suites(), configuration.lints.clone()).with_selection(selection)
}

pub(crate) fn available_lint_rules() -> Vec<LintRule> {
	let mut rules = Linter::new(
		lint_suites(),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.rules();
	rules.sort_by(|left, right| left.id.cmp(&right.id));
	rules
}

pub(crate) fn available_lint_presets() -> Vec<LintPreset> {
	let mut presets = Linter::new(
		lint_suites(),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.presets();
	presets.sort_by(|left, right| left.id.cmp(&right.id));
	presets
}

pub(crate) fn explain_lint_rule(rule_id: &str) -> Option<LintRule> {
	Linter::new(
		lint_suites(),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.find_rule(rule_id)
}

pub(crate) fn explain_lint_preset(preset_id: &str) -> Option<LintPreset> {
	Linter::new(
		lint_suites(),
		monochange_core::lint::WorkspaceLintSettings::default(),
	)
	.registry()
	.find_preset(preset_id)
}

/// Run the check command (validate + lint).
pub(crate) fn run_check_command(
	root: &Path,
	fix: bool,
	ecosystems: &[String],
	only_rules: &[String],
	format: OutputFormat,
) -> MonochangeResult<String> {
	let configuration = load_workspace_configuration(root)?;
	let mut output = String::new();

	monochange_config::validate_workspace(root)?;
	monochange_config::validate_versioned_files_content(root)?;
	let _ = write!(output, "workspace validation passed for {}", root.display());

	let selection = LintSelection::all()
		.with_suites(ecosystems.iter().cloned())
		.with_rules(only_rules.iter().cloned());
	let linter = build_linter(&configuration, selection);
	let report = linter.lint_workspace(root, &configuration);

	if fix {
		let fixes = linter.apply_fixes(&report);
		for (file_path, fixed_content) in fixes {
			std::fs::write(&file_path, fixed_content).map_err(|error| {
				MonochangeError::Io(format!(
					"Failed to write fixed content to {}: {}",
					file_path.display(),
					error
				))
			})?;
		}
	}

	let lint_has_errors = report.has_errors();
	match format {
		OutputFormat::Json => {
			Ok(serde_json::to_string_pretty(&report)
				.unwrap_or_else(|error| panic!("serializing lint reports should succeed: {error}")))
		}
		OutputFormat::Text | OutputFormat::Markdown => {
			output.push_str(&format_check_report(&report, fix));
			if lint_has_errors {
				Err(MonochangeError::Config(
					"lint errors found during check".to_string(),
				))
			} else {
				Ok(output)
			}
		}
	}
}

/// Run lint as part of a Validate step. Returns (`formatted_output`, `has_errors`).
pub(crate) fn run_lint_step(root: &Path, fix: bool) -> MonochangeResult<(String, bool)> {
	let configuration = load_workspace_configuration(root)?;
	let linter = build_linter(&configuration, LintSelection::all());
	let report = linter.lint_workspace(root, &configuration);
	let has_errors = report.has_errors();

	if fix {
		let fixes = linter.apply_fixes(&report);
		for (file_path, fixed_content) in fixes {
			std::fs::write(&file_path, fixed_content).map_err(|error| {
				MonochangeError::Io(format!(
					"Failed to write fixed content to {}: {}",
					file_path.display(),
					error
				))
			})?;
		}
	}

	Ok((format_check_report(&report, fix), has_errors))
}

#[allow(clippy::unnecessary_wraps)]
pub(crate) fn render_lint_catalog(format: OutputFormat) -> MonochangeResult<String> {
	let rules = available_lint_rules();
	let presets = available_lint_presets();
	match format {
		OutputFormat::Json => {
			Ok(serde_json::to_string_pretty(&serde_json::json!({
				"rules": rules,
				"presets": presets,
			}))
			.unwrap_or_else(|error| panic!("serializing lint catalog should succeed: {error}")))
		}
		OutputFormat::Text | OutputFormat::Markdown => {
			let mut output = String::new();
			output.push_str("Rules:\n");
			for rule in rules {
				let _ = writeln!(
					output,
					"- {} [{:?} {:?}]{}",
					rule.id,
					rule.category,
					rule.maturity,
					if rule.autofixable { " [fixable]" } else { "" }
				);
				let _ = writeln!(output, "  {}", rule.description);
			}
			output.push_str("\nPresets:\n");
			for preset in presets {
				let _ = writeln!(output, "- {} [{:?}]", preset.id, preset.maturity);
				let _ = writeln!(output, "  {}", preset.description);
			}
			Ok(output)
		}
	}
}

pub(crate) fn render_lint_explanation(id: &str, format: OutputFormat) -> MonochangeResult<String> {
	if let Some(rule) = explain_lint_rule(id) {
		return match format {
			OutputFormat::Json => {
				Ok(serde_json::to_string_pretty(&rule).unwrap_or_else(|error| {
					panic!("serializing lint rule explanations should succeed: {error}")
				}))
			}
			OutputFormat::Text | OutputFormat::Markdown => {
				let mut output = String::new();
				let _ = writeln!(output, "{}", rule.id);
				let _ = writeln!(output, "name: {}", rule.name);
				let _ = writeln!(output, "category: {:?}", rule.category);
				let _ = writeln!(output, "maturity: {:?}", rule.maturity);
				let _ = writeln!(output, "autofixable: {}", rule.autofixable);
				let _ = writeln!(output, "\n{}", rule.description);
				for (index, option) in rule.options.into_iter().enumerate() {
					if index == 0 {
						output.push_str("\nOptions:\n");
					}
					let _ = writeln!(output, "- {} ({:?})", option.name, option.kind);
					let _ = writeln!(output, "  {}", option.description);
				}
				Ok(output)
			}
		};
	}

	if let Some(preset) = explain_lint_preset(id) {
		return match format {
			OutputFormat::Json => {
				Ok(
					serde_json::to_string_pretty(&preset).unwrap_or_else(|error| {
						panic!("serializing lint preset explanations should succeed: {error}")
					}),
				)
			}
			OutputFormat::Text | OutputFormat::Markdown => {
				let mut output = String::new();
				let _ = writeln!(output, "{}", preset.id);
				let _ = writeln!(output, "name: {}", preset.name);
				let _ = writeln!(output, "maturity: {:?}", preset.maturity);
				let _ = writeln!(output, "\n{}", preset.description);
				output.push_str("\nRules:\n");
				for (rule_id, config) in preset.rules {
					let _ = writeln!(output, "- {} = {}", rule_id, config.severity());
				}
				Ok(output)
			}
		};
	}

	Err(MonochangeError::Config(format!(
		"unknown lint rule or preset `{id}`"
	)))
}

pub(crate) fn handle_lint_subcommand(
	root: &Path,
	lint_matches: &ArgMatches,
) -> MonochangeResult<String> {
	let (subcommand, subcommand_matches) = lint_matches
		.subcommand()
		.expect("clap requires a lint subcommand");

	if subcommand == "list" {
		let format = if subcommand_matches
			.get_one::<String>("format")
			.is_some_and(|value| value == "json")
		{
			OutputFormat::Json
		} else {
			OutputFormat::Text
		};
		return render_lint_catalog(format);
	}

	if subcommand == "explain" {
		let format = if subcommand_matches
			.get_one::<String>("format")
			.is_some_and(|value| value == "json")
		{
			OutputFormat::Json
		} else {
			OutputFormat::Text
		};
		let id = subcommand_matches
			.get_one::<String>("id")
			.expect("clap requires a lint id")
			.as_str();
		return render_lint_explanation(id, format);
	}

	let id = subcommand_matches
		.get_one::<String>("id")
		.expect("clap requires a lint id")
		.as_str();
	scaffold_lint_rule(root, id)
}

pub(crate) fn scaffold_lint_rule(root: &Path, id: &str) -> MonochangeResult<String> {
	let (suite, rule_name) = id.split_once('/').ok_or_else(|| {
		MonochangeError::Config("lint ids must use the form <ecosystem>/<rule-name>".to_string())
	})?;
	let crate_name = match suite {
		"cargo" => "monochange_cargo",
		"npm" => "monochange_npm",
		other => {
			return Err(MonochangeError::Config(format!(
				"scaffolding is not yet supported for lint suite `{other}`"
			)));
		}
	};
	let module_name = rule_name.replace('-', "_");
	let struct_name = rule_name
		.split('-')
		.map(|segment| {
			let mut chars = segment.chars();
			match chars.next() {
				Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
				None => String::new(),
			}
		})
		.collect::<String>()
		+ "Rule";

	let lint_dir = root.join("crates").join(crate_name).join("src/lints");
	std::fs::create_dir_all(&lint_dir).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create lint directory {}: {error}",
			lint_dir.display()
		))
	})?;
	let lint_file = lint_dir.join(format!("{module_name}.rs"));
	if lint_file.exists() {
		return Err(MonochangeError::Config(format!(
			"lint file {} already exists",
			lint_file.display()
		)));
	}

	let fixture_dir = root
		.join("fixtures/tests/lints")
		.join(suite)
		.join(rule_name)
		.join("workspace");
	std::fs::create_dir_all(&fixture_dir).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create lint fixture directory {}: {error}",
			fixture_dir.display()
		))
	})?;

	let template = format!(
		r#"use monochange_core::lint::LintContext;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_linting::declare_lint_rule;
use monochange_linting::LintCategory;
use monochange_linting::LintMaturity;


declare_lint_rule! {{
    pub {struct_name},
    id: "{suite}/{rule_name}",
    name: "TODO: rename me",
    description: "TODO: describe what this lint checks",
    category: LintCategory::BestPractice,
    maturity: LintMaturity::Experimental,
    autofixable: false,
}}

impl LintRuleRunner for {struct_name} {{
    fn rule(&self) -> &LintRule {{
        &self.rule
    }}

    fn run(&self, _ctx: &LintContext<'_>, _config: &LintRuleConfig) -> Vec<LintResult> {{
        Vec::new()
    }}
}}
"#
	);
	std::fs::write(&lint_file, template).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write lint file {}: {error}",
			lint_file.display()
		))
	})?;

	let note_file = fixture_dir.join("README.md");
	std::fs::write(
		&note_file,
		format!("# {id}\n\nAdd fixture workspaces for snapshot and autofix tests here.\n"),
	)
	.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write fixture note {}: {error}",
			note_file.display()
		))
	})?;

	Ok(format!(
		"Created {} and {}.\nNext steps:\n- wire `mod {module_name};` into `crates/{crate_name}/src/lints/mod.rs`\n- register `{struct_name}::new()` in the suite\n- add fixture scenarios under {}",
		lint_file.display(),
		note_file.display(),
		fixture_dir.display()
	))
}

fn format_check_report(report: &LintReport, fixed: bool) -> String {
	if report.results.is_empty() && report.warnings.is_empty() {
		return "lint: no issues found\n".to_string();
	}

	let mut output = String::new();
	let _ = write!(
		output,
		"lint: {} errors, {} warnings\n\n",
		report.error_count, report.warning_count
	);

	for warning in &report.warnings {
		let _ = writeln!(output, "warning: {warning}");
	}
	if !report.warnings.is_empty() {
		output.push('\n');
	}

	let mut by_file: BTreeMap<&Path, Vec<&monochange_core::lint::LintResult>> = BTreeMap::new();
	for result in &report.results {
		by_file
			.entry(&result.location.file_path)
			.or_default()
			.push(result);
	}

	for (file, results) in by_file {
		let _ = writeln!(output, "{}:", file.display());
		for result in results {
			let severity_icon = match result.severity {
				LintSeverity::Error => "✗",
				LintSeverity::Warning => "⚠",
				LintSeverity::Off => "·",
			};
			let fix_indicator = if result.fix.is_some() {
				" [fixable]"
			} else {
				""
			};
			let _ = writeln!(
				output,
				"  {} {} at {}:{}{}",
				severity_icon,
				result.message,
				result.location.line,
				result.location.column,
				fix_indicator
			);
		}
		output.push('\n');
	}

	if fixed {
		output.push_str("Fixed all auto-fixable issues.\n");
	} else if report.autofixable().is_empty() {
		output.push_str("No auto-fixable issues found.\n");
	} else {
		let _ = writeln!(
			output,
			"{} issue(s) can be auto-fixed. Run with --fix to apply.",
			report.autofixable().len()
		);
	}

	output
}

#[cfg(test)]
mod tests {
	use std::fs;

	use super::*;
	use crate::cli::build_lint_subcommand;

	fn lint_settings_root() -> std::path::PathBuf {
		monochange_test_helpers::fixture_path!("config/lint-settings")
	}

	fn clean_lint_workspace() -> tempfile::TempDir {
		monochange_test_helpers::setup_scenario_workspace!("lint-check/clean")
	}

	fn readonly_fix_workspace() -> tempfile::TempDir {
		monochange_test_helpers::setup_scenario_workspace!("lint-check/read-only-fix")
	}

	#[test]
	fn test_format_check_report_empty() {
		let report = LintReport::new();
		let output = format_check_report(&report, false);
		assert!(output.contains("no issues found"));
	}

	#[test]
	fn test_format_check_report_with_issues() {
		let mut report = LintReport::new();
		report.add(monochange_core::lint::LintResult::new(
			"test/rule",
			monochange_core::lint::LintLocation::new("test.toml", 1, 1),
			"Test error",
			LintSeverity::Error,
		));
		report.add(monochange_core::lint::LintResult::new(
			"test/rule",
			monochange_core::lint::LintLocation::new("test.toml", 2, 1),
			"Test warning",
			LintSeverity::Warning,
		));

		let output = format_check_report(&report, false);
		assert!(output.contains("1 errors, 1 warnings"));
		assert!(output.contains("Test error"));
		assert!(output.contains("Test warning"));
	}

	#[test]
	fn render_lint_catalog_lists_rules_and_presets() {
		let text = render_lint_catalog(OutputFormat::Text).unwrap();
		assert!(text.contains("cargo/internal-dependency-workspace"));
		assert!(text.contains("cargo/recommended"));

		let json = render_lint_catalog(OutputFormat::Json).unwrap();
		assert!(json.contains("\"rules\""));
		assert!(json.contains("\"presets\""));
	}

	#[test]
	fn render_lint_explanation_supports_rules_and_presets() {
		let rule =
			render_lint_explanation("cargo/internal-dependency-workspace", OutputFormat::Text)
				.unwrap();
		assert!(rule.contains("Internal dependency workspace"));

		let preset = render_lint_explanation("cargo/recommended", OutputFormat::Text).unwrap();
		assert!(preset.contains("Cargo recommended"));
		assert!(preset.contains("cargo/sorted-dependencies"));
	}

	#[test]
	fn render_lint_explanation_rejects_unknown_ids() {
		let error = render_lint_explanation("unknown/rule", OutputFormat::Text)
			.expect_err("expected unknown lint explanation error");
		assert!(error.to_string().contains("unknown lint rule or preset"));
	}

	#[test]
	fn render_lint_explanation_supports_json() {
		let rule =
			render_lint_explanation("cargo/internal-dependency-workspace", OutputFormat::Json)
				.unwrap();
		assert!(rule.contains("\"cargo/internal-dependency-workspace\""));

		let preset = render_lint_explanation("cargo/recommended", OutputFormat::Json).unwrap();
		assert!(preset.contains("\"cargo/recommended\""));
	}

	#[test]
	fn run_check_command_supports_text_json_and_fix_error_paths() {
		let clean_workspace = clean_lint_workspace();
		let json =
			run_check_command(clean_workspace.path(), false, &[], &[], OutputFormat::Json).unwrap();
		assert!(json.contains("\"error_count\": 0"));

		let text =
			run_check_command(clean_workspace.path(), false, &[], &[], OutputFormat::Text).unwrap();
		assert!(text.contains("workspace validation passed"));

		let tempdir = readonly_fix_workspace();
		let cargo_toml = tempdir.path().join("crates/example/Cargo.toml");
		let mut permissions = fs::metadata(&cargo_toml).unwrap().permissions();
		permissions.set_readonly(true);
		fs::set_permissions(&cargo_toml, permissions).unwrap();
		let error = run_check_command(tempdir.path(), true, &[], &[], OutputFormat::Text)
			.expect_err("expected fix write to fail for readonly manifest");
		assert!(error.to_string().contains("Failed to write fixed content"));
	}

	#[test]
	fn run_lint_step_supports_fix_write_failures() {
		let tempdir = readonly_fix_workspace();
		let cargo_toml = tempdir.path().join("crates/example/Cargo.toml");
		let mut permissions = fs::metadata(&cargo_toml).unwrap().permissions();
		permissions.set_readonly(true);
		fs::set_permissions(&cargo_toml, permissions).unwrap();
		let error =
			run_lint_step(tempdir.path(), true).expect_err("expected lint-step fix write failure");
		assert!(error.to_string().contains("Failed to write fixed content"));
	}

	#[test]
	fn handle_lint_subcommand_dispatches_supported_commands() {
		let root = lint_settings_root();
		let list_matches = build_lint_subcommand()
			.try_get_matches_from(["lint", "list"])
			.unwrap();
		let list_output = handle_lint_subcommand(&root, &list_matches).unwrap();
		assert!(list_output.contains("Rules:"));

		let explain_matches = build_lint_subcommand()
			.try_get_matches_from(["lint", "explain", "cargo/recommended", "--format=json"])
			.unwrap();
		let explain_output = handle_lint_subcommand(&root, &explain_matches).unwrap();
		assert!(explain_output.contains("\"cargo/recommended\""));
	}

	#[test]
	fn scaffold_lint_rule_validates_ids_and_creates_expected_files() {
		let error = scaffold_lint_rule(Path::new("."), "missing-dash")
			.expect_err("expected invalid lint id to fail");
		assert!(error.to_string().contains("<ecosystem>/<rule-name>"));

		let unsupported = scaffold_lint_rule(Path::new("."), "dart/no-foo")
			.expect_err("expected unsupported suite to fail");
		assert!(unsupported.to_string().contains("not yet supported"));

		let tempdir =
			monochange_test_helpers::setup_scenario_workspace!("test-support/scenario-workspace");
		let message = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies").unwrap();
		assert!(message.contains("no_path_dependencies.rs"));
		assert!(
			tempdir
				.path()
				.join("crates/monochange_cargo/src/lints/no_path_dependencies.rs")
				.exists()
		);
		assert!(
			tempdir
				.path()
				.join("fixtures/tests/lints/cargo/no-path-dependencies/workspace/README.md")
				.exists()
		);

		let odd_message = scaffold_lint_rule(tempdir.path(), "cargo/-foo").unwrap();
		assert!(odd_message.contains("_foo.rs"));
		let odd_file = tempdir
			.path()
			.join("crates/monochange_cargo/src/lints/_foo.rs");
		assert!(
			fs::read_to_string(odd_file)
				.unwrap()
				.contains("pub FooRule")
		);
	}

	#[test]
	fn scaffold_lint_rule_reports_directory_and_file_conflicts() {
		let tempdir = tempfile::tempdir().unwrap();
		let lint_dir = tempdir.path().join("crates/monochange_cargo/src/lints");
		fs::create_dir_all(lint_dir.parent().unwrap()).unwrap();
		fs::write(&lint_dir, "not a directory").unwrap();
		let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
			.expect_err("expected lint-dir creation to fail");
		assert!(
			error
				.to_string()
				.contains("failed to create lint directory")
		);

		let tempdir = tempfile::tempdir().unwrap();
		fs::create_dir_all(tempdir.path().join("crates/monochange_cargo/src/lints")).unwrap();
		fs::create_dir_all(
			tempdir
				.path()
				.join("fixtures/tests/lints/cargo/no-path-dependencies/workspace"),
		)
		.unwrap();
		fs::write(
			tempdir
				.path()
				.join("crates/monochange_cargo/src/lints/no_path_dependencies.rs"),
			"existing",
		)
		.unwrap();
		let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
			.expect_err("expected existing lint file to fail");
		assert!(error.to_string().contains("already exists"));

		let tempdir = tempfile::tempdir().unwrap();
		fs::create_dir_all(tempdir.path().join("crates/monochange_cargo/src/lints")).unwrap();
		fs::create_dir_all(
			tempdir
				.path()
				.join("fixtures/tests/lints/cargo/no-path-dependencies"),
		)
		.unwrap();
		fs::write(
			tempdir
				.path()
				.join("fixtures/tests/lints/cargo/no-path-dependencies/workspace"),
			"not a directory",
		)
		.unwrap();
		let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
			.expect_err("expected fixture-dir creation to fail");
		assert!(
			error
				.to_string()
				.contains("failed to create lint fixture directory")
		);
	}

	#[test]
	fn scaffold_lint_rule_reports_write_failures() {
		let tempdir = tempfile::tempdir().unwrap();
		let lint_dir = tempdir.path().join("crates/monochange_cargo/src/lints");
		fs::create_dir_all(&lint_dir).unwrap();
		let mut permissions = fs::metadata(&lint_dir).unwrap().permissions();
		permissions.set_readonly(true);
		fs::set_permissions(&lint_dir, permissions).unwrap();
		let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
			.expect_err("expected lint file write to fail");
		assert!(!error.to_string().is_empty());

		let tempdir = tempfile::tempdir().unwrap();
		let lint_dir = tempdir.path().join("crates/monochange_cargo/src/lints");
		let fixture_dir = tempdir
			.path()
			.join("fixtures/tests/lints/cargo/no-path-dependencies/workspace");
		fs::create_dir_all(&lint_dir).unwrap();
		fs::create_dir_all(&fixture_dir).unwrap();
		let mut permissions = fs::metadata(&fixture_dir).unwrap().permissions();
		permissions.set_readonly(true);
		fs::set_permissions(&fixture_dir, permissions).unwrap();
		let error = scaffold_lint_rule(tempdir.path(), "cargo/no-path-dependencies")
			.expect_err("expected fixture note write to fail");
		assert!(!error.to_string().is_empty());
	}
}
