#![forbid(clippy::indexing_slicing)]

//! Dart and Flutter manifest lint suite.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use monochange_core::MonochangeResult;
use monochange_core::WorkspaceConfiguration;
use monochange_core::lint::LintCategory;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintMaturity;
use monochange_core::lint::LintOptionDefinition;
use monochange_core::lint::LintOptionKind;
use monochange_core::lint::LintPreset;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintSeverity;
use monochange_core::lint::LintSuite;
use monochange_core::lint::LintTarget;
use monochange_core::lint::LintTargetMetadata;
use monochange_core::relative_to_root;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;

use crate::discover_dart_packages;
use crate::manifest_publish_state;

/// Return the shared Dart lint suite.
#[must_use]
pub fn lint_suite() -> DartLintSuite {
	DartLintSuite
}

/// Dart lint suite implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct DartLintSuite;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub(crate) struct DartLintFile {
	pub manifest: Mapping,
	pub workspace_package_names: Arc<BTreeSet<String>>,
}

impl LintSuite for DartLintSuite {
	fn suite_id(&self) -> &'static str {
		"dart"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(NoGitDependenciesInPublishedPackagesRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"dart/recommended",
				"Dart recommended",
				"Balanced Dart manifest linting for published package metadata and publishability",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([
				(
					"dart/no-git-dependencies-in-published-packages".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
			])),
			LintPreset::new(
				"dart/strict",
				"Dart strict",
				"Opinionated Dart manifest linting with the same publishability rules enabled by default",
				LintMaturity::Strict,
			)
			.with_rules(BTreeMap::from([
				(
					"dart/no-git-dependencies-in-published-packages".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
			])),
		]
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		let discovery = discover_dart_packages(workspace_root)?;
		let workspace_package_names = Arc::new(
			discovery
				.packages
				.iter()
				.map(|package| package.name.clone())
				.collect::<BTreeSet<_>>(),
		);

		discovery
			.packages
			.into_iter()
			.filter(|package| {
				is_lintable_workspace_manifest(workspace_root, &package.manifest_path)
			})
			.map(|package| {
				let contents = fs::read_to_string(&package.manifest_path).map_err(|error| {
					monochange_core::MonochangeError::IoSource {
						path: package.manifest_path.clone(),
						source: error,
					}
				})?;
				let manifest = serde_yaml_ng::from_str::<Mapping>(&contents).map_err(|error| {
					monochange_core::MonochangeError::Parse {
						path: package.manifest_path.clone(),
						source: Box::new(error),
					}
				})?;
				let manifest_dir = package.manifest_path.parent().unwrap_or(workspace_root);
				let configured_package =
					configured_package(configuration, workspace_root, manifest_dir);
				let package_id = configured_package.map(ToString::to_string);
				let group_id = configured_package.and_then(|package_id| {
					configuration
						.group_for_package(package_id)
						.map(|group| group.id.clone())
				});
				let relative_path = relative_to_root(workspace_root, &package.manifest_path)
					.unwrap_or_else(|| package.manifest_path.clone());
				let private = matches!(
					manifest_publish_state(&manifest),
					monochange_core::PublishState::Private
				);

				Ok(LintTarget::new(
					workspace_root.to_path_buf(),
					package.manifest_path.clone(),
					contents,
					LintTargetMetadata {
						ecosystem: "dart".to_string(),
						relative_path,
						package_name: Some(package.name),
						package_id,
						group_id,
						managed: configured_package.is_some(),
						private: Some(private),
						publishable: Some(!private),
					},
					Box::new(DartLintFile {
						manifest,
						workspace_package_names: Arc::clone(&workspace_package_names),
					}),
				))
			})
			.collect()
	}
}

fn is_lintable_workspace_manifest(workspace_root: &Path, manifest_path: &Path) -> bool {
	!(manifest_path.starts_with(workspace_root.join("fixtures"))
		|| manifest_path.starts_with(workspace_root.join("target"))
		|| manifest_path.starts_with(workspace_root.join(".git")))
}

fn configured_package<'a>(
	configuration: &'a WorkspaceConfiguration,
	workspace_root: &Path,
	manifest_dir: &Path,
) -> Option<&'a str> {
	let relative_dir = relative_to_root(workspace_root, manifest_dir)?;
	configuration
		.packages
		.iter()
		.find_map(|package| (package.path == relative_dir).then_some(package.id.as_str()))
}

fn dart_file<'a>(ctx: &'a LintContext<'a>) -> Option<&'a DartLintFile> {
	ctx.parsed_as::<DartLintFile>()
}

fn location(ctx: &LintContext<'_>) -> LintLocation {
	LintLocation::new(ctx.manifest_path, 1, 1)
}

