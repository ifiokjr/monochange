#![forbid(clippy::indexing_slicing)]

//! Linting types and utilities for monochange.
//!
//! This module provides the core types for the linting system, including
//! lint rules, results, severity levels, and autofix capabilities.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

/// The severity level of a lint.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
	/// The lint is disabled.
	#[default]
	Off,
	/// The lint produces a warning.
	Warning,
	/// The lint produces an error.
	Error,
}

impl LintSeverity {
	/// Return true if this severity is enabled (not Off).
	#[must_use]
	pub fn is_enabled(self) -> bool {
		matches!(self, Self::Warning | Self::Error)
	}

	/// Return true if this severity is Error.
	#[must_use]
	pub fn is_error(self) -> bool {
		matches!(self, Self::Error)
	}
}

impl std::fmt::Display for LintSeverity {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Off => write!(f, "off"),
			Self::Warning => write!(f, "warning"),
			Self::Error => write!(f, "error"),
		}
	}
}

/// The category of a lint rule.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintCategory {
	/// Formatting and style issues.
	Style,
	/// Potential correctness issues.
	Correctness,
	/// Performance-related issues.
	Performance,
	/// Suspicious patterns that may indicate bugs.
	Suspicious,
	/// Best practice recommendations.
	BestPractice,
}

/// A lint rule definition.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintRule {
	/// The unique identifier for this rule (e.g., "cargo/dependency-field-order").
	pub id: String,
	/// The human-readable name of this rule.
	pub name: String,
	/// A description of what this rule checks for.
	pub description: String,
	/// The category this rule belongs to.
	pub category: LintCategory,
	/// Whether this rule can be automatically fixed.
	pub autofixable: bool,
}

impl LintRule {
	/// Create a new lint rule.
	#[must_use]
	pub fn new(
		id: impl Into<String>,
		name: impl Into<String>,
		description: impl Into<String>,
		category: LintCategory,
		autofixable: bool,
	) -> Self {
		Self {
			id: id.into(),
			name: name.into(),
			description: description.into(),
			category,
			autofixable,
		}
	}
}

/// A location within a file where a lint was triggered.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintLocation {
	/// The path to the file.
	pub file_path: PathBuf,
	/// The 1-indexed line number.
	pub line: usize,
	/// The 1-indexed column number.
	pub column: usize,
	/// The byte span (start, end) within the file, if available.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub span: Option<(usize, usize)>,
}

impl LintLocation {
	/// Create a new lint location.
	#[must_use]
	pub fn new(file_path: impl Into<PathBuf>, line: usize, column: usize) -> Self {
		Self {
			file_path: file_path.into(),
			line,
			column,
			span: None,
		}
	}

	/// Add a byte span to this location.
	#[must_use]
	pub fn with_span(mut self, start: usize, end: usize) -> Self {
		self.span = Some((start, end));
		self
	}
}

/// A single edit operation for fixing a lint.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintEdit {
	/// The byte span (start, end) within the file to replace.
	pub span: (usize, usize),
	/// The replacement text.
	pub replacement: String,
}

impl LintEdit {
	/// Create a new lint edit.
	#[must_use]
	pub fn new(span: (usize, usize), replacement: impl Into<String>) -> Self {
		Self {
			span,
			replacement: replacement.into(),
		}
	}
}

/// An autofix suggestion for a lint result.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintFix {
	/// A human-readable description of the fix.
	pub description: String,
	/// The edit operations to apply.
	pub edits: Vec<LintEdit>,
}

impl LintFix {
	/// Create a new lint fix with a single edit.
	#[must_use]
	pub fn single(
		description: impl Into<String>,
		span: (usize, usize),
		replacement: impl Into<String>,
	) -> Self {
		Self {
			description: description.into(),
			edits: vec![LintEdit::new(span, replacement)],
		}
	}

	/// Create a new lint fix with multiple edits.
	#[must_use]
	pub fn multiple(description: impl Into<String>, edits: Vec<LintEdit>) -> Self {
		Self {
			description: description.into(),
			edits,
		}
	}
}

