use monochange_config::load_workspace_configuration;
use monochange_test_helpers::fixture_path;
use serde_json::json;

use super::*;

fn cargo_target(contents: &str, managed: bool, publishable: bool) -> LintTarget {
	LintTarget::new(
		Path::new(".").to_path_buf(),
		Path::new("Cargo.toml").to_path_buf(),
		contents.to_string(),
		LintTargetMetadata {
			ecosystem: "cargo".to_string(),
			relative_path: Path::new("Cargo.toml").to_path_buf(),
			package_name: Some("example".to_string()),
			package_id: managed.then(|| "example".to_string()),
			group_id: None,
			managed,
			private: Some(!publishable),
			publishable: Some(publishable),
		},
		Box::new(CargoLintFile {
			document: contents.parse::<DocumentMut>().unwrap(),
			workspace_package_names: Arc::new(BTreeSet::from([
				"internal_dep".to_string(),
				"serde".to_string(),
			])),
			workspace_package_publishable: Arc::new(BTreeMap::from([
				("internal_dep".to_string(), false),
				("serde".to_string(), true),
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
	let presets = CargoLintSuite.presets();
	assert_eq!(presets.len(), 2);
	assert_eq!(
		presets.first().map(|preset| preset.id.as_str()),
		Some("cargo/recommended")
	);
	assert_eq!(
		presets.get(1).map(|preset| preset.id.as_str()),
		Some("cargo/strict")
	);
}

#[test]
fn dependency_field_order_rule_reports_and_fixes() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dependencies.serde]
features = ["derive"]
workspace = true
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = DependencyFieldOrderRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn internal_dependency_workspace_rule_reports_and_fixes() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dependencies]
internal_dep = { path = "../internal_dep", version = "0.1.0" }
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = InternalDependencyWorkspaceRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("internal dependency `internal_dep`")
	);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn publishable_dependency_rule_reports_unpublished_workspace_deps() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dev-dependencies]
internal_dep = { workspace = true }
serde = { workspace = true }
"#,
		true,
		true,
	);
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	let results = PublishableDependencyRule::new().run(&ctx, &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("unpublished workspace package `internal_dep`")
	);
}

#[test]
fn publishable_dependency_rule_skips_private_packages() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dev-dependencies]
internal_dep = { workspace = true }
"#,
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
	let results = PublishableDependencyRule::new().run(&ctx, &config());
	assert!(results.is_empty());
}

#[test]
fn publishable_dependency_rule_skips_unparsed_targets_and_non_table_sections() {
	let target = cargo_target(
		r#"dependencies = "not a table"

[package]
name = "example"
version = "0.1.0"
"#,
		true,
		true,
	);
	let non_cargo_parsed = "not a Cargo lint file";
	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: &non_cargo_parsed,
	};
	assert!(
		PublishableDependencyRule::new()
			.run(&ctx, &config())
			.is_empty()
	);

	let ctx = LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	};
	assert!(
		PublishableDependencyRule::new()
			.run(&ctx, &config())
			.is_empty()
	);
}

#[test]
fn publishable_dependency_rule_metadata_is_exposed() {
	let rule = PublishableDependencyRule::new();
	assert_eq!(
		LintRuleRunner::rule(&rule).id,
		"cargo/publishable-dependencies"
	);
}

#[test]
fn required_package_fields_rule_supports_custom_fields() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"
description = "ok"
"#,
		true,
		true,
	);
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
fn sorted_dependencies_rule_reports_and_fixes() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"

[dependencies]
zzz = "1"
aaa = "1"
"#,
		true,
		true,
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
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
}

#[test]
fn unlisted_package_private_rule_reports_for_public_unmanaged_packages() {
	let target = cargo_target(
		r#"[package]
name = "example"
version = "0.1.0"
"#,
		false,
		true,
	);
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
	let root = fixture_path!("monochange/release-base");
	let configuration = load_workspace_configuration(&root).unwrap();
	let targets = CargoLintSuite
		.collect_targets(&root, &configuration)
		.unwrap();
	assert!(targets.iter().any(|target| target.metadata.managed));
	assert!(
		targets
			.iter()
			.all(|target| target.metadata.ecosystem == "cargo")
	);
}
