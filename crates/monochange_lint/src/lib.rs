#![forbid(clippy::indexing_slicing)]

//! # `monochange_lint`
//!
//! Ecosystem-agnostic manifest lint engine for monochange.
//!
//! Ecosystem crates contribute lint suites, rules, presets, and parsed lint
//! targets. This crate is intentionally unaware of which ecosystems exist.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::path::PathBuf;

use glob::Pattern;
use ignore::gitignore::Gitignore;
use ignore::gitignore::GitignoreBuilder;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintProgressReporter;
use monochange_core::lint::LintReport;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRegistry;
use monochange_core::lint::LintSelector;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_core::lint::NoopLintProgressReporter;
use monochange_core::lint::WorkspaceLintSettings;

/// Optional filters applied when running the linter.
#[derive(Debug, Clone, Default)]
pub struct LintSelection {
	suites: BTreeSet<String>,
	only_rules: BTreeSet<String>,
}

impl LintSelection {
	/// Create an unconstrained selection.
	#[must_use]
	pub fn all() -> Self {
		Self::default()
	}

	/// Limit execution to the provided suites.
	#[must_use]
	pub fn with_suites(mut self, suites: impl IntoIterator<Item = impl Into<String>>) -> Self {
		self.suites = suites.into_iter().map(Into::into).collect();
		self
	}

	/// Limit execution to the provided rules.
	#[must_use]
	pub fn with_rules(mut self, rules: impl IntoIterator<Item = impl Into<String>>) -> Self {
		self.only_rules = rules.into_iter().map(Into::into).collect();
		self
	}

	#[must_use]
	pub fn allows_suite(&self, suite_id: &str) -> bool {
		self.suites.is_empty() || self.suites.contains(suite_id)
	}

	#[must_use]
	pub fn allows_rule(&self, rule_id: &str) -> bool {
		self.only_rules.is_empty() || self.only_rules.contains(rule_id)
	}
}

/// Registered lint suites, rules, and presets.
#[derive(Default)]
pub struct LintRegistry {
	rules: LintRuleRegistry,
	presets: BTreeMap<String, LintPreset>,
	suites: BTreeMap<String, Box<dyn LintSuite>>,
}

impl std::fmt::Debug for LintRegistry {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LintRegistry")
			.field("rule_count", &self.rules.rules().len())
			.field("preset_count", &self.presets.len())
			.field("suite_count", &self.suites.len())
			.finish()
	}
}

impl LintRegistry {
	/// Build a registry from ecosystem-provided suites.
	#[must_use]
	pub fn new(suites: Vec<Box<dyn LintSuite>>) -> Self {
		let mut registry = Self::default();
		for suite in suites {
			registry.register_suite(suite);
		}
		registry
	}

	/// Register one suite.
	pub fn register_suite(&mut self, suite: Box<dyn LintSuite>) {
		let suite_id = suite.suite_id().to_string();
		for preset in suite.presets() {
			self.presets.insert(preset.id.clone(), preset);
		}
		for rule in suite.rules() {
			self.rules.register(rule);
		}
		self.suites.insert(suite_id, suite);
	}

	/// Return cloned rule metadata for display commands.
	#[must_use]
	pub fn rules(&self) -> Vec<LintRule> {
		self.rules
			.rules()
			.iter()
			.map(|rule| rule.rule().clone())
			.collect()
	}

	/// Return cloned preset metadata.
	#[must_use]
	pub fn presets(&self) -> Vec<LintPreset> {
		self.presets.values().cloned().collect()
	}

	/// Find a rule by id.
	#[must_use]
	pub fn find_rule(&self, id: &str) -> Option<LintRule> {
		self.rules.find(id).map(|rule| rule.rule().clone())
	}

	/// Find a preset by id.
	#[must_use]
	pub fn find_preset(&self, id: &str) -> Option<LintPreset> {
		self.presets.get(id).cloned()
	}
}

/// Run lint suites against workspace manifests.
#[derive(Debug)]
pub struct Linter {
	registry: LintRegistry,
	settings: WorkspaceLintSettings,
	selection: LintSelection,
}

