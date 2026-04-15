//! Snapshot tests for lint rules.
//!
//! These tests verify that:
//! 1. Lint rules correctly identify issues
//! 2. Fixes are isolated to specific parts of files
//! 3. Unchanged parts of files remain exactly the same
//! 4. The autofix produces expected results

use std::collections::BTreeMap;
use std::fmt::Write;

use insta::assert_snapshot;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintSeverity;
use tempfile::TempDir;

use crate::Linter;

/// Helper to format a lint report for snapshot testing
fn format_report(report: &monochange_core::lint::LintReport) -> String {
	let mut output = String::new();
	let _ = write!(
		output,
		"Errors: {}, Warnings: {}\n\n",
		report.error_count, report.warning_count
	);

	for result in &report.results {
		let fix_info = if let Some(fix) = result.fix.as_ref() {
			format!(" [FIXABLE: {}]", fix.description)
		} else {
			String::new()
		};

		let _ = writeln!(
			output,
			"[{}] {}: {}{}",
			result.severity, result.rule_id, result.message, fix_info
		);
	}

	output
}

/// Helper to format file diff for snapshot testing
fn format_file_diff(original: &str, fixed: &str) -> String {
	let mut output = String::new();
	output.push_str("=== ORIGINAL ===\n");
	output.push_str(original);
	output.push_str("\n=== FIXED ===\n");
	output.push_str(fixed);
	output.push_str("\n=== END ===\n");
	output
}

mod cargo_lints {
	use super::*;

	#[test]
	fn dependency_field_order_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		// Fields in incorrect order: features before workspace
		let content = r#"[package]
name = "test"
version = "1.0.0"

[dependencies]
serde = { features = ["derive"], workspace = true }
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"cargo/dependency-field-order".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		// Snapshot the error report
		assert_snapshot!("dependency_field_order_report", format_report(&report));

