#![forbid(clippy::indexing_slicing)]

//! Changeset lint suite for monochange.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use monochange_core::BumpSeverity;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintCategory;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintMaturity;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_core::lint::LintTargetMetadata;

use crate::RawChangeEntry;
use crate::parse_bump_severity;

/// Return the shared changeset lint suite.
#[must_use]
pub fn lint_suite() -> ChangesetLintSuite {
	ChangesetLintSuite::new()
}

/// Parsed changeset data stored in a lint target.
#[derive(Debug, Clone)]
pub struct ChangesetLintFile {
	/// The markdown body after frontmatter.
	pub(crate) body: String,
	/// The parsed change entries from frontmatter.
	pub(crate) changes: Vec<RawChangeEntry>,
}

/// Changeset lint suite implementation.
#[derive(Debug, Clone, Default)]
pub struct ChangesetLintSuite;

impl ChangesetLintSuite {
	/// Create a new changeset lint suite.
	#[must_use]
	pub fn new() -> Self {
		Self
	}
}

impl LintSuite for ChangesetLintSuite {
	fn suite_id(&self) -> &'static str {
		"changesets"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(SummaryRule::new()),
			Box::new(NoSectionHeadingsRule::new()),
			Box::new(BumpScopeRule::new(BumpSeverity::None)),
			Box::new(BumpScopeRule::new(BumpSeverity::Patch)),
			Box::new(BumpScopeRule::new(BumpSeverity::Minor)),
			Box::new(BumpScopeRule::new(BumpSeverity::Major)),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"changesets/recommended",
				"Changesets recommended",
				"Balanced changeset linting for typical monochange repositories",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([(
				"changesets/summary".to_string(),
				LintRuleConfig::Severity(LintSeverity::Error),
			)])),
		]
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		_configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		let changeset_dir = workspace_root.join(".changeset");
		if !changeset_dir.exists() {
			return Ok(Vec::new());
		}

		let mut targets = Vec::new();
		for entry in fs::read_dir(&changeset_dir)
			.map_err(|error| {
				MonochangeError::Io(format!("failed to read changeset directory: {error}"))
			})?
			.filter_map(Result::ok)
		{
			let path = entry.path();
			let Some(ext) = path.extension() else {
				continue;
			};
			if ext != "md" {
				continue;
			}
			let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
			if !Path::new(file_name)
				.extension()
				.is_some_and(|ext| ext.eq_ignore_ascii_case("md"))
				|| file_name == "README.md"
			{
				continue;
			}

			let contents = fs::read_to_string(&path).map_err(|error| {
				MonochangeError::Io(format!("failed to read changeset file: {error}"))
			})?;
			let Some((body, changes)) = parse_changeset_for_lint(&contents) else {
				continue;
			};

			let relative_path = path.strip_prefix(workspace_root).unwrap_or(&path);
			targets.push(LintTarget::new(
				workspace_root.to_path_buf(),
				path.clone(),
				contents,
				LintTargetMetadata {
					ecosystem: "changesets".to_string(),
					relative_path: relative_path.to_path_buf(),
					package_name: None,
					package_id: None,
					group_id: None,
					managed: false,
					private: None,
					publishable: None,
				},
				Box::new(ChangesetLintFile { body, changes }),
			));
		}

		Ok(targets)
	}
}

/// Parse a changeset file for linting.
///
/// Returns `Some((body, changes))` if the file has valid frontmatter,
/// or `None` if it doesn't look like a changeset file.
fn parse_changeset_for_lint(contents: &str) -> Option<(String, Vec<RawChangeEntry>)> {
	let contents = contents.replace("\r\n", "\n").replace('\r', "\n");
	let without_opening = contents.strip_prefix("---")?;
	let (frontmatter, body_with_separator) = without_opening.split_once("\n---\n")?;
	let body = body_with_separator.trim().to_string();
	let mapping: serde_yaml_ng::Mapping = serde_yaml_ng::from_str(frontmatter).ok()?;

	let mut changes = Vec::new();
	for (key, value) in mapping {
		let package = key.as_str()?;
		let (bump, change_type) = parse_simple_change_value(&value);
		changes.push(RawChangeEntry {
			package: package.to_string(),
			bump,
			version: None,
			reason: None,
			details: None,
			change_type,
			caused_by: Vec::new(),
		});
	}

	Some((body, changes))
}

