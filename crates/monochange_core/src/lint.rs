#![forbid(clippy::indexing_slicing)]

//! Linting types and utilities for monochange.
//!
//! This module provides the shared types and contracts for monochange's
//! manifest-linting system. The runtime engine stays ecosystem-agnostic while
//! ecosystem crates provide suites, targets, and rule implementations.

use std::any::Any;
use std::collections::BTreeMap;
use std::path::Path;
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

/// The maturity tier of a lint rule or preset.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LintMaturity {
	/// Stable, generally recommended lints.
	#[default]
	Stable,
	/// Opinionated lints that are useful for strict repositories.
	Strict,
	/// Experimental lints that may evolve quickly.
	Experimental,
}

/// The kind of a rule option.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintOptionKind {
	Boolean,
	String,
	StringList,
	Integer,
}

/// A documented rule option.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintOptionDefinition {
	pub name: String,
	pub description: String,
	pub kind: LintOptionKind,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub default_value: Option<serde_json::Value>,
}

impl LintOptionDefinition {
	/// Create a new option definition.
	#[must_use]
	pub fn new(
		name: impl Into<String>,
		description: impl Into<String>,
		kind: LintOptionKind,
	) -> Self {
		Self {
			name: name.into(),
			description: description.into(),
			kind,
			default_value: None,
		}
	}

	/// Attach a default value.
	#[must_use]
	pub fn with_default(mut self, value: serde_json::Value) -> Self {
		self.default_value = Some(value);
		self
	}
}

/// A lint rule definition.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintRule {
	/// The unique identifier for this rule (e.g., `cargo/sorted-dependencies`).
	pub id: String,
	/// The human-readable name of this rule.
	pub name: String,
	/// A description of what this rule checks for.
	pub description: String,
	/// The category this rule belongs to.
	pub category: LintCategory,
	/// How mature this rule is.
	pub maturity: LintMaturity,
	/// Whether this rule can be automatically fixed.
	pub autofixable: bool,
	/// Documented rule options.
	#[serde(default)]
	pub options: Vec<LintOptionDefinition>,
}

impl LintRule {
	/// Create a new lint rule.
	#[must_use]
	pub fn new(
		id: impl Into<String>,
		name: impl Into<String>,
		description: impl Into<String>,
		category: LintCategory,
		maturity: LintMaturity,
		autofixable: bool,
	) -> Self {
		Self {
			id: id.into(),
			name: name.into(),
			description: description.into(),
			category,
			maturity,
			autofixable,
			options: Vec::new(),
		}
	}

	/// Attach documented options.
	#[must_use]
	pub fn with_options(mut self, options: Vec<LintOptionDefinition>) -> Self {
		self.options = options;
		self
	}
}

/// A reusable lint preset.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LintPreset {
	pub id: String,
	pub name: String,
	pub description: String,
	pub maturity: LintMaturity,
	#[serde(default)]
	pub rules: BTreeMap<String, LintRuleConfig>,
}

impl LintPreset {
	/// Create a new lint preset.
	#[must_use]
	pub fn new(
		id: impl Into<String>,
		name: impl Into<String>,
		description: impl Into<String>,
		maturity: LintMaturity,
	) -> Self {
		Self {
			id: id.into(),
			name: name.into(),
			description: description.into(),
			maturity,
			rules: BTreeMap::new(),
		}
	}

	/// Add rules to the preset.
	#[must_use]
	pub fn with_rules(mut self, rules: BTreeMap<String, LintRuleConfig>) -> Self {
		self.rules = rules;
		self
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
#[allow(variant_size_differences)]
pub enum LintRuleConfig {
	/// Simple severity configuration (e.g., `"error"`, `"warning"`, or `"off"`).
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
			.and_then(serde_json::Value::as_bool)
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
				.filter_map(|value| value.as_str().map(String::from))
				.collect()
		})
	}

	/// Clone this config while overriding the severity.
	#[must_use]
	pub fn with_severity(&self, severity: LintSeverity) -> Self {
		match self {
			Self::Severity(_) => Self::Severity(severity),
			Self::Detailed { options, .. } => {
				Self::Detailed {
					level: severity,
					options: options.clone(),
				}
			}
		}
	}

	/// Merge another config into this one, preferring values from `other`.
	#[must_use]
	pub fn merged_with(&self, other: &Self) -> Self {
		match (self, other) {
			(_, Self::Severity(severity)) => Self::Severity(*severity),
			(Self::Severity(_), Self::Detailed { level, options }) => {
				Self::Detailed {
					level: *level,
					options: options.clone(),
				}
			}
			(
				Self::Detailed {
					options: left_options,
					..
				},
				Self::Detailed {
					level: right_level,
					options: right_options,
				},
			) => {
				let mut merged = left_options.clone();
				merged.extend(right_options.clone());
				Self::Detailed {
					level: *right_level,
					options: merged,
				}
			}
		}
	}
}

impl Default for LintRuleConfig {
	fn default() -> Self {
		Self::Severity(LintSeverity::Off)
	}
}