impl Linter {
	/// Create a linter from registered suites and workspace settings.
	#[must_use]
	pub fn new(suites: Vec<Box<dyn LintSuite>>, settings: WorkspaceLintSettings) -> Self {
		Self {
			registry: LintRegistry::new(suites),
			settings,
			selection: LintSelection::all(),
		}
	}

	/// Override the current selection filters.
	#[must_use]
	pub fn with_selection(mut self, selection: LintSelection) -> Self {
		self.selection = selection;
		self
	}

	/// Access the rule and preset registry.
	#[must_use]
	pub fn registry(&self) -> &LintRegistry {
		&self.registry
	}

	/// Lint all suite targets in the workspace.
	#[must_use]
	pub fn lint_workspace(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
		reporter: &dyn LintProgressReporter,
	) -> LintReport {
		let mut report = LintReport::new();
		self.warn_for_missing_presets(&mut report);
		let gitignore_filter =
			(!self.settings.disable_gitignore).then(|| build_gitignore_filter(workspace_root));

		let active_suites: Vec<&str> = self
			.registry
			.suites
			.keys()
			.filter(|suite_id| self.selection.allows_suite(suite_id))
			.map(String::as_str)
			.collect();
		reporter.planning_started(&active_suites);

		let mut total_files = 0usize;
		let mut total_rules = 0usize;
		let mut suite_targets: Vec<(String, Vec<LintTarget>)> = Vec::new();

		for (suite_id, suite) in &self.registry.suites {
			if !self.selection.allows_suite(suite_id) {
				continue;
			}

			let targets = match suite.collect_targets(workspace_root, configuration) {
				Ok(targets) => targets,
				Err(error) => {
					report.warn(format!(
						"failed to collect lint targets for suite `{suite_id}`: {error}"
					));
					continue;
				}
			};

			let applicable_rules = suite
				.rules()
				.iter()
				.filter(|rule| self.selection.allows_rule(&rule.rule().id))
				.count();

			total_files += targets.len();
			total_rules += applicable_rules * targets.len();
			suite_targets.push((suite_id.clone(), targets));
		}

		reporter.planning_finished(total_files, total_rules);

		for (suite_id, targets) in &suite_targets {
			let fallback = LintTarget {
				workspace_root: workspace_root.to_path_buf(),
				manifest_path: workspace_root.join("Cargo.toml"),
				contents: String::new(),
				metadata: monochange_core::lint::LintTargetMetadata::default(),
				parsed: Box::new(()),
			};
			let any_target = targets.first().unwrap_or(&fallback);
			let applicable_rules = self.registry.rules.applicable_rules(any_target);
			reporter.suite_started(suite_id, targets.len(), applicable_rules.len());

			let mut suite_result_count = 0usize;
			let mut suite_fixable = 0usize;

			for target in targets {
				if !self.target_is_included(target, gitignore_filter.as_ref()) {
					continue;
				}

				let target_report = self.lint_target_with_reporter(target, reporter);
				suite_result_count += target_report.results.len();
				suite_fixable += target_report.autofixable().len();
				report.merge(target_report);
			}

			reporter.suite_finished(suite_id, suite_result_count, suite_fixable);
		}

		report
	}

	/// Apply autofixes from a lint report.
	#[must_use]
	pub fn apply_fixes(&self, report: &LintReport) -> BTreeMap<PathBuf, String> {
		let mut fixes_by_file: BTreeMap<PathBuf, Vec<LintFix>> = BTreeMap::new();
		for result in report.autofixable() {
			if let Some(fix) = &result.fix {
				fixes_by_file
					.entry(result.location.file_path.clone())
					.or_default()
					.push(fix.clone());
			}
		}

		let mut fixed_files = BTreeMap::new();
		for (file_path, fixes) in fixes_by_file {
			let Ok(contents) = std::fs::read_to_string(&file_path) else {
				continue;
			};
			fixed_files.insert(file_path, apply_fixes_to_content(&contents, &fixes));
		}

		fixed_files
	}

