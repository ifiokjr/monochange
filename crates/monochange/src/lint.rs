//! Check command implementation for monochange CLI.
//!
//! `mc check` combines workspace validation with lint rule enforcement.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

use monochange_config::load_workspace_configuration;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::lint::LintReport;
use monochange_core::lint::LintSeverity;
use monochange_lint::Linter;

use crate::OutputFormat;

fn configure_linter(configuration: &monochange_core::WorkspaceConfiguration, linter: &mut Linter) {
	linter.set_ecosystem_config(
		monochange_core::Ecosystem::Cargo,
		configuration.cargo.lints.clone(),
	);
	linter.set_ecosystem_config(
		monochange_core::Ecosystem::Npm,
		configuration.npm.lints.clone(),
	);
	linter.set_ecosystem_config(
		monochange_core::Ecosystem::Deno,
		configuration.deno.lints.clone(),
	);
	linter.set_ecosystem_config(
		monochange_core::Ecosystem::Dart,
		configuration.dart.lints.clone(),
	);
}

/// Run the check command (validate + lint).
pub(crate) fn run_check_command(
	root: &Path,
	fix: bool,
	ecosystems: &[String],
	format: OutputFormat,
) -> MonochangeResult<String> {
	let _ = ecosystems;

	let configuration = load_workspace_configuration(root)?;

	let mut output = String::new();

	// Phase 1: Validation (same as `mc validate`)
	monochange_config::validate_workspace(root)?;
	monochange_config::validate_versioned_files_content(root)?;
	let _ = write!(output, "workspace validation passed for {}", root.display());

	// Phase 2: Lint rules
	let mut linter = Linter::new();
	configure_linter(&configuration, &mut linter);

	let report = linter.lint_workspace(root);

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

	let lint_has_errors = report.has_errors();

	match format {
		OutputFormat::Json => {
			let json_report = serde_json::to_string_pretty(&report).map_err(|e| {
				MonochangeError::Io(format!("Failed to serialize lint report: {e}"))
			})?;
			Ok(json_report)
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
	let mut linter = Linter::new();
	configure_linter(&configuration, &mut linter);

	let report = linter.lint_workspace(root);
	let has_errors = report.has_errors();

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

	Ok((format_check_report(&report, fix), has_errors))
}

/// Format lint report as human-readable text.
fn format_check_report(report: &LintReport, fixed: bool) -> String {
	if report.results.is_empty() {
		return "lint: no issues found\n".to_string();
	}

	let mut output = String::new();
	let _ = write!(
		output,
		"lint: {} errors, {} warnings\n\n",
		report.error_count, report.warning_count
	);

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
	use tempfile::TempDir;

	use super::*;

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
}