/// Selector used to scope lint settings to a subset of manifest targets.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct LintSelector {
	#[serde(default)]
	pub ecosystems: Vec<String>,
	#[serde(default)]
	pub paths: Vec<String>,
	#[serde(default)]
	pub package_ids: Vec<String>,
	#[serde(default)]
	pub group_ids: Vec<String>,
	#[serde(default)]
	pub managed: Option<bool>,
	#[serde(default)]
	pub private: Option<bool>,
	#[serde(default)]
	pub publishable: Option<bool>,
}

/// A scoped lint override.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct LintScopeConfig {
	#[serde(default)]
	pub name: Option<String>,
	#[serde(default, rename = "match")]
	pub selector: LintSelector,
	#[serde(default, rename = "use")]
	pub presets: Vec<String>,
	#[serde(default)]
	pub rules: BTreeMap<String, LintRuleConfig>,
}

/// Top-level workspace lint settings from `monochange.toml`.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct WorkspaceLintSettings {
	#[serde(default, rename = "use")]
	pub presets: Vec<String>,
	#[serde(default)]
	pub rules: BTreeMap<String, LintRuleConfig>,
	#[serde(default)]
	pub scopes: Vec<LintScopeConfig>,
}

/// Metadata attached to a lint target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, Default)]
pub struct LintTargetMetadata {
	pub ecosystem: String,
	pub relative_path: PathBuf,
	#[serde(default)]
	pub package_name: Option<String>,
	#[serde(default)]
	pub package_id: Option<String>,
	#[serde(default)]
	pub group_id: Option<String>,
	#[serde(default)]
	pub managed: bool,
	#[serde(default)]
	pub private: Option<bool>,
	#[serde(default)]
	pub publishable: Option<bool>,
}

/// A parsed manifest ready for lint execution.
pub struct LintTarget {
	pub workspace_root: PathBuf,
	pub manifest_path: PathBuf,
	pub contents: String,
	pub metadata: LintTargetMetadata,
	pub parsed: Box<dyn Any>,
}

impl std::fmt::Debug for LintTarget {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LintTarget")
			.field("workspace_root", &self.workspace_root)
			.field("manifest_path", &self.manifest_path)
			.field("metadata", &self.metadata)
			.field("contents_len", &self.contents.len())
			.field("parsed", &"<opaque>")
			.finish()
	}
}

impl LintTarget {
	/// Create a new lint target.
	#[must_use]
	pub fn new(
		workspace_root: impl Into<PathBuf>,
		manifest_path: impl Into<PathBuf>,
		contents: impl Into<String>,
		metadata: LintTargetMetadata,
		parsed: Box<dyn Any>,
	) -> Self {
		Self {
			workspace_root: workspace_root.into(),
			manifest_path: manifest_path.into(),
			contents: contents.into(),
			metadata,
			parsed,
		}
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
		self.results
			.iter()
			.filter(|result| result.fix.is_some())
			.collect()
	}
}

/// The input to a lint rule.
pub struct LintContext<'a> {
	/// The workspace root path.
	pub workspace_root: &'a Path,
	/// The package manifest path being linted.
	pub manifest_path: &'a Path,
	/// The raw contents of the manifest file.
	pub contents: &'a str,
	/// Shared metadata about the lint target.
	pub metadata: &'a LintTargetMetadata,
	/// The parsed document for this target.
	pub parsed: &'a dyn Any,
}

impl LintContext<'_> {
	/// Downcast the parsed document to a concrete type.
	pub fn parsed_as<T: Any>(&self) -> Option<&T> {
		self.parsed.downcast_ref::<T>()
	}
}

impl std::fmt::Debug for LintContext<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LintContext")
			.field("workspace_root", &self.workspace_root)
			.field("manifest_path", &self.manifest_path)
			.field("contents_len", &self.contents.len())
			.field("metadata", &self.metadata)
			.finish()
	}
}

/// A lint rule that can be executed.
pub trait LintRuleRunner: Send + Sync {
	/// Get the rule definition.
	fn rule(&self) -> &LintRule;

	/// Check if this rule applies to the given target.
	fn applies_to(&self, _target: &LintTarget) -> bool {
		true
	}

	/// Run this rule and return any lint results.
	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult>;
}

/// A collection of rules and target discovery for one manifest family.
pub trait LintSuite: Send + Sync {
	/// Return the suite identifier, typically matching the ecosystem name.
	fn suite_id(&self) -> &'static str;

	/// Return the rules provided by this suite.
	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>>;

	/// Return built-in presets contributed by this suite.
	fn presets(&self) -> Vec<LintPreset> {
		Vec::new()
	}

