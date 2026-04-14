#![forbid(clippy::indexing_slicing)]

//! # `monochange_lint`
//!
//! Ecosystem-specific lint rules for monochange.
//!
//! This crate provides linting capabilities for monorepo package manifests
//! across multiple ecosystems (Cargo, npm, Deno, Dart).

use std::collections::BTreeMap;
use std::path::Path;

use monochange_core::Ecosystem;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintReport;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRegistry;
use monochange_core::lint::LintSeverity;

mod cargo;
mod npm;

pub use cargo::CargoLintRules;
pub use npm::NpmLintRules;

/// A linter that can run lint rules across package manifests.
#[derive(Debug)]
pub struct Linter {
	registry: LintRuleRegistry,
	config: BTreeMap<String, BTreeMap<String, LintRuleConfig>>,
}

impl Default for Linter {
	fn default() -> Self {
		Self::new()
	}
}

impl Linter {
	/// Create a new linter with default rules.
	#[must_use]
	pub fn new() -> Self {
		let mut registry = LintRuleRegistry::new();

		// Register cargo rules
		for rule in CargoLintRules::default_rules() {
			registry.register(rule);
		}

		// Register npm rules
		for rule in NpmLintRules::default_rules() {
			registry.register(rule);
		}

		Self {
			registry,
			config: BTreeMap::new(),
		}
	}

	/// Set the lint configuration for an ecosystem.
	pub fn set_ecosystem_config(
		&mut self,
		ecosystem: Ecosystem,
		config: BTreeMap<String, LintRuleConfig>,
	) {
		self.config.insert(ecosystem.as_str().to_string(), config);
	}

	/// Lint a single file.
	#[must_use]
	pub fn lint_file(&self, file_path: &Path, workspace_root: &Path) -> LintReport {
		let mut report = LintReport::new();

		// Determine which rules apply to this file
		let applicable_rules = self.registry.applicable_rules(file_path);
		if applicable_rules.is_empty() {
			return report;
		}

		// Read file contents
		let contents = match std::fs::read_to_string(file_path) {
			Ok(contents) => contents,
			Err(error) => {
				report.warn(format!(
					"Failed to read file {}: {}",
					file_path.display(),
					error
				));
				return report;
			}
		};

		// Create lint context
		let ctx = LintContext {
			workspace_root,
			manifest_path: file_path,
			contents: &contents,
			parsed: None,
		};

		// Run each applicable rule
		for rule in applicable_rules {
			let ecosystem = rule.rule().id.split('/').next().unwrap_or("unknown");
			let rule_id = rule.rule().id.clone();

			// Get config for this rule
			let config = self
				.config
				.get(ecosystem)
				.and_then(|ecosystem_config| ecosystem_config.get(&rule_id))
				.cloned()
				.unwrap_or_else(|| {
					// Default: error for most rules
					LintRuleConfig::Severity(LintSeverity::Error)
				});

			// Skip if rule is disabled
			if !config.severity().is_enabled() {
				continue;
			}

			// Run the rule
			let results = rule.run(&ctx, &config);
			for mut result in results {
				// Apply configured severity
				result.severity = config.severity();
				report.add(result);
			}
		}

		report
	}

	/// Lint all files in a workspace.
	#[must_use]
	pub fn lint_workspace(&self, workspace_root: &Path) -> LintReport {
		let mut report = LintReport::new();

		// Walk the workspace looking for manifest files
		for entry in walkdir::WalkDir::new(workspace_root)
			.into_iter()
			.filter_map(Result::ok)
		{
			let path = entry.path();
			if !path.is_file() {
				continue;
			}

			// Check if file is a manifest we can lint
			let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
			if matches!(
				file_name,
				"Cargo.toml" | "package.json" | "deno.json" | "pubspec.yaml"
			) {
				let file_report = self.lint_file(path, workspace_root);
				report.merge(file_report);
			}
		}

		report
	}

	/// Apply autofixes from a lint report.
	///
	/// Returns a map of file paths to their fixed contents.
	#[must_use]
	pub fn apply_fixes(&self, report: &LintReport) -> BTreeMap<std::path::PathBuf, String> {
		let mut fixes_by_file: BTreeMap<std::path::PathBuf, Vec<LintFix>> = BTreeMap::new();

		// Collect all fixable results grouped by file
		for result in report.autofixable() {
			if let Some(fix) = &result.fix {
				fixes_by_file
					.entry(result.location.file_path.clone())
					.or_default()
					.push(fix.clone());
			}
		}

		// Apply fixes to each file
		let mut fixed_files = BTreeMap::new();
		for (file_path, fixes) in fixes_by_file {
			let contents = match std::fs::read_to_string(&file_path) {
				Ok(contents) => contents,
				Err(_) => continue,
			};

			let fixed = apply_fixes_to_content(&contents, &fixes);
			fixed_files.insert(file_path, fixed);
		}

		fixed_files
	}
}

/// Apply a set of fixes to file contents.
///
/// Fixes are applied in reverse order of their spans to avoid
/// invalidating earlier edits.
fn apply_fixes_to_content(contents: &str, fixes: &[LintFix]) -> String {
	let mut edits: Vec<_> = fixes.iter().flat_map(|fix| fix.edits.iter()).collect();

	// Sort by span start in reverse order (largest offsets first)
	edits.sort_by_key(|edit| std::cmp::Reverse(edit.span.0));

	let mut result = contents.to_string();
	for edit in edits {
		if edit.span.0 < result.len() && edit.span.1 <= result.len() {
			result.replace_range(edit.span.0..edit.span.1, &edit.replacement);
		}
	}

	result
}

/// Configuration for lint rules per ecosystem.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct LintConfig {
	#[serde(flatten)]
	pub ecosystems: BTreeMap<String, EcosystemLintConfig>,
}

/// Configuration for a single ecosystem's lint rules.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct EcosystemLintConfig {
	#[serde(flatten)]
	pub rules: BTreeMap<String, LintRuleConfig>,
}

#[cfg(test)]
mod tests {
	use std::io::Write;

	use tempfile::TempDir;

	use super::*;

	#[test]
	fn test_linter_creation() {
		let linter = Linter::new();
		assert!(!linter.registry.rules().is_empty());
	}

	#[test]
	fn test_lint_file_cargo_toml() {
		let dir = TempDir::new().unwrap();
		let cargo_toml = dir.path().join("Cargo.toml");

		let content = r#"
[package]
name = "test"
version = "1.0.0"

[dependencies]
serde = "1.0"
tokio = { version = "1.0", features = ["full"] }
"#;

		std::fs::write(&cargo_toml, content).unwrap();

		let linter = Linter::new();
		let report = linter.lint_file(&cargo_toml, dir.path());

		// Should have results for dependency ordering
		assert!(!report.results.is_empty());
	}

	#[test]
	fn test_apply_fixes_to_content() {
		let content = "Hello World";
		let fixes = vec![LintFix::single("Replace World", (6, 11), "Universe")];

		let result = apply_fixes_to_content(content, &fixes);
		assert_eq!(result, "Hello Universe");
	}

	#[test]
	fn test_apply_multiple_fixes() {
		let content = "Hello World and Earth";
		let fixes = vec![
			LintFix::single("Replace World", (6, 11), "Universe"),
			LintFix::single("Replace Earth", (16, 21), "Mars"),
		];

		let result = apply_fixes_to_content(content, &fixes);
		assert_eq!(result, "Hello Universe and Mars");
	}
}
