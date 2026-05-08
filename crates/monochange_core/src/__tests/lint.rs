use std::path::Path;

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
fn noop_lint_progress_reporter_methods_are_noops() {
	let reporter = NoopLintProgressReporter;
	let path = Path::new("Cargo.toml");
	reporter.planning_started(&["cargo"]);
	reporter.planning_finished(1, 1);
	reporter.suite_started("cargo", 1, 1);
	reporter.suite_finished("cargo", 1, 1);
	reporter.file_started(path, 1);
	reporter.file_rule_started(path, "cargo/example");
	reporter.file_rule_finished(path, "cargo/example", 1);
	reporter.file_finished(path, 1);
	reporter.fix_started(1);
	reporter.fix_applied(path, "fixed");
	reporter.fix_finished(1);
	reporter.summary(1, 1, 1, true);
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