fn parse_simple_change_value(
	value: &serde_yaml_ng::Value,
) -> (Option<BumpSeverity>, Option<String>) {
	if let Some(token) = value.as_str().map(str::trim).filter(|s| !s.is_empty()) {
		if let Some(bump) = parse_bump_severity(token) {
			return (Some(bump), None);
		}
		return (None, Some(token.to_string()));
	}

	if let Some(mapping) = value.as_mapping() {
		let bump = mapping
			.get(serde_yaml_ng::Value::String("bump".to_string()))
			.and_then(serde_yaml_ng::Value::as_str)
			.and_then(parse_bump_severity);
		let change_type = mapping
			.get(serde_yaml_ng::Value::String("type".to_string()))
			.and_then(serde_yaml_ng::Value::as_str)
			.map(str::trim)
			.filter(|s| !s.is_empty())
			.map(ToString::to_string);
		return (bump, change_type);
	}

	(None, None)
}

fn changeset_file<'a>(ctx: &'a LintContext<'a>) -> Option<&'a ChangesetLintFile> {
	ctx.parsed_as::<ChangesetLintFile>()
}

// ── Summary rule ───────────────────────────────────────────────────────────

#[derive(Debug)]
struct SummaryRule {
	rule: LintRule,
}

impl SummaryRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"changesets/summary",
				"Changeset summary heading",
				"Requires changeset body to start with a summary heading",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for SummaryRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		let required = config.bool_option("required", false);
		let heading_level = config
			.option("heading_level")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let min_length = config
			.option("min_length")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let max_length = config
			.option("max_length")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let forbid_trailing_period = config.bool_option("forbid_trailing_period", false);
		let forbid_conventional_commit_prefix =
			config.bool_option("forbid_conventional_commit_prefix", false);

		use crate::first_non_empty_line;
		use crate::has_conventional_commit_prefix;
		use crate::markdown_heading_level;
		use crate::markdown_heading_text;

		let mut results = Vec::new();
		let body = &file.body;

		let Some(first_line) = first_non_empty_line(body) else {
			if required {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					"changeset body must start with a summary heading",
					severity,
				));
			}
			return results;
		};

		let heading = markdown_heading_level(first_line);
		if required && heading.is_none() {
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				"changeset body must start with a summary heading",
				severity,
			));
			return results;
		}

		if let (Some(required_level), Some(actual_level)) = (heading_level, heading)
			&& actual_level != required_level
		{
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				format!(
					"changeset summary heading must use level {required_level}, found level {actual_level}"
				),
				severity,
			));
			return results;
		}

		let summary =
			markdown_heading_text(first_line).unwrap_or_else(|| first_line.trim().to_string());

		if let Some(min) = min_length
			&& summary.chars().count() < min
		{
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				format!("changeset summary must be at least {min} characters"),
				severity,
			));
		}

		if let Some(max) = max_length
			&& summary.chars().count() > max
		{
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				format!("changeset summary must be at most {max} characters"),
				severity,
			));
		}

		if forbid_trailing_period && summary.ends_with('.') {
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				"changeset summary must not end with a period",
				severity,
			));
		}

		if forbid_conventional_commit_prefix && has_conventional_commit_prefix(&summary) {
			results.push(LintResult::new(
				self.rule.id.clone(),
				LintLocation::new(ctx.manifest_path, 1, 1),
				"changeset summary must not use a conventional-commit prefix",
				severity,
			));
		}

		results
	}
}

// ── No section headings rule ─────────────────────────────────────────────────

#[derive(Debug)]
struct NoSectionHeadingsRule {
	rule: LintRule,
}