/// A single lint result.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintResult {
	/// The ID of the rule that triggered this lint.
	pub rule_id: String,
	/// The location where the lint was triggered.
	pub location: LintLocation,
	/// The human-readable message.
	pub message: String,
	/// The severity of this lint.
	pub severity: LintSeverity,
	/// The autofix suggestion, if available.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub fix: Option<LintFix>,
}

impl LintResult {
	/// Create a new lint result.
	#[must_use]
	pub fn new(
		rule_id: impl Into<String>,
		location: LintLocation,
		message: impl Into<String>,
		severity: LintSeverity,
	) -> Self {
		Self {
			rule_id: rule_id.into(),
			location,
			message: message.into(),
			severity,
			fix: None,
		}
	}

	/// Add a fix to this lint result.
	#[must_use]
	pub fn with_fix(mut self, fix: LintFix) -> Self {
		self.fix = Some(fix);
		self
	}
}

/// Configuration for a single lint rule.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LintRuleConfig {
	/// Simple severity configuration (e.g., "error", "warn", or "off").
	Severity(LintSeverity),
	/// Detailed configuration with options.
	Detailed {
		/// The severity level.
		level: LintSeverity,
		/// Additional rule-specific options.
		#[serde(flatten)]
		options: BTreeMap<String, serde_json::Value>,
	},
}

impl LintRuleConfig {
	/// Get the severity level from this configuration.
	#[must_use]
	pub fn severity(&self) -> LintSeverity {
		match self {
			Self::Severity(severity) => *severity,
			Self::Detailed { level, .. } => *level,
		}
	}

	/// Get an option value by key.
	pub fn option(&self, key: &str) -> Option<&serde_json::Value> {
		match self {
			Self::Severity(_) => None,
			Self::Detailed { options, .. } => options.get(key),
		}
	}

	/// Get a boolean option value.
	pub fn bool_option(&self, key: &str, default: bool) -> bool {
		self.option(key)
			.and_then(|v| v.as_bool())
			.unwrap_or(default)
	}

	/// Get a string option value.
	pub fn string_option(&self, key: &str) -> Option<String> {
		self.option(key).and_then(|v| v.as_str()).map(String::from)
	}

	/// Get a string list option value.
	pub fn string_list_option(&self, key: &str) -> Option<Vec<String>> {
		self.option(key).and_then(|v| v.as_array()).map(|arr| {
			arr.iter()
				.filter_map(|v| v.as_str().map(String::from))
				.collect()
		})
	}
}

impl Default for LintRuleConfig {
	fn default() -> Self {
		Self::Severity(LintSeverity::Off)
	}
}

/// A collection of lint results.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct LintReport {
	/// The results of the lint run.
	pub results: Vec<LintResult>,
	/// Any warnings encountered during linting.
	pub warnings: Vec<String>,
	/// The number of errors found.
	pub error_count: usize,
	/// The number of warnings found.
	pub warning_count: usize,
}

impl LintReport {
	/// Create a new empty lint report.
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// Add a result to the report.
	pub fn add(&mut self, result: LintResult) {
		match result.severity {
			LintSeverity::Error => self.error_count += 1,
			LintSeverity::Warning => self.warning_count += 1,
			LintSeverity::Off => {}
		}
		self.results.push(result);
	}

	/// Add a warning message.
	pub fn warn(&mut self, message: impl Into<String>) {
		self.warnings.push(message.into());
	}

	/// Return true if there are any errors.
	#[must_use]
	pub fn has_errors(&self) -> bool {
		self.error_count > 0
	}

	/// Return true if there are any errors or warnings.
	#[must_use]
	pub fn has_issues(&self) -> bool {
		self.error_count > 0 || self.warning_count > 0
	}

	/// Merge another report into this one.
	pub fn merge(&mut self, other: Self) {
		self.results.extend(other.results);
		self.warnings.extend(other.warnings);
		self.error_count += other.error_count;
		self.warning_count += other.warning_count;
	}

	/// Return all autofixable results.
	#[must_use]
	pub fn autofixable(&self) -> Vec<&LintResult> {
		self.results.iter().filter(|r| r.fix.is_some()).collect()
	}
}

