use monochange_config::load_workspace_configuration;
use monochange_test_helpers::fixture_path;
use serde_json::json;

use super::*;

fn npm_target(contents: &str, managed: bool, private: bool) -> LintTarget {
	LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("./package.json").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "npm".to_string(),
			relative_path: Path::new("package.json").to_path_buf(),
			package_name: Some("example".to_string()),
			package_id: managed.then(|| "example".to_string()),
			group_id: None,
			managed,
			private: Some(private),
			publishable: Some(!private),
		},
		Box::new(NpmLintFile {
			manifest: serde_json::from_str(contents).unwrap(),
			workspace_package_names: Arc::new(BTreeSet::from([
				"@scope/internal".to_string(),
				"shared".to_string(),
			])),
		}),
	)
}

fn config() -> LintRuleConfig {
	LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fix".to_string(), json!(true))]),
	}
}

#[test]
fn presets_are_exposed() {
	let presets = NpmLintSuite.presets();
	assert_eq!(presets.len(), 2);
	assert_eq!(
		presets.first().map(|preset| preset.id.as_str()),
		Some("npm/recommended")
	);
	assert_eq!(
		presets.get(1).map(|preset| preset.id.as_str()),
		Some("npm/strict")
	);
}

#[test]
fn workspace_protocol_rule_reports_internal_ranges() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "@scope/internal": "^1.0.0"
  }
}"#,
		true,
		false,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = WorkspaceProtocolRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn sorted_dependencies_rule_reports_unsorted_sections() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "zzz": "1",
    "aaa": "1"
  }
}"#,
		true,
		false,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = SortedDependenciesRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
}

#[test]
fn required_package_fields_rule_supports_custom_fields() {
	let target = npm_target(r#"{"name":"example","description":"ok"}"#, true, false);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fields".to_string(), json!(["description", "license"]))]),
	};
	let results = RequiredPackageFieldsRule::new().run(&ctx, &config);
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("license")
	);
}

#[test]
fn root_no_prod_deps_rule_moves_dependencies() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "left-pad": "1"
  }
}"#,
		true,
		false,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = RootNoProdDepsRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn no_duplicate_dependencies_rule_prefers_dev_dependencies() {
	let target = npm_target(
		r#"{
  "name": "example",
  "dependencies": {
    "shared": "1"
  },
  "devDependencies": {
    "shared": "1"
  }
}"#,
		true,
		false,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = NoDuplicateDependenciesRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn unlisted_package_private_rule_reports_for_public_unmanaged_packages() {
	let target = npm_target(r#"{"name":"example"}"#, false, false);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = UnlistedPackagePrivateRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn collect_targets_marks_configured_packages_as_managed() {
	let root = fixture_path!("cli-output/discover-mixed");
	let configuration = load_workspace_configuration(&root).unwrap();
	let targets = NpmLintSuite.collect_targets(&root, &configuration).unwrap();
	assert!(
		targets
			.iter()
			.all(|target| target.metadata.ecosystem == "npm")
	);
	assert!(targets.iter().any(|target| target.metadata.managed));
}
