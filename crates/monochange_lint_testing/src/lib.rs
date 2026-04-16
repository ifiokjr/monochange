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
mod tests {
	use std::collections::BTreeMap;
	use std::path::PathBuf;

	use monochange_core::lint::LintEdit;
	use monochange_core::lint::LintLocation;
	use monochange_core::lint::LintResult;
	use monochange_core::lint::LintSeverity;
	use monochange_core::lint::LintSuite;
	use monochange_core::lint::LintTarget;
	use monochange_test_helpers::fixture_path;

	use super::*;

	struct EmptySuite;

	impl LintSuite for EmptySuite {
		fn suite_id(&self) -> &'static str {
			"empty"
		}

		fn rules(&self) -> Vec<Box<dyn monochange_core::lint::LintRuleRunner>> {
			Vec::new()
		}

		fn collect_targets(
			&self,
			_workspace_root: &Path,
			_configuration: &monochange_core::WorkspaceConfiguration,
		) -> monochange_core::MonochangeResult<Vec<LintTarget>> {
			Ok(Vec::new())
		}
	}

	#[test]
	fn format_helpers_render_stable_output() {
		let mut report = LintReport::new();
		report.warn("be careful");
		report.add(LintResult::new(
			"example/rule",
			LintLocation::new("path/file.txt", 2, 4),
			"something happened",
			LintSeverity::Warning,
		));

		let formatted = format_report(&report);
		assert!(formatted.contains("errors: 0"));
		assert!(formatted.contains("warning: be careful"));
		assert!(formatted.contains("example/rule path/file.txt:2:4 something happened"));

		let fixed_files = BTreeMap::from([(PathBuf::from("a.txt"), "hello".to_string())]);
		let formatted_fixed_files = format_fixed_files(&fixed_files);
		assert!(formatted_fixed_files.contains("== a.txt =="));
		assert!(formatted_fixed_files.ends_with("hello\n"));

		let fix = LintFix {
			description: "rewrite".to_string(),
			edits: vec![LintEdit::new((1, 3), "abc")],
		};
		let formatted_fix = format_fix(&fix);
		assert!(formatted_fix.contains("rewrite"));
		assert!(formatted_fix.contains("1..3 => abc"));
	}

	#[test]
	fn apply_and_format_fixes_reads_fixture_files() {
		let root = fixture_path!("test-support/setup-fixture");
		let file_path = root.join("root.txt");
		let mut report = LintReport::new();
		report.add(
			LintResult::new(
				"example/rule",
				LintLocation::new(&file_path, 1, 1).with_span(0, 4),
				"rewrite root",
				LintSeverity::Error,
			)
			.with_fix(LintFix::single("replace root", (0, 4), "BASE")),
		);
		let linter = Linter::new(
			vec![Box::new(EmptySuite)],
			monochange_core::lint::WorkspaceLintSettings::default(),
		)
		.with_selection(monochange_lint::LintSelection::all().with_rules(Vec::<String>::new()));
		let output = apply_and_format_fixes(&linter, &report);
		assert!(output.contains("== "));
		assert!(output.contains("BASE"));
	}

	#[test]
	fn relative_path_prefers_root_relative_output() {
		let root = Path::new("/tmp/workspace");
		let path = Path::new("/tmp/workspace/crates/core/Cargo.toml");
		assert_eq!(relative_path(root, path), "crates/core/Cargo.toml");
	}
}