	/// Discover and parse lint targets for this suite.
	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &crate::WorkspaceConfiguration,
	) -> crate::MonochangeResult<Vec<LintTarget>>;
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
			.find(|rule| rule.rule().id == id)
			.map(AsRef::as_ref)
	}

	/// Find all rules that apply to a given target.
	#[must_use]
	pub fn applicable_rules(&self, target: &LintTarget) -> Vec<&dyn LintRuleRunner> {
		self.rules
			.iter()
			.filter(|rule| rule.applies_to(target))
			.map(AsRef::as_ref)
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

	#[test]
	fn detailed_rule_config_merges_options() {
		let left = LintRuleConfig::Detailed {
			level: LintSeverity::Warning,
			options: BTreeMap::from([("fix".to_string(), serde_json::Value::Bool(false))]),
		};
		let right = LintRuleConfig::Detailed {
			level: LintSeverity::Error,
			options: BTreeMap::from([("fields".to_string(), serde_json::json!(["description"]))]),
		};

		let merged = left.merged_with(&right);
		assert_eq!(merged.severity(), LintSeverity::Error);
		assert!(!merged.bool_option("fix", true));
		assert_eq!(
			merged.string_list_option("fields"),
			Some(vec!["description".to_string()])
		);
	}

	#[test]
	fn lint_rule_config_with_severity_preserves_options() {
		let config = LintRuleConfig::Detailed {
			level: LintSeverity::Warning,
			options: BTreeMap::from([("fix".to_string(), serde_json::Value::Bool(true))]),
		};
		let updated = config.with_severity(LintSeverity::Error);
		assert_eq!(updated.severity(), LintSeverity::Error);
		assert!(updated.bool_option("fix", false));
	}

	#[test]
	fn lint_target_and_context_expose_parsed_payloads() {
		let target = LintTarget::new(
			".",
			"Cargo.toml",
			"contents",
			LintTargetMetadata {
				ecosystem: "cargo".to_string(),
				relative_path: PathBuf::from("Cargo.toml"),
				package_name: Some("core".to_string()),
				package_id: Some("core".to_string()),
				group_id: None,
				managed: true,
				private: Some(false),
				publishable: Some(true),
			},
			Box::new(42usize),
		);
		let ctx = LintContext {
			workspace_root: target.workspace_root.as_path(),
			manifest_path: target.manifest_path.as_path(),
			contents: &target.contents,
			metadata: &target.metadata,
			parsed: target.parsed.as_ref(),
		};
		assert_eq!(ctx.parsed_as::<usize>(), Some(&42));
		assert!(format!("{target:?}").contains("contents_len"));
		assert!(format!("{ctx:?}").contains("metadata"));
	}

	#[test]
	fn lint_option_definition_with_default_sets_default_value() {
		let option = LintOptionDefinition::new("fix", "desc", LintOptionKind::Boolean)
			.with_default(serde_json::Value::Bool(true));
		assert_eq!(option.default_value, Some(serde_json::Value::Bool(true)));
	}

	#[test]
	fn lint_rule_config_with_severity_updates_plain_severity() {
		let config = LintRuleConfig::Severity(LintSeverity::Warning);
		assert_eq!(
			config.with_severity(LintSeverity::Error).severity(),
			LintSeverity::Error
		);
	}

	#[test]
	fn lint_rule_config_merged_with_promotes_detailed_options_from_plain_config() {
		let left = LintRuleConfig::Severity(LintSeverity::Warning);
		let right = LintRuleConfig::Detailed {
			level: LintSeverity::Error,
			options: BTreeMap::from([("fix".to_string(), serde_json::Value::Bool(true))]),
		};
		let merged = left.merged_with(&right);
		assert_eq!(merged.severity(), LintSeverity::Error);
		assert!(merged.bool_option("fix", false));
	}

	#[test]
	fn lint_registry_finds_applicable_rules() {
		#[derive(Debug)]
		struct ExampleRule(LintRule);
		impl LintRuleRunner for ExampleRule {
			fn rule(&self) -> &LintRule {
				&self.0
			}

			fn applies_to(&self, target: &LintTarget) -> bool {
				target.metadata.ecosystem == "cargo"
			}

			fn run(&self, _ctx: &LintContext<'_>, _config: &LintRuleConfig) -> Vec<LintResult> {
				Vec::new()
			}
		}

		let mut registry = LintRuleRegistry::new();
		registry.register(Box::new(ExampleRule(LintRule::new(
			"cargo/example",
			"Example",
			"example",
			LintCategory::Style,
			LintMaturity::Stable,
			false,
		))));
		let target = LintTarget::new(
			".",
			"Cargo.toml",
			"",
			LintTargetMetadata {
				ecosystem: "cargo".to_string(),
				relative_path: PathBuf::from("Cargo.toml"),
				package_name: None,
				package_id: None,
				group_id: None,
				managed: false,
				private: None,
				publishable: None,
			},
			Box::new(()),
		);
		assert!(registry.find("cargo/example").is_some());
		assert_eq!(registry.applicable_rules(&target).len(), 1);
		let ctx = LintContext {
			workspace_root: target.workspace_root.as_path(),
			manifest_path: target.manifest_path.as_path(),
			contents: &target.contents,
			metadata: &target.metadata,
			parsed: target.parsed.as_ref(),
		};
		assert!(
			registry
				.find("cargo/example")
				.unwrap()
				.run(&ctx, &LintRuleConfig::default())
				.is_empty()
		);
	}
}
