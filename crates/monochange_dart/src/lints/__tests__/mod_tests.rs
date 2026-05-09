use std::path::Path;

use monochange_config::load_workspace_configuration;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_test_helpers::fixture_path;
use serde_json::json;
use serde_yaml_ng::Mapping;

use super::*;

fn metadata_publishability_root() -> std::path::PathBuf {
	fixture_path!("dart-lints/metadata-publishability/workspace")
}

fn metadata_publishability_targets() -> Vec<LintTarget> {
	let root = metadata_publishability_root();
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("load dart lint fixture config: {error}"));
	lint_suite()
		.collect_targets(&root, &configuration)
		.unwrap_or_else(|error| panic!("collect dart lint fixture targets: {error}"))
}

fn sdk_dependency_hygiene_root() -> std::path::PathBuf {
	fixture_path!("dart-lints/sdk-dependency-hygiene/workspace")
}

fn sdk_dependency_hygiene_targets() -> Vec<LintTarget> {
	let root = sdk_dependency_hygiene_root();
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("load dart sdk fixture config: {error}"));
	lint_suite()
		.collect_targets(&root, &configuration)
		.unwrap_or_else(|error| panic!("collect dart sdk fixture targets: {error}"))
}

fn advanced_workspace_flutter_root() -> std::path::PathBuf {
	fixture_path!("dart-lints/advanced-workspace-flutter/workspace")
}

fn advanced_workspace_flutter_targets() -> Vec<LintTarget> {
	let root = advanced_workspace_flutter_root();
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("load dart advanced fixture config: {error}"));
	lint_suite()
		.collect_targets(&root, &configuration)
		.unwrap_or_else(|error| panic!("collect dart advanced fixture targets: {error}"))
}

fn find_target<'a>(targets: &'a [LintTarget], package_name: &str) -> &'a LintTarget {
	targets
		.iter()
		.find(|target| target.metadata.package_name.as_deref() == Some(package_name))
		.unwrap_or_else(|| panic!("missing target for {package_name}"))
}

fn ctx(target: &LintTarget) -> LintContext<'_> {
	LintContext {
		workspace_root: &target.workspace_root,
		manifest_path: &target.manifest_path,
		contents: &target.contents,
		metadata: &target.metadata,
		parsed: target.parsed.as_ref(),
	}
}

fn config() -> LintRuleConfig {
	LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fix".to_string(), json!(true))]),
	}
}

#[test]
fn presets_are_exposed() {
	let presets = DartLintSuite.presets();
	assert_eq!(presets.len(), 2);

	let recommended = presets.first().expect("expected recommended preset");
	assert_eq!(recommended.id, "dart/recommended");
	assert_eq!(
		recommended.rules.get("dart/dependency-sorted"),
		Some(&LintRuleConfig::Severity(LintSeverity::Warning))
	);
	assert_eq!(
		recommended.rules.get("dart/sdk-constraint-present"),
		Some(&LintRuleConfig::Severity(LintSeverity::Error))
	);
	assert!(
		!recommended
			.rules
			.contains_key("dart/internal-path-dependency-policy")
	);

	let strict = presets.get(1).expect("expected strict preset");
	assert_eq!(strict.id, "dart/strict");
	assert!(strict.rules.contains_key("dart/assets-sorted"));
	assert!(
		strict
			.rules
			.contains_key("dart/flutter-package-metadata-consistent")
	);
	assert!(
		strict
			.rules
			.contains_key("dart/workspace-internal-version-consistency")
	);
}

#[test]
fn required_package_fields_rule_reports_missing_metadata_for_managed_publishable_packages() {
	let targets = metadata_publishability_targets();
	let failing = find_target(&targets, "published_missing");
	let passing = find_target(&targets, "published_ok");

	let failing_results = RequiredPackageFieldsRule::new().run(&ctx(failing), &config());
	assert_eq!(failing_results.len(), 2);
	assert!(
		failing_results
			.iter()
			.any(|result| result.message.contains("description"))
	);
	assert!(
		failing_results
			.iter()
			.any(|result| result.message.contains("repository"))
	);

	let passing_results = RequiredPackageFieldsRule::new().run(&ctx(passing), &config());
	assert!(passing_results.is_empty());
}

#[test]
fn required_package_fields_rule_supports_custom_fields() {
	let targets = metadata_publishability_targets();
	let target = find_target(&targets, "published_missing");
	let config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("fields".to_string(), json!(["description"]))]),
	};
	let results = RequiredPackageFieldsRule::new().run(&ctx(target), &config);
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("description")
	);
}

#[test]
fn sdk_constraint_present_rule_reports_missing_constraints() {
	let targets = sdk_dependency_hygiene_targets();
	let missing = find_target(&targets, "missing_sdk");
	let modern = find_target(&targets, "modern_ok");

	let results = SdkConstraintPresentRule::new().run(&ctx(missing), &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("environment.sdk")
	);
	assert!(
		SdkConstraintPresentRule::new()
			.run(&ctx(modern), &config())
			.is_empty()
	);
}