/// The input to a lint rule.
pub struct LintContext<'a> {
	/// The workspace root path.
	pub workspace_root: &'a std::path::Path,
	/// The package manifest path being linted.
	pub manifest_path: &'a std::path::Path,
	/// The raw contents of the manifest file.
	pub contents: &'a str,
	/// The parsed document, if available (ecosystem-specific).
	pub parsed: Option<&'a dyn std::any::Any>,
}

impl std::fmt::Debug for LintContext<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LintContext")
			.field("workspace_root", &self.workspace_root)
			.field("manifest_path", &self.manifest_path)
			.field("contents_len", &self.contents.len())
			.field("parsed", &self.parsed.is_some())
			.finish()
	}
}

/// A lint rule that can be executed.
pub trait LintRuleRunner: Send + Sync {
	/// Get the rule definition.
	fn rule(&self) -> &LintRule;

	/// Check if this rule applies to the given file.
	fn applies_to(&self, path: &std::path::Path) -> bool;

	/// Run this rule and return any lint results.
	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult>;
}

/// A registry of lint rules.
#[derive(Default)]
pub struct LintRuleRegistry {
	rules: Vec<Box<dyn LintRuleRunner>>,
}

impl std::fmt::Debug for LintRuleRegistry {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LintRuleRegistry")
			.field("rule_count", &self.rules.len())
			.finish()
	}
}

impl LintRuleRegistry {
	/// Create a new empty registry.
	#[must_use]
	pub fn new() -> Self {
		Self::default()
	}

	/// Register a lint rule.
	pub fn register(&mut self, rule: Box<dyn LintRuleRunner>) {
		self.rules.push(rule);
	}

	/// Get all registered rules.
	#[must_use]
	pub fn rules(&self) -> &[Box<dyn LintRuleRunner>] {
		&self.rules
	}

	/// Find a rule by ID.
	pub fn find(&self, id: &str) -> Option<&dyn LintRuleRunner> {
		self.rules
			.iter()
			.find(|r| r.rule().id == id)
			.map(|r| r.as_ref())
	}

	/// Find all rules that apply to a given file.
	#[must_use]
	pub fn applicable_rules(&self, path: &std::path::Path) -> Vec<&dyn LintRuleRunner> {
		self.rules
			.iter()
			.filter(|r| r.applies_to(path))
			.map(|r| r.as_ref())
			.collect()
	}
}

impl std::fmt::Display for LintResult {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let fix_indicator = if self.fix.is_some() { " [fixable]" } else { "" };
		write!(
			f,
			"{}: {} at {}:{}:{}{}",
			self.severity,
			self.message,
			self.location.file_path.display(),
			self.location.line,
			self.location.column,
			fix_indicator
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn lint_severity_ordering() {
		assert!(LintSeverity::Error > LintSeverity::Warning);
		assert!(LintSeverity::Warning > LintSeverity::Off);
		assert!(LintSeverity::Error > LintSeverity::Off);
	}

	#[test]
	fn lint_rule_config_severity() {
		let config = LintRuleConfig::Severity(LintSeverity::Error);
		assert_eq!(config.severity(), LintSeverity::Error);

		let config = LintRuleConfig::Detailed {
			level: LintSeverity::Warning,
			options: BTreeMap::new(),
		};
		assert_eq!(config.severity(), LintSeverity::Warning);
	}

	#[test]
	fn lint_report_counting() {
		let mut report = LintReport::new();
		assert!(!report.has_issues());
		assert!(!report.has_errors());

		report.add(LintResult::new(
			"test/rule",
			LintLocation::new("test.toml", 1, 1),
			"Test warning",
			LintSeverity::Warning,
		));
		assert!(report.has_issues());
		assert!(!report.has_errors());
		assert_eq!(report.warning_count, 1);

		report.add(LintResult::new(
			"test/rule",
			LintLocation::new("test.toml", 2, 1),
			"Test error",
			LintSeverity::Error,
		));
		assert!(report.has_errors());
		assert_eq!(report.error_count, 1);
	}
}