		// Apply fixes and snapshot the diff
		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&cargo_toml) {
			assert_snapshot!(
				"dependency_field_order_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn internal_dependency_workspace_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		// Internal dependency without workspace = true
		let content = r#"[package]
name = "test"
version = "1.0.0"

[dependencies]
internal-crate = { path = "../internal-crate", version = "1.0.0" }
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"cargo/internal-dependency-workspace".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		assert_snapshot!(
			"internal_dependency_workspace_report",
			format_report(&report)
		);

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&cargo_toml) {
			assert_snapshot!(
				"internal_dependency_workspace_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn required_package_fields_error() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		// Missing required fields
		let content = r#"[package]
name = "test"
version = "1.0.0"
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"cargo/required-package-fields".to_string(),
			LintRuleConfig::Detailed {
				level: LintSeverity::Error,
				options: {
					let mut opts = BTreeMap::new();
					opts.insert(
						"fields".to_string(),
						serde_json::json!(["description", "license", "repository"]),
					);
					opts
				},
			},
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		assert_snapshot!("required_package_fields_report", format_report(&report));
	}

	#[test]
	fn sorted_dependencies_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		// Dependencies not in alphabetical order
		let content = r#"[package]
name = "test"
version = "1.0.0"

[dependencies]
zzzz = "1.0"
aaaa = "1.0"
mmmm = "1.0"
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"cargo/sorted-dependencies".to_string(),
			LintRuleConfig::Severity(LintSeverity::Warning),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		assert_snapshot!("sorted_dependencies_report", format_report(&report));

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&cargo_toml) {
			assert_snapshot!(
				"sorted_dependencies_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn unlisted_package_private_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		// Package not in monochange.toml and not marked private
		let content = r#"[package]
name = "unlisted-crate"
version = "1.0.0"
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"cargo/unlisted-package-private".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		assert_snapshot!("unlisted_package_private_report", format_report(&report));

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&cargo_toml) {
			assert_snapshot!(
				"unlisted_package_private_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn fix_preserves_comments_and_formatting() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		// File with comments and specific formatting that should be preserved
		let content = r#"# Top level comment
[package]
name = "test"
version = "1.0.0"

# Dependencies section comment
[dependencies]
# Internal dep comment
internal-crate = { path = "../internal-crate", version = "1.0.0" }

# External dep
serde = "1.0"
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"cargo/internal-dependency-workspace".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());
		let fixes = linter.apply_fixes(&report);

		if let Some(fixed_content) = fixes.get(&cargo_toml) {
			// Verify comments are preserved
			assert!(
				fixed_content.contains("# Top level comment"),
				"Top level comment should be preserved"
			);
			assert!(
				fixed_content.contains("# Dependencies section comment"),
				"Section comment should be preserved"
			);
			assert!(
				fixed_content.contains("# Internal dep comment"),
				"Inline comment should be preserved"
			);
			assert!(
				fixed_content.contains("# External dep"),
				"External dep comment should be preserved"
			);

			// Snapshot to show the full preserved formatting
			assert_snapshot!(
				"fix_preserves_formatting",
				format_file_diff(content, fixed_content)
			);
		} else {
			panic!("Expected fixes to be generated");
		}
	}
}

mod npm_lints {
	use super::*;

	#[test]
	fn workspace_protocol_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let package_json = dir.path().join("package.json");

		// Internal dependency without workspace: protocol
		let content = r#"{
  "name": "test",
  "version": "1.0.0",
  "dependencies": {
    "internal-pkg": "^1.0.0"
  }
}"#;

		std::fs::write(&package_json, content).unwrap();

		// Create a packages directory to simulate workspace
		std::fs::create_dir_all(dir.path().join("packages").join("internal-pkg")).unwrap();
		let internal_pkg = dir
			.path()
			.join("packages")
			.join("internal-pkg")
			.join("package.json");
		std::fs::write(
			&internal_pkg,
			r#"{"name": "internal-pkg", "version": "1.0.0"}"#,
		)
		.unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"npm/workspace-protocol".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Npm, config);

		let report = linter.lint_file(&package_json, dir.path());

		assert_snapshot!("workspace_protocol_report", format_report(&report));

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&package_json) {
			assert_snapshot!(
				"workspace_protocol_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn sorted_dependencies_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let package_json = dir.path().join("package.json");

		// Dependencies not sorted
		let content = r#"{
  "name": "test",
  "version": "1.0.0",
  "dependencies": {
    "zzzz": "^1.0.0",
    "aaaa": "^1.0.0",
    "mmmm": "^1.0.0"
  }
}"#;

		std::fs::write(&package_json, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"npm/sorted-dependencies".to_string(),
			LintRuleConfig::Severity(LintSeverity::Warning),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Npm, config);

		let report = linter.lint_file(&package_json, dir.path());

		assert_snapshot!("npm_sorted_dependencies_report", format_report(&report));

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&package_json) {
			assert_snapshot!(
				"npm_sorted_dependencies_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn root_no_prod_deps_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let package_json = dir.path().join("package.json");

		// Root package.json with production dependencies
		let content = r#"{
  "name": "root",
  "version": "1.0.0",
  "dependencies": {
    "some-dep": "^1.0.0"
  }
}"#;

		std::fs::write(&package_json, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"npm/root-no-prod-deps".to_string(),
			LintRuleConfig::Severity(LintSeverity::Warning),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Npm, config);

		let report = linter.lint_file(&package_json, dir.path());

		assert_snapshot!("root_no_prod_deps_report", format_report(&report));

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&package_json) {
			assert_snapshot!(
				"root_no_prod_deps_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn no_duplicate_dependencies_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let package_json = dir.path().join("package.json");

		// Same dependency in multiple sections
		let content = r#"{
  "name": "test",
  "version": "1.0.0",
  "dependencies": {
    "shared-dep": "^1.0.0"
  },
  "devDependencies": {
    "shared-dep": "^1.0.0"
  }
}"#;

		std::fs::write(&package_json, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"npm/no-duplicate-dependencies".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Npm, config);

		let report = linter.lint_file(&package_json, dir.path());

		assert_snapshot!("no_duplicate_deps_report", format_report(&report));

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&package_json) {
			assert_snapshot!(
				"no_duplicate_deps_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}

	#[test]
	fn unlisted_package_private_error_and_fix() {
		let dir = TempDir::new().unwrap();
		let package_json = dir.path().join("package.json");

		// Package not marked as private
		let content = r#"{
  "name": "unlisted-pkg",
  "version": "1.0.0"
}"#;

		std::fs::write(&package_json, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		config.insert(
			"npm/unlisted-package-private".to_string(),
			LintRuleConfig::Severity(LintSeverity::Error),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Npm, config);

		let report = linter.lint_file(&package_json, dir.path());

		assert_snapshot!(
			"npm_unlisted_package_private_report",
			format_report(&report)
		);

		let fixes = linter.apply_fixes(&report);
		if let Some(fixed_content) = fixes.get(&package_json) {
			assert_snapshot!(
				"npm_unlisted_package_private_diff",
				format_file_diff(content, fixed_content)
			);
		}
	}
}

mod config_tests {
	use super::*;

	#[test]
	fn severity_off_disables_rule() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		let content = r#"[package]
name = "test"
version = "1.0.0"

[dependencies]
serde = { features = ["derive"], workspace = true }
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		// Rule is disabled
		config.insert(
			"cargo/dependency-field-order".to_string(),
			LintRuleConfig::Severity(LintSeverity::Off),
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		let field_order_results: Vec<_> = report
			.results
			.iter()
			.filter(|r| r.rule_id == "cargo/dependency-field-order")
			.collect();
		assert!(
			field_order_results.is_empty(),
			"Rule should be disabled when severity is Off"
		);
	}

	#[test]
	fn detailed_config_with_options() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		let content = r#"[package]
name = "test"
version = "1.0.0"
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let mut linter = Linter::new();
		let mut config = BTreeMap::new();
		// Only require description, not license/repository
		config.insert(
			"cargo/required-package-fields".to_string(),
			LintRuleConfig::Detailed {
				level: LintSeverity::Error,
				options: {
					let mut opts = BTreeMap::new();
					opts.insert("fields".to_string(), serde_json::json!(["description"]));
					opts
				},
			},
		);
		linter.set_ecosystem_config(monochange_core::Ecosystem::Cargo, config);

		let report = linter.lint_file(&cargo_toml, dir.path());

		let required_fields_results: Vec<_> = report
			.results
			.iter()
			.filter(|r| r.rule_id == "cargo/required-package-fields")
			.collect();
		assert_eq!(required_fields_results.len(), 1);
		assert!(
			required_fields_results
				.first()
				.is_some_and(|r| r.message.contains("description"))
		);
	}
}