#[test]
fn sdk_constraint_modern_rule_reports_legacy_and_overly_broad_constraints() {
	let targets = sdk_dependency_hygiene_targets();
	let legacy = find_target(&targets, "legacy_sdk");
	let wide = find_target(&targets, "wide_sdk");

	let legacy_results = SdkConstraintModernRule::new().run(&ctx(legacy), &config());
	assert_eq!(legacy_results.len(), 1);
	assert!(
		legacy_results
			.first()
			.expect("expected lint result")
			.message
			.contains("3.0.0")
	);

	let wide_results = SdkConstraintModernRule::new().run(&ctx(wide), &config());
	assert_eq!(wide_results.len(), 1);
	assert!(
		wide_results
			.first()
			.expect("expected lint result")
			.message
			.contains("upper bound")
	);

	let tuned_config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([
			("minimum".to_string(), json!("2.19.0")),
			("require_upper_bound".to_string(), json!(false)),
		]),
	};
	assert!(
		SdkConstraintModernRule::new()
			.run(&ctx(legacy), &tuned_config)
			.is_empty()
	);
	assert!(
		SdkConstraintModernRule::new()
			.run(&ctx(wide), &tuned_config)
			.is_empty()
	);
}

#[test]
fn dependency_sorted_rule_reports_unsorted_sections_and_emits_parseable_fix() {
	let targets = sdk_dependency_hygiene_targets();
	let target = find_target(&targets, "unsorted_deps");
	let results = DependencySortedRule::new().run(&ctx(target), &config());
	assert_eq!(results.len(), 3);

	let replacement = results
		.first()
		.and_then(|result| result.fix.as_ref())
		.and_then(|fix| fix.edits.first())
		.map(|edit| edit.replacement.clone())
		.expect("expected fix replacement");
	assert!(serde_yaml_ng::from_str::<Mapping>(&replacement).is_ok());
	assert_eq!(
		source_key_order(&replacement, "dependencies"),
		Some(vec!["alpha".to_string(), "zebra".to_string()])
	);
	assert_eq!(
		source_key_order(&replacement, "dev_dependencies"),
		Some(vec!["beta".to_string(), "zeta".to_string()])
	);
	assert_eq!(
		source_key_order(&replacement, "dependency_overrides"),
		Some(vec!["analyzer".to_string(), "yaml".to_string()])
	);
}

#[test]
fn assets_sorted_rule_reports_unsorted_flutter_assets_and_families() {
	let targets = advanced_workspace_flutter_targets();
	let target = find_target(&targets, "flutter_assets_unsorted");
	let results = AssetsSortedRule::new().run(&ctx(target), &config());
	assert_eq!(results.len(), 4);

	let replacement = results
		.first()
		.and_then(|result| result.fix.as_ref())
		.and_then(|fix| fix.edits.first())
		.map(|edit| edit.replacement.clone())
		.expect("expected fix replacement");
	let rewritten = serde_yaml_ng::from_str::<Mapping>(&replacement)
		.unwrap_or_else(|error| panic!("expected yaml replacement: {error}"));
	let flutter = flutter_section(&rewritten).expect("expected flutter section");
	assert_eq!(
		sequence_order(yaml_sequence(flutter, "assets").expect("expected assets")),
		vec![
			"assets/icons/alpha.png".to_string(),
			"assets/icons/zebra.png".to_string(),
		]
	);
}

#[test]
fn flutter_package_metadata_consistent_rule_is_flutter_only() {
	let targets = advanced_workspace_flutter_targets();
	let failing = find_target(&targets, "flutter_missing_sdk");
	let passing = find_target(&targets, "path_ok");

	let results = FlutterPackageMetadataConsistentRule::new().run(&ctx(failing), &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("dependencies.flutter")
	);
	assert!(
		FlutterPackageMetadataConsistentRule::new()
			.run(&ctx(passing), &config())
			.is_empty()
	);
}

#[test]
fn internal_path_dependency_policy_rule_supports_path_and_hosted_modes() {
	let targets = advanced_workspace_flutter_targets();
	let path_ok = find_target(&targets, "path_ok");
	let path_fail = find_target(&targets, "path_fail");

	assert!(
		InternalPathDependencyPolicyRule::new()
			.run(&ctx(path_ok), &config())
			.is_empty()
	);
	let failing = InternalPathDependencyPolicyRule::new().run(&ctx(path_fail), &config());
	assert_eq!(failing.len(), 1);
	assert!(
		failing
			.first()
			.expect("expected lint result")
			.message
			.contains("use `path:` references")
	);

	let hosted_config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("mode".to_string(), json!("hosted"))]),
	};
	assert!(
		InternalPathDependencyPolicyRule::new()
			.run(&ctx(path_fail), &hosted_config)
			.is_empty()
	);
}

