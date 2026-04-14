//! Lint command implementation for monochange CLI.

use std::io::Write;
use std::path::Path;

use monochange_config::load_workspace_configuration;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::lint::LintReport;
use monochange_lint::Linter;

use crate::OutputFormat;

/// Run the lint command.
pub(crate) fn run_lint_command(
	root: &Path,
	fix: bool,
	ecosystems: &[String],
	format: OutputFormat,
) -> MonochangeResult<String> {
	// Load configuration
	let configuration = load_workspace_configuration(root)?;

	// Create linter with configuration
	let mut linter = Linter::new();

	// Set up ecosystem-specific lint configs
	if let Some(cargo_config) = configuration
		.ecosystems
		.get(&monochange_core::Ecosystem::Cargo)
	{
		linter.set_ecosystem_config(
			monochange_core::Ecosystem::Cargo,
			cargo_config.lints.clone(),
		);
	}
	if let Some(npm_config) = configuration
		.ecosystems
		.get(&monochange_core::Ecosystem::Npm)
	{
		linter.set_ecosystem_config(monochange_core::Ecosystem::Npm, npm_config.lints.clone());
	}
	if let Some(deno_config) = configuration
		.ecosystems
		.get(&monochange_core::Ecosystem::Deno)
	{
		linter.set_ecosystem_config(monochange_core::Ecosystem::Deno, deno_config.lints.clone());
	}
	if let Some(dart_config) = configuration
		.ecosystems
		.get(&monochange_core::Ecosystem::Dart)
	{
		linter.set_ecosystem_config(monochange_core::Ecosystem::Dart, dart_config.lints.clone());
	}

	// Run linting
	let report = linter.lint_workspace(root);

	// Apply fixes if requested
	if fix {
		let fixes = linter.apply_fixes(&report);
		for (file_path, fixed_content) in fixes {
			std::fs::write(&file_path, fixed_content).map_err(|e| {
				MonochangeError::Io(format!(
					"Failed to write fixed content to {}: {}",
					file_path.display(),
					e
				))
			})?;
		}
	}

	// Format output
	match format {
		OutputFormat::Json => {
			Ok(serde_json::to_string_pretty(&report).map_err(|e| {
				MonochangeError::Io(format!("Failed to serialize lint report: {}", e))
			})?)
		}
		OutputFormat::Text => Ok(format_lint_report(&report, fix)),
	}
}

/// Format lint report as human-readable text.
fn format_lint_report(report: &LintReport, fixed: bool) -> String {
	if report.results.is_empty() {
		return "✓ No linting issues found\n".to_string();
	}

	let mut output = String::new();
	output.push_str(&format!(
		"Lint Report: {} errors, {} warnings\n\n",
		report.error_count, report.warning_count
	));

	// Group results by file
	let mut by_file: std::collections::BTreeMap<
		&std::path::Path,
		Vec<&monochange_core::lint::LintResult>,
	> = std::collections::BTreeMap::new();
	for result in &report.results {
		by_file
			.entry(&result.location.file_path)
			.or_default()
			.push(result);
	}

	for (file, results) in by_file {
		output.push_str(&format!("{}:\n", file.display()));
		for result in results {
			let severity_icon = match result.severity {
				monochange_core::lint::LintSeverity::Error => "✗",
				monochange_core::lint::LintSeverity::Warning => "⚠",
				monochange_core::lint::LintSeverity::Off => "·",
			};
			let fix_indicator = if result.fix.is_some() {
				" [fixable]"
			} else {
				""
			};
			output.push_str(&format!(
				"  {} {} at {}:{}{}\n",
				severity_icon,
				result.message,
				result.location.line,
				result.location.column,
				fix_indicator
			));
		}
		output.push('\n');
	}

	if fixed {
		output.push_str("Fixed all auto-fixable issues.\n");
	} else if report.autofixable().is_empty() {
		output.push_str("No auto-fixable issues found.\n");
	} else {
		output.push_str(&format!(
			"{} issue(s) can be auto-fixed. Run with --fix to apply.\n",
			report.autofixable().len()
		));
	}

	output
}

#[cfg(test)]
mod tests {
	use std::io::Write;

	use tempfile::TempDir;

	use super::*;

	#[test]
	fn test_format_lint_report_empty() {
		let report = LintReport::new();
		let output = format_lint_report(&report, false);
		assert!(output.contains("No linting issues found"));
	}

	#[test]
	fn test_format_lint_report_with_issues() {
		let mut report = LintReport::new();
		report.add(monochange_core::lint::LintResult::new(
			"test/rule",
			monochange_core::lint::LintLocation::new("test.toml", 1, 1),
			"Test error",
			monochange_core::lint::LintSeverity::Error,
		));
		report.add(monochange_core::lint::LintResult::new(
			"test/rule",
			monochange_core::lint::LintLocation::new("test.toml", 2, 1),
			"Test warning",
			monochange_core::lint::LintSeverity::Warning,
		));

		let output = format_lint_report(&report, false);
		assert!(output.contains("1 errors, 1 warnings"));
		assert!(output.contains("Test error"));
		assert!(output.contains("Test warning"));
	}
}
