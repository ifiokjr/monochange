use std::fs;
use std::path::PathBuf;

use super::*;
use crate::cli::build_lint_subcommand;

fn lint_settings_root() -> PathBuf {
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
	assert!(text.contains("dart/required-package-fields"));
	assert!(text.contains("dart/sdk-constraint-present"));
	assert!(text.contains("dart/internal-path-dependency-policy"));
	assert!(text.contains("dart/flutter-package-metadata-consistent"));
	assert!(text.contains("dart/recommended"));
	assert!(text.contains("dart/strict"));

	let json = render_lint_catalog(OutputFormat::Json).unwrap();
	assert!(json.contains("\"rules\""));
	assert!(json.contains("\"presets\""));
}

#[test]
fn render_lint_explanation_supports_rules_and_presets() {
	let rule =
		render_lint_explanation("cargo/internal-dependency-workspace", OutputFormat::Text).unwrap();
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
		render_lint_explanation("cargo/internal-dependency-workspace", OutputFormat::Json).unwrap();
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
fn run_check_command_reports_all_validation_errors_before_failing() {
	let tempdir = tempfile::tempdir()
		.unwrap_or_else(|error| panic!("expected tempdir to be created: {error}"));
	let root = tempdir.path();
	let crate_dir = root.join("crates/core");
	fs::create_dir_all(&crate_dir)
		.unwrap_or_else(|error| panic!("expected crate dir to be created: {error}"));
	fs::create_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("expected changeset dir to be created: {error}"));
	fs::write(
		root.join("monochange.toml"),
		"[package.core]\n\
		path = \"crates/core\"\n\
		type = \"cargo\"\n\
		versioned_files = [\n\
		  { path = \"missing.toml\", type = \"cargo\" },\n\
		  { path = \"missing/*.toml\", type = \"cargo\" },\n\
		]\n",
	)
	.unwrap_or_else(|error| panic!("expected config to be written: {error}"));
	fs::write(
		crate_dir.join("Cargo.toml"),
		"[package]\n\
		name = \"core\"\n\
		version = \"1.0.0\"\n\
		edition = \"2021\"\n\
		description = \"Test core package\"\n\
		license = \"MIT\"\n\
		repository = \"https://github.com/monochange/monochange\"\n",
	)
	.unwrap_or_else(|error| panic!("expected manifest to be written: {error}"));
	fs::write(
		root.join(".changeset/broken.md"),
		"---\nmissing: patch\n---\n\n# Broken target\n",
	)
	.unwrap_or_else(|error| panic!("expected changeset to be written: {error}"));

	let (warnings, errors) = collect_workspace_validation_issues(root);
	assert!(warnings.is_empty());
	assert!(
		errors
			.iter()
			.any(|error| error.contains("unknown package or group `missing`"))
	);
	assert!(
		errors
			.iter()
			.any(|error| error.contains("versioned file") && error.contains("missing.toml"))
	);

	let warning_tempdir = tempfile::tempdir()
		.unwrap_or_else(|error| panic!("expected warning tempdir to be created: {error}"));
	let warning_root = warning_tempdir.path();
	let warning_crate_dir = warning_root.join("crates/core");
	fs::create_dir_all(&warning_crate_dir)
		.unwrap_or_else(|error| panic!("expected warning crate dir to be created: {error}"));
	fs::write(
		warning_root.join("monochange.toml"),
		"[package.core]\n\
		path = \"crates/core\"\n\
		type = \"cargo\"\n\
		versioned_files = [{ path = \"missing/*.toml\", type = \"cargo\" }]\n",
	)
	.unwrap_or_else(|error| panic!("expected warning config to be written: {error}"));
	fs::write(
		warning_crate_dir.join("Cargo.toml"),
		"[package]\n\
		name = \"core\"\n\
		version = \"1.0.0\"\n\
		edition = \"2021\"\n\
		description = \"Test core package\"\n\
		license = \"MIT\"\n\
		repository = \"https://github.com/monochange/monochange\"\n",
	)
	.unwrap_or_else(|error| panic!("expected warning manifest to be written: {error}"));
	let warning_text = run_check_command(warning_root, false, &[], &[], OutputFormat::Text)
		.unwrap_or_else(|error| panic!("expected warning-only check to pass: {error}"));
	assert!(warning_text.contains("warning:"));
	assert!(warning_text.contains("matches no files"));

	let text_error = run_check_command(root, false, &[], &[], OutputFormat::Text)
		.expect_err("expected text check to fail validation");
	let text_message = text_error.to_string();
	assert!(text_message.contains("workspace validation failed"));
	assert!(text_message.contains("unknown package or group `missing`"));
	assert!(text_message.contains("missing.toml"));

	let json_error = run_check_command(root, false, &[], &[], OutputFormat::Json)
		.expect_err("expected json check to fail validation");
	let json_message = json_error.to_string();
	assert!(json_message.contains("check failed"));
	assert!(json_message.contains("workspace validation failed"));
}

#[test]
fn run_check_command_applies_fixes_and_reports_them() {
	let tempdir = readonly_fix_workspace();
	let output = run_check_command(tempdir.path(), true, &[], &[], OutputFormat::Text)
		.unwrap_or_else(|error| panic!("expected fixable lint workspace to succeed: {error}"));
	assert!(output.contains("Fixed all auto-fixable issues."));

	let manifest = fs::read_to_string(tempdir.path().join("crates/example/Cargo.toml"))
		.unwrap_or_else(|error| panic!("expected fixed manifest to be readable: {error}"));
	assert!(manifest.contains("publish = false"));
}

#[test]
fn run_check_command_applies_fixes_without_progress_reporter() {
	let tempdir = readonly_fix_workspace();
	let result = run_check_command(tempdir.path(), true, &[], &[], OutputFormat::Json);
	assert!(
		result.is_ok(),
		"expected fixable lint workspace to succeed without a reporter: {result:?}"
	);

	let manifest = fs::read_to_string(tempdir.path().join("crates/example/Cargo.toml"))
		.unwrap_or_else(|error| panic!("expected fixed manifest to be readable: {error}"));
	assert!(manifest.contains("publish = false"));
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
	let odd_contents = fs::read_to_string(odd_file).unwrap();
	assert!(odd_contents.contains("pub FooRule"));
	assert!(odd_contents.contains("Keep `declare_lint_rule!`"));

	let dart_message = scaffold_lint_rule(tempdir.path(), "dart/sdk-constraint-present").unwrap();
	assert!(dart_message.contains("sdk_constraint_present.rs"));
	assert!(
		tempdir
			.path()
			.join("crates/monochange_dart/src/lints/sdk_constraint_present.rs")
			.exists()
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