impl NoSectionHeadingsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"changesets/no_section_headings",
				"Changeset no section headings",
				"Requires changeset body to not use change types as headings",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for NoSectionHeadingsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		use std::collections::BTreeSet;

		use crate::markdown_has_heading;

		let change_types: BTreeSet<&str> = file
			.changes
			.iter()
			.filter_map(|change| change.change_type.as_deref())
			.collect();

		let mut results = Vec::new();
		for change_type in change_types {
			if markdown_has_heading(&file.body, change_type) {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("changeset type `{change_type}` must not also be used as a heading"),
					severity,
				));
			}
		}

		results
	}
}

// ── Bump scope rule ────────────────────────────────────────────────────────

#[derive(Debug)]
struct BumpScopeRule {
	rule: LintRule,
	bump: BumpSeverity,
}

impl BumpScopeRule {
	fn new(bump: BumpSeverity) -> Self {
		Self {
			rule: LintRule::new(
				format!("changesets/bump/{bump}"),
				format!("Changeset {bump} scope"),
				format!("Requires changesets with bump `{bump}` to satisfy scope rules"),
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
			bump,
		}
	}
}

impl LintRuleRunner for BumpScopeRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let severity = config.severity();
		if !severity.is_enabled() {
			return Vec::new();
		}

		let Some(file) = changeset_file(ctx) else {
			return Vec::new();
		};

		use crate::markdown_has_code_block;
		use crate::markdown_has_heading;

		let required_bump = config
			.option("required_bump")
			.and_then(|v| v.as_str())
			.and_then(parse_bump_severity);
		let required_sections = config
			.string_list_option("required_sections")
			.unwrap_or_default();
		let forbidden_headings = config
			.string_list_option("forbidden_headings")
			.unwrap_or_default();
		let min_body_chars = config
			.option("min_body_chars")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let max_body_chars = config
			.option("max_body_chars")
			.and_then(serde_json::Value::as_u64)
			.map(|v| v as usize);
		let require_code_block = config.bool_option("require_code_block", false);

		let mut results = Vec::new();

		for change in &file.changes {
			if change.bump != Some(self.bump) {
				continue;
			}

			if let Some(required) = required_bump
				&& change.bump != Some(required)
			{
				let actual = change
					.bump
					.map_or_else(|| "auto".to_string(), |b| b.to_string());
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!(
						"changeset type `{}` requires bump `{required}`, found `{actual}`",
						change.change_type.as_deref().unwrap_or("<unknown>")
					),
					severity,
				));
			}

			for section in &required_sections {
				if !markdown_has_heading(&file.body, section) {
					results.push(LintResult::new(
						self.rule.id.clone(),
						LintLocation::new(ctx.manifest_path, 1, 1),
						format!("changeset must include a `{section}` section"),
						severity,
					));
				}
			}

			for heading in &forbidden_headings {
				if markdown_has_heading(&file.body, heading) {
					results.push(LintResult::new(
						self.rule.id.clone(),
						LintLocation::new(ctx.manifest_path, 1, 1),
						format!("changeset must not use `{heading}` as a heading"),
						severity,
					));
				}
			}

			if let Some(min_chars) = min_body_chars
				&& file.body.trim().chars().count() < min_chars
			{
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("changeset body must be at least {min_chars} characters"),
					severity,
				));
			}

			if let Some(max_chars) = max_body_chars
				&& file.body.trim().chars().count() > max_chars
			{
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("changeset body must be at most {max_chars} characters"),
					severity,
				));
			}

			if require_code_block && !markdown_has_code_block(&file.body) {
				results.push(LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					"changeset must include a fenced code block",
					severity,
				));
			}
		}

		results
	}
}

// ── Trait extension for LintRuleConfig ─────────────────────────────────────

#[allow(dead_code)]
trait LintRuleConfigExt {
	fn bool_option(&self, key: &str, default: bool) -> bool;
	fn string_list_option(&self, key: &str) -> Option<Vec<String>>;
}

impl LintRuleConfigExt for LintRuleConfig {
	fn bool_option(&self, key: &str, default: bool) -> bool {
		self.option(key)
			.and_then(serde_json::Value::as_bool)
			.unwrap_or(default)
	}