fn yaml_key(key: &str) -> Value {
	Value::String(key.to_string())
}

fn yaml_mapping<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Mapping> {
	mapping.get(yaml_key(key)).and_then(Value::as_mapping)
}

fn manifest_has_key(mapping: &Mapping, key: &str) -> bool {
	mapping.contains_key(yaml_key(key))
}

fn manifest_declares_private(mapping: &Mapping) -> bool {
	matches!(
		manifest_publish_state(mapping),
		monochange_core::PublishState::Private
	)
}

fn insert_publish_to_none(contents: &str) -> String {
	if contents.is_empty() {
		return "publish_to: none\n".to_string();
	}

	let separator = if contents.ends_with('\n') { "" } else { "\n" };
	format!("{contents}{separator}publish_to: none\n")
}

#[derive(Debug)]
struct NoGitDependenciesInPublishedPackagesRule {
	rule: LintRule,
}

impl NoGitDependenciesInPublishedPackagesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/no-git-dependencies-in-published-packages",
				"No git dependencies in published packages",
				"Prevents published Dart packages from using git: dependencies unless explicitly allowed",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![LintOptionDefinition::new(
				"allow",
				"list of dependency names that may use git: sources",
				LintOptionKind::StringList,
			)]),
		}
	}
}

impl LintRuleRunner for NoGitDependenciesInPublishedPackagesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		if manifest_declares_private(&file.manifest) {
			return Vec::new();
		}

		let allowed = config
			.string_list_option("allow")
			.unwrap_or_default()
			.into_iter()
			.collect::<BTreeSet<_>>();
		let mut results = Vec::new();

		for section in ["dependencies", "dev_dependencies"] {
			let Some(dependencies) = yaml_mapping(&file.manifest, section) else {
				continue;
			};

			for (dependency_name, value) in dependencies {
				let Some(dependency_name) = dependency_name.as_str() else {
					continue;
				};
				if allowed.contains(dependency_name) {
					continue;
				}
				let uses_git = matches!(
					value,
					Value::Mapping(mapping) if mapping.contains_key(yaml_key("git"))
				);
				if !uses_git {
					continue;
				}

				results.push(LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!(
						"published Dart packages must not use `git:` for dependency `{dependency_name}` in `{section}`"
					),
					config.severity(),
				));
			}
		}

		results
	}
}

#[derive(Debug)]
struct RequiredPackageFieldsRule {
	rule: LintRule,
}

impl RequiredPackageFieldsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/required-package-fields",
				"Required package fields",
				"Requires selected pubspec.yaml fields for managed publishable Dart packages",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fields",
				"list of pubspec.yaml fields that must be present",
				LintOptionKind::StringList,
			)]),
		}
	}
}

impl LintRuleRunner for RequiredPackageFieldsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		if !ctx.metadata.managed || manifest_declares_private(&file.manifest) {
			return Vec::new();
		}

		config
			.string_list_option("fields")
			.unwrap_or_else(|| vec!["description".to_string(), "repository".to_string()])
			.into_iter()
			.filter(|field| !manifest_has_key(&file.manifest, field))
			.map(|field| {
				LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!("missing required pubspec.yaml field `{field}`"),
					config.severity(),
				)
			})
			.collect()
	}
}

#[derive(Debug)]
struct UnlistedPackagePrivateRule {
	rule: LintRule,
}

impl UnlistedPackagePrivateRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/unlisted-package-private",
				"Unlisted package must be private",
				"Requires unmanaged Dart packages to declare publish_to: none",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that inserts publish_to: none",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for UnlistedPackagePrivateRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		if ctx.metadata.managed || manifest_declares_private(&file.manifest) {
			return Vec::new();
		}

		let mut result = LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			"unmanaged Dart packages must set publish_to: none or be declared in monochange.toml",
			config.severity(),
		);
		if config.bool_option("fix", true) {
			result = result.with_fix(LintFix::single(
				"insert publish_to: none",
				(0, ctx.contents.len()),
				insert_publish_to_none(ctx.contents),
			));
		}

		vec![result]
	}
}

#[cfg(test)]
mod tests {
	use std::path::Path;

	use monochange_config::load_workspace_configuration;
	use monochange_core::lint::LintSuite;
	use monochange_core::lint::LintTarget;
	use monochange_test_helpers::fixture_path;
	use serde_json::json;

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
		assert_eq!(
			presets.first().map(|preset| preset.id.as_str()),
			Some("dart/recommended")
		);
		assert_eq!(
			presets.get(1).map(|preset| preset.id.as_str()),
			Some("dart/strict")
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
}
