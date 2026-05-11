use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::lint::LintEdit;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;

use super::*;
use crate::fixture_path;

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