	fn string_list_option(&self, key: &str) -> Option<Vec<String>> {
		self.option(key)?.as_array().map(|arr| {
			arr.iter()
				.filter_map(|v| v.as_str().map(ToString::to_string))
				.collect()
		})
	}
}

#[cfg(test)]
mod tests {
	use std::collections::BTreeMap;
	use std::fs;
	use std::path::Path;
	use std::path::PathBuf;

	use monochange_core::lint::LintContext;
	use monochange_core::lint::LintRuleConfig;
	use monochange_core::lint::LintRuleRunner;
	use monochange_core::lint::LintSeverity;
	use monochange_core::lint::LintTargetMetadata;
	use serde_json::json;

	use super::*;

	fn change(
		package: &str,
		bump: Option<BumpSeverity>,
		change_type: Option<&str>,
	) -> RawChangeEntry {
		RawChangeEntry {
			package: package.to_string(),
			bump,
			version: None,
			reason: None,
			details: None,
			change_type: change_type.map(ToString::to_string),
			caused_by: Vec::new(),
		}
	}

	fn lint_file(body: &str, changes: Vec<RawChangeEntry>) -> ChangesetLintFile {
		ChangesetLintFile {
			body: body.to_string(),
			changes,
		}
	}

	fn metadata() -> LintTargetMetadata {
		LintTargetMetadata {
			ecosystem: "changesets".to_string(),
			relative_path: PathBuf::from(".changeset/change.md"),
			package_name: None,
			package_id: None,
			group_id: None,
			managed: false,
			private: None,
			publishable: None,
		}
	}

	fn workspace_configuration(root: &Path) -> WorkspaceConfiguration {
		WorkspaceConfiguration {
			root_path: root.to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults::default(),
			changelog: monochange_core::ChangelogSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			lints: monochange_core::lint::WorkspaceLintSettings::default(),
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
			python: monochange_core::EcosystemSettings::default(),
			go: monochange_core::EcosystemSettings::default(),
		}
	}

	fn severity(severity: LintSeverity) -> LintRuleConfig {
		LintRuleConfig::Severity(severity)
	}

	fn detailed(options: BTreeMap<String, serde_json::Value>) -> LintRuleConfig {
		LintRuleConfig::Detailed {
			level: LintSeverity::Error,
			options,
		}
	}