	fn lint_target_with_reporter(
		&self,
		target: &LintTarget,
		reporter: &dyn LintProgressReporter,
	) -> LintReport {
		let mut report = LintReport::new();
		let applicable_rules = self.registry.rules.applicable_rules(target);
		if applicable_rules.is_empty() {
			return report;
		}

		reporter.file_started(&target.manifest_path, applicable_rules.len());

		for rule in applicable_rules {
			let rule_id = rule.rule().id.as_str();
			if !self.selection.allows_rule(rule_id) {
				continue;
			}

			let config = self
				.resolve_rule_config(target, rule_id)
				.unwrap_or(LintRuleConfig::Severity(LintSeverity::Error));
			if !config.severity().is_enabled() {
				continue;
			}

			let ctx = LintContext {
				workspace_root: &target.workspace_root,
				manifest_path: &target.manifest_path,
				contents: &target.contents,
				metadata: &target.metadata,
				parsed: target.parsed.as_ref(),
			};

			reporter.file_rule_started(&target.manifest_path, rule_id);
			let mut rule_results = Vec::new();
			for mut result in rule.run(&ctx, &config) {
				result.severity = config.severity();
				rule_results.push(result);
			}
			reporter.file_rule_finished(&target.manifest_path, rule_id, rule_results.len());
			for result in rule_results {
				report.add(result);
			}
		}

		reporter.file_finished(&target.manifest_path, report.results.len());
		report
	}

	/// Lint a single target without progress reporting (convenience).
	#[allow(dead_code)]
	fn lint_target(&self, target: &LintTarget) -> LintReport {
		self.lint_target_with_reporter(target, &NoopLintProgressReporter)
	}

	fn resolve_rule_config(&self, target: &LintTarget, rule_id: &str) -> Option<LintRuleConfig> {
		let mut resolved = None;
		for preset_id in &self.settings.presets {
			resolved = merge_config(
				resolved,
				self.registry
					.presets
					.get(preset_id)
					.and_then(|preset| preset.rules.get(rule_id)),
			);
		}
		resolved = merge_config(resolved, self.settings.rules.get(rule_id));

		for scope in &self.settings.scopes {
			if !selector_matches(&scope.selector, target) {
				continue;
			}
			for preset_id in &scope.presets {
				resolved = merge_config(
					resolved,
					self.registry
						.presets
						.get(preset_id)
						.and_then(|preset| preset.rules.get(rule_id)),
				);
			}
			resolved = merge_config(resolved, scope.rules.get(rule_id));
		}

		resolved
	}

	fn warn_for_missing_presets(&self, report: &mut LintReport) {
		for preset_id in self.settings.presets.iter().chain(
			self.settings
				.scopes
				.iter()
				.flat_map(|scope| scope.presets.iter()),
		) {
			if !self.registry.presets.contains_key(preset_id) {
				report.warn(format!("unknown lint preset `{preset_id}`"));
			}
		}
	}

	fn target_is_included(
		&self,
		target: &LintTarget,
		gitignore_filter: Option<&Gitignore>,
	) -> bool {
		if gitignore_filter.is_some_and(|filter| {
			gitignore_path_is_excluded(filter, &target.workspace_root, &target.manifest_path)
		}) {
			return false;
		}

		let relative = target.metadata.relative_path.to_string_lossy();
		if !self.settings.include.is_empty()
			&& !self
				.settings
				.include
				.iter()
				.any(|pattern| lint_path_pattern_matches(pattern, relative.as_ref(), "include"))
		{
			return false;
		}

		if self
			.settings
			.exclude
			.iter()
			.any(|pattern| lint_path_pattern_matches(pattern, relative.as_ref(), "exclude"))
		{
			return false;
		}

		true
	}
}

fn lint_path_pattern_matches(pattern: &str, relative_path: &str, kind: &str) -> bool {
	Pattern::new(pattern).map_or_else(
		|error| {
			tracing::warn!(pattern, kind, error = %error, "invalid lint path pattern");
			false
		},
		|pattern| pattern.matches(relative_path),
	)
}

fn build_gitignore_filter(workspace_root: &Path) -> Gitignore {
	let mut builder = GitignoreBuilder::new(workspace_root);
	for path in [
		workspace_root.join(".gitignore"),
		workspace_root.join(".git/info/exclude"),
	] {
		if path.is_file() {
			let _ = builder.add(path);
		}
	}
	builder.build().unwrap_or_else(|_| Gitignore::empty())
}

fn gitignore_path_is_excluded(
	filter: &Gitignore,
	workspace_root: &Path,
	manifest_path: &Path,
) -> bool {
	manifest_path
		.strip_prefix(workspace_root)
		.ok()
		.is_some_and(|relative| {
			filter
				.matched_path_or_any_parents(relative, false)
				.is_ignore()
		})
}