#[test]
fn workspace_internal_version_consistency_rule_reports_workspace_drift() {
	let targets = advanced_workspace_flutter_targets();
	let mismatch = find_target(&targets, "version_mismatch");
	let ok = find_target(&targets, "path_fail");

	let results = WorkspaceInternalVersionConsistencyRule::new().run(&ctx(mismatch), &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("workspace version is `1.2.3`")
	);
	assert!(
		WorkspaceInternalVersionConsistencyRule::new()
			.run(&ctx(ok), &config())
			.is_empty()
	);
}

#[test]
fn no_git_dependencies_rule_reports_published_git_dependencies_and_supports_allow_list() {
	let targets = metadata_publishability_targets();
	let failing = find_target(&targets, "git_dep_fail");
	let private = find_target(&targets, "private_git_dep");

	let results = NoGitDependenciesInPublishedPackagesRule::new().run(&ctx(failing), &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("git_dep")
	);

	let allow_config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("allow".to_string(), json!(["git_dep"]))]),
	};
	assert!(
		NoGitDependenciesInPublishedPackagesRule::new()
			.run(&ctx(failing), &allow_config)
			.is_empty()
	);
	assert!(
		NoGitDependenciesInPublishedPackagesRule::new()
			.run(&ctx(private), &config())
			.is_empty()
	);
}

#[test]
fn no_unexpected_dependency_overrides_rule_supports_allow_list_and_private_packages() {
	let targets = sdk_dependency_hygiene_targets();
	let failing = find_target(&targets, "override_fail");
	let private = find_target(&targets, "override_private");
	let allowed = find_target(&targets, "override_allowed");

	let results = NoUnexpectedDependencyOverridesRule::new().run(&ctx(failing), &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.expect("expected lint result")
			.message
			.contains("override_fail")
	);
	assert!(
		NoUnexpectedDependencyOverridesRule::new()
			.run(&ctx(private), &config())
			.is_empty()
	);

	let allow_config = LintRuleConfig::Detailed {
		level: LintSeverity::Error,
		options: BTreeMap::from([("allow_packages".to_string(), json!(["override_allowed"]))]),
	};
	assert!(
		NoUnexpectedDependencyOverridesRule::new()
			.run(&ctx(allowed), &allow_config)
			.is_empty()
	);
}

#[test]
fn unlisted_package_private_rule_reports_for_public_unmanaged_packages() {
	let targets = metadata_publishability_targets();
	let failing = find_target(&targets, "unmanaged_public");
	let passing = find_target(&targets, "unmanaged_private");

	let results = UnlistedPackagePrivateRule::new().run(&ctx(failing), &config());
	assert_eq!(results.len(), 1);
	assert!(
		results
			.first()
			.and_then(|result| result.fix.as_ref())
			.is_some()
	);
	assert!(
		UnlistedPackagePrivateRule::new()
			.run(&ctx(passing), &config())
			.is_empty()
	);
}

#[test]
fn collect_targets_loads_managed_workspace_dart_packages() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dart/workspace");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("load dart workspace config: {error}"));
	let targets = lint_suite()
		.collect_targets(&root, &configuration)
		.unwrap_or_else(|error| panic!("collect dart lint targets: {error}"));

	assert_eq!(targets.len(), 2);
	assert!(
		targets
			.iter()
			.all(|target| target.metadata.ecosystem == "dart")
	);
	assert!(targets.iter().all(|target| target.metadata.managed));
	assert!(
		targets
			.iter()
			.all(|target| target.parsed.downcast_ref::<DartLintFile>().is_some())
	);
	assert!(
		targets
			.iter()
			.any(|target| target.metadata.package_name.as_deref() == Some("dart_app"))
	);
	assert!(
		targets
			.iter()
			.any(|target| target.metadata.package_name.as_deref() == Some("dart_shared"))
	);
}

#[test]
fn collect_targets_mark_workspace_versions_for_internal_rules() {
	let targets = advanced_workspace_flutter_targets();
	let target = find_target(&targets, "path_fail");
	let file = target
		.parsed
		.downcast_ref::<DartLintFile>()
		.expect("expected dart lint file");
	assert_eq!(
		file.workspace_package_versions
			.get("core")
			.map(ToString::to_string)
			.as_deref(),
		Some("1.2.3")
	);
}

#[test]
fn collect_targets_marks_private_packages_from_publish_to_none() {
	let targets = metadata_publishability_targets();
	let private = find_target(&targets, "private_git_dep");
	assert_eq!(private.metadata.private, Some(true));
	assert_eq!(private.metadata.publishable, Some(false));
}

#[test]
fn collect_targets_ignores_fixture_manifests_outside_workspace_packages() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("load workspace config: {error}"));
	let targets = lint_suite()
		.collect_targets(&root, &configuration)
		.unwrap_or_else(|error| panic!("collect repo dart lint targets: {error}"));

	assert!(
		targets
			.iter()
			.all(|target| !target.manifest_path.starts_with(root.join("fixtures")))
	);
}
