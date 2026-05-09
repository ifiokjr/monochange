#![forbid(clippy::indexing_slicing)]

//! Testing helpers for monochange lint suites.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::lint::LintFix;
use monochange_core::lint::LintReport;
use monochange_lint::Linter;

/// Render a lint report in a stable snapshot-friendly format.
#[must_use]
pub fn format_report(report: &LintReport) -> String {
	let mut output = String::new();
	let _ = writeln!(
		output,
		"errors: {}\nwarnings: {}",
		report.error_count, report.warning_count
	);

	for warning in &report.warnings {
		let _ = writeln!(output, "warning: {warning}");
	}

	for result in &report.results {
		let fixable = if result.fix.is_some() { " fixable" } else { "" };
		let _ = writeln!(
			output,
			"{} {}:{}:{} {}{}",
			result.rule_id,
			result.location.file_path.display(),
			result.location.line,
			result.location.column,
			result.message,
			fixable,
		);
	}

	output
}

/// Render fixed file contents in a stable snapshot-friendly format.
#[must_use]
pub fn format_fixed_files(fixed_files: &BTreeMap<PathBuf, String>) -> String {
	let mut output = String::new();
	for (path, contents) in fixed_files {
		let _ = writeln!(output, "== {} ==", path.display());
		output.push_str(contents);
		if !contents.ends_with('\n') {
			output.push('\n');
		}
	}
	output
}

/// Apply fixes and format the resulting files.
#[must_use]
pub fn apply_and_format_fixes(linter: &Linter, report: &LintReport) -> String {
	format_fixed_files(&linter.apply_fixes(report))
}

/// Format a fix list for a single rule result.
#[must_use]
pub fn format_fix(fix: &LintFix) -> String {
	let mut output = String::new();
	let _ = writeln!(output, "{}", fix.description);
	for edit in &fix.edits {
		let _ = writeln!(
			output,
			"  {}..{} => {}",
			edit.span.0, edit.span.1, edit.replacement
		);
	}
	output
}

/// Normalize a path relative to a workspace root for snapshot output.
#[must_use]
pub fn relative_path(root: &Path, path: &Path) -> String {
	path.strip_prefix(root)
		.unwrap_or(path)
		.to_string_lossy()
		.to_string()
}

#[cfg(test)]
#[path = "__tests__/lib_tests.rs"]
mod tests;