fn merge_config(
	current: Option<LintRuleConfig>,
	next: Option<&LintRuleConfig>,
) -> Option<LintRuleConfig> {
	match (current, next) {
		(None, None) => None,
		(Some(config), None) => Some(config),
		(None, Some(config)) => Some(config.clone()),
		(Some(current), Some(next)) => Some(current.merged_with(next)),
	}
}

fn selector_matches(selector: &LintSelector, target: &LintTarget) -> bool {
	if !selector.ecosystems.is_empty()
		&& !selector
			.ecosystems
			.iter()
			.any(|ecosystem| ecosystem == &target.metadata.ecosystem)
	{
		return false;
	}

	if !selector.paths.is_empty() {
		let relative = target.metadata.relative_path.to_string_lossy();
		let matches_path = selector.paths.iter().any(|pattern| {
			Pattern::new(pattern).map_or_else(
				|error| {
					tracing::warn!(pattern, error = %error, "invalid lint scope path pattern");
					false
				},
				|pattern| pattern.matches(relative.as_ref()),
			)
		});
		if !matches_path {
			return false;
		}
	}

	if !selector.package_ids.is_empty()
		&& !target
			.metadata
			.package_id
			.as_ref()
			.is_some_and(|package_id| {
				selector
					.package_ids
					.iter()
					.any(|candidate| candidate == package_id)
			}) {
		return false;
	}

	if !selector.group_ids.is_empty()
		&& !target.metadata.group_id.as_ref().is_some_and(|group_id| {
			selector
				.group_ids
				.iter()
				.any(|candidate| candidate == group_id)
		}) {
		return false;
	}

	if let Some(managed) = selector.managed
		&& target.metadata.managed != managed
	{
		return false;
	}

	if let Some(private) = selector.private
		&& target.metadata.private != Some(private)
	{
		return false;
	}

	if let Some(publishable) = selector.publishable
		&& target.metadata.publishable != Some(publishable)
	{
		return false;
	}

	true
}

