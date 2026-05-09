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