	fn must<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
		result.unwrap_or_else(|error| panic!("{context}: {error}"))
	}

	fn run_rule<R>(rule: &R, file: &ChangesetLintFile, config: &LintRuleConfig) -> Vec<LintResult>
	where
		R: LintRuleRunner,
	{
		let metadata = metadata();
		let manifest_path = Path::new(".changeset/change.md");
		let ctx = LintContext {
			workspace_root: Path::new("."),
			manifest_path,
			contents: &file.body,
			metadata: &metadata,
			parsed: file,
		};
		rule.run(&ctx, config)
	}

	fn run_rule_with_wrong_parsed<R>(rule: &R, config: &LintRuleConfig) -> Vec<LintResult>
	where
		R: LintRuleRunner,
	{
		let metadata = metadata();
		let parsed = 42_u8;
		let ctx = LintContext {
			workspace_root: Path::new("."),
			manifest_path: Path::new(".changeset/change.md"),
			contents: "# Summary",
			metadata: &metadata,
			parsed: &parsed,
		};
		rule.run(&ctx, config)
	}

	#[test]
	fn parse_changeset_for_lint_parses_frontmatter_shapes() {
		let parsed = parse_changeset_for_lint(
			"---\r\ncore: patch\r\ncli: feature\r\napi:\r\n  bump: minor\r\n  type: migration\r\nempty: ''\r\n---\r\n\r\n# Ship changes\r\n\r\nBody\r\n",
		)
		.expect("expected changeset to parse");
		assert_eq!(parsed.0, "# Ship changes\n\nBody");
		assert_eq!(parsed.1.len(), 4);
		assert!(
			parsed.1.iter().any(|entry| {
				entry.package == "core" && entry.bump == Some(BumpSeverity::Patch)
			})
		);
		assert!(parsed.1.iter().any(|entry| {
			entry.package == "cli" && entry.change_type.as_deref() == Some("feature")
		}));
		assert!(parsed.1.iter().any(|entry| {
			entry.package == "api"
				&& entry.bump == Some(BumpSeverity::Minor)
				&& entry.change_type.as_deref() == Some("migration")
		}));
		assert!(parsed.1.iter().any(|entry| {
			entry.package == "empty" && entry.bump.is_none() && entry.change_type.is_none()
		}));
	}

	#[test]
	fn parse_changeset_for_lint_rejects_non_changesets() {
		assert!(parse_changeset_for_lint("# No frontmatter").is_none());
		assert!(parse_changeset_for_lint("---\nnot: [valid\n---\n# Broken").is_none());
		assert!(parse_changeset_for_lint("---\n[not-a-map]\n---\n# Broken").is_none());
		assert!(parse_changeset_for_lint("---\n123: patch\n---\n# Broken").is_none());
	}

	#[test]
	fn lint_suite_exposes_changeset_rules_and_presets() {
		let suite = ChangesetLintSuite::new();
		assert_eq!(suite.suite_id(), "changesets");

		let ids = suite
			.rules()
			.into_iter()
			.map(|rule| rule.rule().id.clone())
			.collect::<Vec<_>>();
		assert!(ids.iter().any(|id| id == "changesets/summary"));
		assert!(ids.iter().any(|id| id == "changesets/no_section_headings"));
		assert!(ids.iter().any(|id| id == "changesets/bump/none"));
		assert!(ids.iter().any(|id| id == "changesets/bump/patch"));
		assert!(ids.iter().any(|id| id == "changesets/bump/minor"));
		assert!(ids.iter().any(|id| id == "changesets/bump/major"));

		let presets = suite.presets();
		assert!(presets.iter().any(|preset| {
			preset.id == "changesets/recommended" && preset.rules.contains_key("changesets/summary")
		}));
	}

	#[test]
	fn collect_targets_filters_and_parses_changeset_files() {
		let tempdir = must(tempfile::tempdir(), "tempdir");
		let changeset_dir = tempdir.path().join(".changeset");
		must(fs::create_dir_all(&changeset_dir), "changeset dir");
		must(
			fs::write(
				changeset_dir.join("change.md"),
				"---\ncore: patch\n---\n\n# Add target\n",
			),
			"write changeset",
		);
		must(
			fs::write(changeset_dir.join("README.md"), "# Readme"),
			"write readme",
		);
		must(
			fs::write(changeset_dir.join("ignored"), "ignored"),
			"write no extension",
		);
		must(
			fs::write(changeset_dir.join("ignored.txt"), "ignored"),
			"write txt",
		);
		must(
			fs::write(changeset_dir.join("not-a-change.md"), "# No frontmatter"),
			"write markdown",
		);

		let configuration = workspace_configuration(tempdir.path());
		let targets = must(
			ChangesetLintSuite::new().collect_targets(tempdir.path(), &configuration),
			"collect targets",
		);
		assert_eq!(targets.len(), 1);
		let target = targets.first().expect("target");
		assert_eq!(target.metadata.ecosystem, "changesets");
		assert_eq!(
			target.metadata.relative_path,
			PathBuf::from(".changeset/change.md")
		);
		let parsed = target
			.parsed
			.downcast_ref::<ChangesetLintFile>()
			.expect("changeset lint file");
		assert_eq!(parsed.body, "# Add target");
		assert!(
			parsed.changes.iter().any(|entry| {
				entry.package == "core" && entry.bump == Some(BumpSeverity::Patch)
			})
		);
	}

	#[test]
	fn collect_targets_handles_missing_and_invalid_changeset_directories() {
		let tempdir = must(tempfile::tempdir(), "tempdir");
		let configuration = workspace_configuration(tempdir.path());
		let targets = must(
			ChangesetLintSuite::new().collect_targets(tempdir.path(), &configuration),
			"missing changeset dir is fine",
		);
		assert!(targets.is_empty());

		must(
			fs::write(tempdir.path().join(".changeset"), "not a directory"),
			"write file",
		);
		let error = ChangesetLintSuite::new()
			.collect_targets(tempdir.path(), &configuration)
			.expect_err("file changeset path should fail read_dir");
		assert!(
			error
				.to_string()
				.contains("failed to read changeset directory")
		);

		let unreadable_tempdir = must(tempfile::tempdir(), "tempdir");
		let unreadable_changeset_dir = unreadable_tempdir.path().join(".changeset");
		must(
			fs::create_dir_all(unreadable_changeset_dir.join("directory.md")),
			"directory with markdown extension",
		);
		let configuration = workspace_configuration(unreadable_tempdir.path());
		let error = ChangesetLintSuite::new()
			.collect_targets(unreadable_tempdir.path(), &configuration)
			.expect_err("directory changeset path should fail read_to_string");
		assert!(error.to_string().contains("failed to read changeset file"));
	}

	#[test]
	fn summary_rule_respects_disabled_and_wrong_target_types() {
		let rule = SummaryRule::new();
		let file = lint_file("", Vec::new());
		let mut options = BTreeMap::new();
		options.insert("required".to_string(), json!(true));
		assert!(run_rule(&rule, &file, &severity(LintSeverity::Off)).is_empty());
		assert!(run_rule_with_wrong_parsed(&rule, &detailed(options)).is_empty());
	}

	#[test]
	fn summary_rule_requires_first_body_line_to_be_heading() {
		let rule = SummaryRule::new();
		let mut options = BTreeMap::new();
		options.insert("required".to_string(), json!(true));
		let config = detailed(options);

		let empty_results = run_rule(&rule, &lint_file("", Vec::new()), &config);
		assert!(empty_results.iter().any(|result| {
			result.message == "changeset body must start with a summary heading"
		}));

		let paragraph_results =
			run_rule(&rule, &lint_file("summary paragraph", Vec::new()), &config);
		assert!(paragraph_results.iter().any(|result| {
			result.message == "changeset body must start with a summary heading"
		}));
	}

	#[test]
	fn summary_rule_enforces_heading_level_one_by_configuration() {
		let rule = SummaryRule::new();
		let mut options = BTreeMap::new();
		options.insert("required".to_string(), json!(true));
		options.insert("heading_level".to_string(), json!(1));
		let results = run_rule(
			&rule,
			&lint_file("#### Too deep", Vec::new()),
			&detailed(options),
		);
		assert!(results.iter().any(|result| {
			result
				.message
				.contains("changeset summary heading must use level 1, found level 4")
		}));
	}

	#[test]
	fn summary_rule_reports_length_period_and_prefix_issues_together() {
		let rule = SummaryRule::new();
		let mut options = BTreeMap::new();
		options.insert("min_length".to_string(), json!(30));
		options.insert("max_length".to_string(), json!(5));
		options.insert("forbid_trailing_period".to_string(), json!(true));
		options.insert("forbid_conventional_commit_prefix".to_string(), json!(true));
		let results = run_rule(
			&rule,
			&lint_file("# feat: add.", Vec::new()),
			&detailed(options),
		);

		assert!(results.iter().any(|result| {
			result.message == "changeset summary must be at least 30 characters"
		}));
		assert!(
			results.iter().any(|result| {
				result.message == "changeset summary must be at most 5 characters"
			})
		);
		assert!(
			results
				.iter()
				.any(|result| { result.message == "changeset summary must not end with a period" })
		);
		assert!(results.iter().any(|result| {
			result.message == "changeset summary must not use a conventional-commit prefix"
		}));
	}

	#[test]
	fn no_section_headings_rule_reports_unique_change_type_headings() {
		let rule = NoSectionHeadingsRule::new();
		let file = lint_file(
			"# Summary\n\n## feature\n\nDetails",
			vec![
				change("core", Some(BumpSeverity::Patch), Some("feature")),
				change("cli", Some(BumpSeverity::Patch), Some("feature")),
				change("api", Some(BumpSeverity::Patch), None),
			],
		);
		let results = run_rule(&rule, &file, &severity(LintSeverity::Error));
		assert_eq!(results.len(), 1);
		assert!(results.iter().any(|result| {
			result.message == "changeset type `feature` must not also be used as a heading"
		}));
		assert!(run_rule(&rule, &file, &severity(LintSeverity::Off)).is_empty());
		assert!(run_rule_with_wrong_parsed(&rule, &severity(LintSeverity::Error)).is_empty());
	}

	#[test]
	fn bump_scope_rule_reports_all_constraints_for_matching_changes() {
		let rule = BumpScopeRule::new(BumpSeverity::Patch);
		let file = lint_file(
			"# Summary\n\n## Forbidden\n\nA body without the requested section.",
			vec![
				change("core", Some(BumpSeverity::Patch), Some("feature")),
				change("cli", Some(BumpSeverity::Minor), Some("feature")),
			],
		);
		let mut options = BTreeMap::new();
		options.insert("required_bump".to_string(), json!("minor"));
		options.insert("required_sections".to_string(), json!(["Motivation", 7]));
		options.insert("forbidden_headings".to_string(), json!(["Forbidden"]));
		options.insert("min_body_chars".to_string(), json!(200));
		options.insert("max_body_chars".to_string(), json!(10));
		options.insert("require_code_block".to_string(), json!(true));

		let results = run_rule(&rule, &file, &detailed(options));
		assert!(results.iter().any(|result| {
			result
				.message
				.contains("changeset type `feature` requires bump `minor`, found `patch`")
		}));
		assert!(
			results.iter().any(|result| {
				result.message == "changeset must include a `Motivation` section"
			})
		);
		assert!(
			results.iter().any(|result| {
				result.message == "changeset must not use `Forbidden` as a heading"
			})
		);
		assert!(
			results.iter().any(|result| {
				result.message == "changeset body must be at least 200 characters"
			})
		);
		assert!(
			results
				.iter()
				.any(|result| { result.message == "changeset body must be at most 10 characters" })
		);
		assert!(
			results
				.iter()
				.any(|result| { result.message == "changeset must include a fenced code block" })
		);
	}

	#[test]
	fn bump_scope_rule_ignores_non_matching_changes_and_accepts_valid_body() {
		let rule = BumpScopeRule::new(BumpSeverity::Major);
		let file = lint_file(
			"# Summary\n\n## Motivation\n\n```rust\nlet ok = true;\n```",
			vec![change("core", Some(BumpSeverity::Patch), Some("feature"))],
		);
		let mut options = BTreeMap::new();
		options.insert("required_bump".to_string(), json!("major"));
		options.insert("required_sections".to_string(), json!(["Motivation"]));
		options.insert("forbidden_headings".to_string(), json!(["Forbidden"]));
		options.insert("min_body_chars".to_string(), json!(1));
		options.insert("max_body_chars".to_string(), json!(200));
		options.insert("require_code_block".to_string(), json!(true));
		assert!(run_rule(&rule, &file, &detailed(options)).is_empty());
		assert!(run_rule(&rule, &file, &severity(LintSeverity::Off)).is_empty());
		assert!(run_rule_with_wrong_parsed(&rule, &severity(LintSeverity::Error)).is_empty());
	}

	#[test]
	fn lint_rule_config_extension_reads_bool_and_string_list_options() {
		let mut options = BTreeMap::new();
		options.insert("enabled".to_string(), json!(true));
		options.insert("names".to_string(), json!(["one", 2, "two"]));
		let config = detailed(options);

		assert!(<LintRuleConfig as LintRuleConfigExt>::bool_option(
			&config, "enabled", false
		));
		assert!(<LintRuleConfig as LintRuleConfigExt>::bool_option(
			&config, "missing", true
		));
		let names = <LintRuleConfig as LintRuleConfigExt>::string_list_option(&config, "names")
			.expect("string list option");
		assert_eq!(names, vec!["one".to_string(), "two".to_string()]);
		assert!(
			<LintRuleConfig as LintRuleConfigExt>::string_list_option(&config, "missing").is_none()
		);
	}
}