fn apply_fixes_to_content(contents: &str, fixes: &[LintFix]) -> String {
	let mut edits: Vec<_> = fixes.iter().flat_map(|fix| fix.edits.iter()).collect();
	edits.sort_by_key(|edit| std::cmp::Reverse(edit.span.0));

	let mut result = contents.to_string();
	for edit in edits {
		if edit.span.0 < result.len() && edit.span.1 <= result.len() {
			result.replace_range(edit.span.0..edit.span.1, &edit.replacement);
		}
	}
	result
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use monochange_core::MonochangeResult;
	use monochange_core::lint::LintCategory;
	use monochange_core::lint::LintLocation;
	use monochange_core::lint::LintMaturity;
	use monochange_core::lint::LintResult;
	use monochange_core::lint::LintRuleRunner;
	use monochange_core::lint::LintTargetMetadata;

	use super::*;

	#[derive(Default)]
	struct ExampleSuite;

	struct ExampleRule {
		rule: LintRule,
	}

	impl ExampleRule {
		fn new() -> Self {
			Self {
				rule: LintRule::new(
					"example/no-bad",
					"No bad",
					"Flags files containing the word bad",
					LintCategory::Correctness,
					LintMaturity::Stable,
					false,
				),
			}
		}
	}

	impl LintRuleRunner for ExampleRule {
		fn rule(&self) -> &LintRule {
			&self.rule
		}

		fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
			if ctx.contents.contains("bad") {
				vec![LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					"found bad",
					config.severity(),
				)]
			} else {
				Vec::new()
			}
		}
	}

	impl LintSuite for ExampleSuite {
		fn suite_id(&self) -> &'static str {
			"example"
		}

		fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
			vec![Box::new(ExampleRule::new())]
		}

		fn presets(&self) -> Vec<LintPreset> {
			vec![
				LintPreset::new(
					"example/recommended",
					"Example recommended",
					"Recommended example lints",
					LintMaturity::Stable,
				)
				.with_rules(BTreeMap::from([(
					"example/no-bad".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				)])),
			]
		}

		fn collect_targets(
			&self,
			workspace_root: &Path,
			_configuration: &WorkspaceConfiguration,
		) -> MonochangeResult<Vec<LintTarget>> {
			Ok(vec![LintTarget::new(
				workspace_root.to_path_buf(),
				workspace_root.join("example.txt"),
				"this is bad",
				LintTargetMetadata {
					ecosystem: "example".to_string(),
					relative_path: PathBuf::from("example.txt"),
					package_name: None,
					package_id: None,
					group_id: None,
					managed: false,
					private: None,
					publishable: None,
				},
				Box::new(()),
			)])
		}
	}

	#[derive(Default)]
	struct FailingSuite;

	#[derive(Default)]
	struct EmptyTargetSuite;

	impl LintSuite for FailingSuite {
		fn suite_id(&self) -> &'static str {
			"failing"
		}

		fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
			Vec::new()
		}

		fn collect_targets(
			&self,
			_workspace_root: &Path,
			_configuration: &WorkspaceConfiguration,
		) -> MonochangeResult<Vec<LintTarget>> {
			Err(monochange_core::MonochangeError::Config("boom".to_string()))
		}
	}

	impl LintSuite for EmptyTargetSuite {
		fn suite_id(&self) -> &'static str {
			"empty-target"
		}

		fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
			Vec::new()
		}

		fn collect_targets(
			&self,
			workspace_root: &Path,
			_configuration: &WorkspaceConfiguration,
		) -> MonochangeResult<Vec<LintTarget>> {
			Ok(vec![LintTarget::new(
				workspace_root.to_path_buf(),
				workspace_root.join("empty.txt"),
				"fine",
				LintTargetMetadata {
					ecosystem: "empty-target".to_string(),
					relative_path: PathBuf::from("empty.txt"),
					package_name: None,
					package_id: None,
					group_id: None,
					managed: false,
					private: None,
					publishable: None,
				},
				Box::new(()),
			)])
		}
	}

	fn sample_workspace_configuration(root: &Path) -> WorkspaceConfiguration {
		WorkspaceConfiguration {
			root_path: root.to_path_buf(),
			defaults: monochange_core::WorkspaceDefaults::default(),
			changelog: monochange_core::ChangelogSettings::default(),
			packages: Vec::new(),
			groups: Vec::new(),
			cli: Vec::new(),
			changesets: monochange_core::ChangesetSettings::default(),
			source: None,
			lints: WorkspaceLintSettings::default(),
			cargo: monochange_core::EcosystemSettings::default(),
			npm: monochange_core::EcosystemSettings::default(),
			deno: monochange_core::EcosystemSettings::default(),
			dart: monochange_core::EcosystemSettings::default(),
			python: monochange_core::EcosystemSettings::default(),
			go: monochange_core::EcosystemSettings::default(),
		}
	}

	#[test]
	fn linter_runs_preset_backed_rules() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert_eq!(report.error_count, 1);
		assert_eq!(report.results.len(), 1);
	}

	#[test]
	fn linter_lint_target_uses_noop_reporter_convenience() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let target = ExampleSuite
			.collect_targets(root.path(), &configuration)
			.unwrap_or_else(|error| panic!("expected example suite targets: {error}"))
			.into_iter()
			.next()
			.unwrap_or_else(|| panic!("expected a target"));
		let report = linter.lint_target(&target);
		assert_eq!(report.error_count, 1);
		assert_eq!(report.results.len(), 1);
	}

	#[test]
	fn scoped_rule_override_can_disable_a_rule() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			scopes: vec![monochange_core::lint::LintScopeConfig {
				name: Some("turn it off".to_string()),
				selector: LintSelector {
					ecosystems: vec!["example".to_string()],
					paths: vec!["*.txt".to_string()],
					package_ids: Vec::new(),
					group_ids: Vec::new(),
					managed: None,
					private: None,
					publishable: None,
				},
				presets: Vec::new(),
				rules: BTreeMap::from([(
					"example/no-bad".to_string(),
					LintRuleConfig::Severity(LintSeverity::Off),
				)]),
			}],
			rules: BTreeMap::new(),
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(report.results.is_empty());
	}

	#[test]
	fn linter_warns_about_unknown_presets() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["missing/preset".to_string()],
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(
			report
				.warnings
				.iter()
				.any(|warning| warning.contains("missing/preset"))
		);
	}

	#[test]
	fn gitignored_targets_are_excluded_by_default() {
		let root = tempfile::tempdir().unwrap();
		std::fs::write(root.path().join(".gitignore"), "example.txt\n")
			.unwrap_or_else(|error| panic!("write .gitignore: {error}"));
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(report.results.is_empty());
	}

	#[test]
	fn disable_gitignore_allows_linting_gitignored_targets() {
		let root = tempfile::tempdir().unwrap();
		std::fs::write(root.path().join(".gitignore"), "example.txt\n")
			.unwrap_or_else(|error| panic!("write .gitignore: {error}"));
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			disable_gitignore: true,
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert_eq!(report.error_count, 1);
	}

	#[test]
	fn include_and_exclude_patterns_filter_targets() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let include_filtered = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			include: vec!["other/**".to_string()],
			..WorkspaceLintSettings::default()
		};
		let include_report = Linter::new(vec![Box::new(ExampleSuite)], include_filtered)
			.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(include_report.results.is_empty());

		let exclude_filtered = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			exclude: vec!["example.*".to_string()],
			..WorkspaceLintSettings::default()
		};
		let exclude_report = Linter::new(vec![Box::new(ExampleSuite)], exclude_filtered)
			.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(exclude_report.results.is_empty());
	}

	#[test]
	fn selection_can_filter_suites_and_rules() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			presets: vec!["example/recommended".to_string()],
			..WorkspaceLintSettings::default()
		};
		let suite_filtered = Linter::new(vec![Box::new(ExampleSuite)], settings.clone())
			.with_selection(LintSelection::all().with_suites(["other"]));
		let suite_report =
			suite_filtered.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(suite_report.results.is_empty());

		let rule_filtered = Linter::new(vec![Box::new(ExampleSuite)], settings)
			.with_selection(LintSelection::all().with_rules(["other/rule"]));
		let rule_report =
			rule_filtered.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(rule_report.results.is_empty());
	}

	#[test]
	fn selector_matches_package_and_publishability_filters() {
		let target = LintTarget::new(
			PathBuf::from("."),
			PathBuf::from("crates/core/Cargo.toml"),
			"",
			LintTargetMetadata {
				ecosystem: "cargo".to_string(),
				relative_path: PathBuf::from("crates/core/Cargo.toml"),
				package_name: Some("core".to_string()),
				package_id: Some("core".to_string()),
				group_id: Some("sdk".to_string()),
				managed: true,
				private: Some(false),
				publishable: Some(true),
			},
			Box::new(()),
		);
		let selector = LintSelector {
			ecosystems: vec!["cargo".to_string()],
			paths: vec!["crates/*/Cargo.toml".to_string()],
			package_ids: vec!["core".to_string()],
			group_ids: vec!["sdk".to_string()],
			managed: Some(true),
			private: Some(false),
			publishable: Some(true),
		};
		assert!(selector_matches(&selector, &target));
	}

	#[test]
	fn registry_debug_and_lookup_helpers_report_counts() {
		let registry = LintRegistry::new(vec![Box::new(ExampleSuite)]);
		let debug = format!("{registry:?}");
		assert!(debug.contains("rule_count"));
		assert!(registry.find_rule("example/no-bad").is_some());
		assert!(registry.find_preset("example/recommended").is_some());
	}

	#[test]
	fn linter_warns_when_suite_target_collection_fails() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let linter = Linter::new(
			vec![Box::new(FailingSuite)],
			WorkspaceLintSettings::default(),
		);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(
			report
				.warnings
				.iter()
				.any(|warning| warning.contains("failed to collect lint targets"))
		);
	}

	#[test]
	fn apply_fixes_skips_missing_files() {
		let root = tempfile::tempdir().unwrap();
		let linter = Linter::new(
			vec![Box::new(ExampleSuite)],
			WorkspaceLintSettings::default(),
		);
		let mut report = LintReport::new();
		report.add(
			LintResult::new(
				"example/no-bad",
				LintLocation::new(root.path().join("missing.txt"), 1, 1).with_span(0, 3),
				"missing",
				LintSeverity::Error,
			)
			.with_fix(LintFix::single("rewrite", (0, 3), "ok")),
		);
		assert!(linter.apply_fixes(&report).is_empty());
	}

	#[test]
	fn merge_config_and_selector_helpers_cover_edge_cases() {
		assert!(merge_config(None, None).is_none());
		assert!(!lint_path_pattern_matches(
			"[",
			"packages/example/package.json",
			"include"
		));
		assert_eq!(
			merge_config(None, Some(&LintRuleConfig::Severity(LintSeverity::Warning)))
				.expect("config")
				.severity(),
			LintSeverity::Warning,
		);
		assert_eq!(
			merge_config(Some(LintRuleConfig::Severity(LintSeverity::Error)), None)
				.expect("config")
				.severity(),
			LintSeverity::Error,
		);

		let target = LintTarget::new(
			".",
			"example.txt",
			"good",
			LintTargetMetadata {
				ecosystem: "example".to_string(),
				relative_path: PathBuf::from("example.txt"),
				package_name: None,
				package_id: Some("pkg".to_string()),
				group_id: Some("grp".to_string()),
				managed: false,
				private: Some(false),
				publishable: Some(true),
			},
			Box::new(()),
		);
		assert!(!selector_matches(
			&LintSelector {
				ecosystems: vec!["cargo".to_string()],
				..LintSelector::default()
			},
			&target,
		));
		assert!(!selector_matches(
			&LintSelector {
				paths: vec!["[".to_string()],
				..LintSelector::default()
			},
			&target,
		));
		assert!(!selector_matches(
			&LintSelector {
				package_ids: vec!["other".to_string()],
				..LintSelector::default()
			},
			&target,
		));
		assert!(!selector_matches(
			&LintSelector {
				group_ids: vec!["other".to_string()],
				..LintSelector::default()
			},
			&target,
		));
		assert!(!selector_matches(
			&LintSelector {
				managed: Some(true),
				..LintSelector::default()
			},
			&target,
		));
		assert!(!selector_matches(
			&LintSelector {
				private: Some(true),
				..LintSelector::default()
			},
			&target,
		));
		assert!(!selector_matches(
			&LintSelector {
				publishable: Some(false),
				..LintSelector::default()
			},
			&target,
		));
	}

	#[test]
	fn scope_presets_and_empty_targets_are_covered() {
		let root = tempfile::tempdir().unwrap();
		let configuration = sample_workspace_configuration(root.path());
		let settings = WorkspaceLintSettings {
			scopes: vec![monochange_core::lint::LintScopeConfig {
				name: Some("preset scope".to_string()),
				selector: LintSelector {
					ecosystems: vec!["example".to_string()],
					paths: vec!["*.txt".to_string()],
					package_ids: Vec::new(),
					group_ids: Vec::new(),
					managed: None,
					private: None,
					publishable: None,
				},
				presets: vec!["example/recommended".to_string()],
				rules: BTreeMap::new(),
			}],
			..WorkspaceLintSettings::default()
		};
		let linter = Linter::new(vec![Box::new(ExampleSuite)], settings);
		let report = linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert_eq!(report.error_count, 1);

		let empty_linter = Linter::new(
			vec![Box::new(EmptyTargetSuite)],
			WorkspaceLintSettings::default(),
		);
		let empty_report =
			empty_linter.lint_workspace(root.path(), &configuration, &NoopLintProgressReporter);
		assert!(empty_report.results.is_empty());
	}

	#[test]
	fn example_rule_returns_no_results_when_contents_are_clean() {
		let target = LintTarget::new(
			".",
			"example.txt",
			"good",
			LintTargetMetadata {
				ecosystem: "example".to_string(),
				relative_path: PathBuf::from("example.txt"),
				package_name: None,
				package_id: None,
				group_id: None,
				managed: false,
				private: None,
				publishable: None,
			},
			Box::new(()),
		);
		let ctx = LintContext {
			workspace_root: target.workspace_root.as_path(),
			manifest_path: target.manifest_path.as_path(),
			contents: &target.contents,
			metadata: &target.metadata,
			parsed: target.parsed.as_ref(),
		};
		assert!(
			ExampleRule::new()
				.run(&ctx, &LintRuleConfig::Severity(LintSeverity::Error))
				.is_empty()
		);
	}
}
