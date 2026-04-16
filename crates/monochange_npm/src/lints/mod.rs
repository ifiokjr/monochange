#![forbid(clippy::indexing_slicing)]

//! npm-family manifest lint suite.

use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use monochange_core::MonochangeResult;
use monochange_core::PublishState;
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
use serde_json::Map;
use serde_json::Value;

use crate::discover_npm_packages;

/// Return the shared npm-family lint suite.
#[must_use]
pub fn lint_suite() -> NpmLintSuite {
	NpmLintSuite
}

/// npm-family lint suite implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct NpmLintSuite;

#[derive(Debug, Clone)]
struct NpmLintFile {
	manifest: Value,
	workspace_package_names: Arc<BTreeSet<String>>,
}

impl LintSuite for NpmLintSuite {
	fn suite_id(&self) -> &'static str {
		"npm"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(WorkspaceProtocolRule::new()),
			Box::new(SortedDependenciesRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(RootNoProdDepsRule::new()),
			Box::new(NoDuplicateDependenciesRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"npm/recommended",
				"npm recommended",
				"Balanced npm-family manifest linting for typical JavaScript workspaces",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([
				(
					"npm/workspace-protocol".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/sorted-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
				(
					"npm/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/root-no-prod-deps".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/no-duplicate-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
			])),
			LintPreset::new(
				"npm/strict",
				"npm strict",
				"Opinionated npm-family manifest linting with style rules promoted to errors",
				LintMaturity::Strict,
			)
			.with_rules(BTreeMap::from([
				(
					"npm/workspace-protocol".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/sorted-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/root-no-prod-deps".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/no-duplicate-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"npm/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
			])),
		]
	}

	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>> {
		let discovery = discover_npm_packages(workspace_root)?;
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
				let manifest = serde_json::from_str::<Value>(&contents).map_err(|error| {
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
				let private = matches!(package.publish_state, PublishState::Private);

				Ok(LintTarget::new(
					workspace_root.to_path_buf(),
					package.manifest_path.clone(),
					contents,
					LintTargetMetadata {
						ecosystem: "npm".to_string(),
						relative_path,
						package_name: Some(package.name),
						package_id,
						group_id,
						managed: configured_package.is_some(),
						private: Some(private),
						publishable: Some(!private),
					},
					Box::new(NpmLintFile {
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

fn npm_file<'a>(ctx: &'a LintContext<'a>) -> Option<&'a NpmLintFile> {
	ctx.parsed_as::<NpmLintFile>()
}

fn dependency_sections() -> [&'static str; 4] {
	[
		"dependencies",
		"devDependencies",
		"peerDependencies",
		"optionalDependencies",
	]
}

fn location(ctx: &LintContext<'_>) -> LintLocation {
	LintLocation::new(ctx.manifest_path, 1, 1)
}

fn manifest_object_mut(value: &mut Value) -> Option<&mut Map<String, Value>> {
	value.as_object_mut()
}

fn source_key_order(contents: &str, section: &str, keys: &[&String]) -> Option<Vec<String>> {
	let section_anchor = format!("\"{section}\"");
	let section_start = contents.find(&section_anchor)?;
	let rest = &contents[section_start..];
	let open_offset = rest.find('{')? + section_start;
	let close_offset = matching_brace_offset(contents, open_offset)?;
	let section_text = &contents[open_offset..=close_offset];
	let mut keyed_positions = keys
		.iter()
		.filter_map(|key| {
			section_text
				.find(&format!("\"{key}\""))
				.map(|position| ((*key).clone(), position))
		})
		.collect::<Vec<_>>();
	keyed_positions.sort_by_key(|(_, position)| *position);
	Some(
		keyed_positions
			.into_iter()
			.map(|(key, _)| key)
			.collect::<Vec<_>>(),
	)
}

fn matching_brace_offset(contents: &str, open_offset: usize) -> Option<usize> {
	let mut depth = 0usize;
	for (offset, ch) in contents[open_offset..].char_indices() {
		match ch {
			'{' => depth += 1,
			'}' => {
				depth -= 1;
				if depth == 0 {
					return Some(open_offset + offset);
				}
			}
			_ => {}
		}
	}
	None
}

#[derive(Debug)]
struct WorkspaceProtocolRule {
	rule: LintRule,
}

impl WorkspaceProtocolRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/workspace-protocol",
				"Workspace protocol",
				"Requires internal npm-family dependencies to use the workspace: protocol",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![
				LintOptionDefinition::new(
					"require_for_private",
					"also enforce the rule for private packages",
					LintOptionKind::Boolean,
				),
				LintOptionDefinition::new(
					"fix",
					"apply an autofix that rewrites the dependency value",
					LintOptionKind::Boolean,
				),
			]),
		}
	}
}

impl LintRuleRunner for WorkspaceProtocolRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if ctx.metadata.private == Some(true) && !config.bool_option("require_for_private", false) {
			return Vec::new();
		}
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in dependency_sections() {
			let Some(object) = file.manifest.get(section).and_then(Value::as_object) else {
				continue;
			};
			for (dep_name, version) in object {
				let Some(version) = version.as_str() else {
					continue;
				};
				if !file.workspace_package_names.contains(dep_name)
					|| version.starts_with("workspace:")
				{
					continue;
				}

				let mut result = LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!(
						"internal dependency `{dep_name}` should use the workspace: protocol (found `{version}`)"
					),
					config.severity(),
				);

				if config.bool_option("fix", true) {
					let mut rewritten = file.manifest.clone();
					if let Some(root) = manifest_object_mut(&mut rewritten)
						&& let Some(section) = root.get_mut(section).and_then(Value::as_object_mut)
					{
						section.insert(dep_name.clone(), Value::String("workspace:*".to_string()));
					}
					result = result.with_fix(LintFix::single(
						"rewrite dependency to workspace:*",
						(0, ctx.contents.len()),
						serde_json::to_string_pretty(&rewritten)
							.unwrap_or_else(|_| ctx.contents.to_string()),
					));
				}

				results.push(result);
			}
		}

		results
	}
}

#[derive(Debug)]
struct SortedDependenciesRule {
	rule: LintRule,
}

impl SortedDependenciesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/sorted-dependencies",
				"Sorted dependencies",
				"Requires npm-family dependency sections to be alphabetically sorted",
				LintCategory::Style,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that rewrites the manifest with sorted dependency sections",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for SortedDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in dependency_sections() {
			let Some(object) = file.manifest.get(section).and_then(Value::as_object) else {
				continue;
			};
			let keys = object.keys().collect::<Vec<_>>();
			let source_order = source_key_order(ctx.contents, section, &keys)
				.unwrap_or_else(|| keys.iter().map(|key| (*key).clone()).collect::<Vec<_>>());
			let mut sorted_keys = keys.iter().map(|key| (*key).clone()).collect::<Vec<_>>();
			sorted_keys.sort();
			if source_order == sorted_keys {
				continue;
			}

			let mut result = LintResult::new(
				self.rule.id.clone(),
				location(ctx),
				format!("dependencies in `{section}` are not sorted alphabetically"),
				config.severity(),
			);
			if config.bool_option("fix", true) {
				let mut rewritten = file.manifest.clone();
				if let Some(root) = manifest_object_mut(&mut rewritten)
					&& let Some(section_obj) = root.get_mut(section).and_then(Value::as_object_mut)
				{
					let current = section_obj.clone();
					section_obj.clear();
					for key in sorted_keys {
						if let Some(value) = current.get(&key) {
							section_obj.insert(key.clone(), value.clone());
						}
					}
				}
				result = result.with_fix(LintFix::single(
					"sort dependency section alphabetically",
					(0, ctx.contents.len()),
					serde_json::to_string_pretty(&rewritten)
						.unwrap_or_else(|_| ctx.contents.to_string()),
				));
			}
			results.push(result);
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
				"npm/required-package-fields",
				"Required package fields",
				"Requires selected package.json fields to be present",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fields",
				"list of package.json fields that must be present",
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
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		config
			.string_list_option("fields")
			.unwrap_or_else(|| {
				vec![
					"description".to_string(),
					"repository".to_string(),
					"license".to_string(),
				]
			})
			.into_iter()
			.filter(|field| file.manifest.get(field).is_none())
			.map(|field| {
				LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!("missing required package.json field `{field}`"),
					config.severity(),
				)
			})
			.collect()
	}
}

#[derive(Debug)]
struct RootNoProdDepsRule {
	rule: LintRule,
}

impl RootNoProdDepsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/root-no-prod-deps",
				"Root no production dependencies",
				"Requires the root package.json to keep production dependencies out of dependencies",
				LintCategory::BestPractice,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that moves dependencies into devDependencies",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for RootNoProdDepsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if ctx.manifest_path.parent() != Some(ctx.workspace_root) {
			return Vec::new();
		}
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let Some(deps) = file.manifest.get("dependencies").and_then(Value::as_object) else {
			return Vec::new();
		};
		if deps.is_empty() {
			return Vec::new();
		}

		let mut result = LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			"root package.json should not have production dependencies; move them to devDependencies",
			config.severity(),
		);
		if config.bool_option("fix", true) {
			let mut rewritten = file.manifest.clone();
			if let Some(root) = manifest_object_mut(&mut rewritten) {
				let moved = root
					.remove("dependencies")
					.and_then(|value| value.as_object().cloned())
					.unwrap_or_default();
				let dev_dependencies = root
					.entry("devDependencies".to_string())
					.or_insert_with(|| Value::Object(Map::new()));
				if let Some(dev_dependencies) = dev_dependencies.as_object_mut() {
					for (name, value) in moved {
						dev_dependencies.insert(name, value);
					}
				}
			}
			result = result.with_fix(LintFix::single(
				"move root dependencies to devDependencies",
				(0, ctx.contents.len()),
				serde_json::to_string_pretty(&rewritten)
					.unwrap_or_else(|_| ctx.contents.to_string()),
			));
		}
		vec![result]
	}
}

#[derive(Debug)]
struct NoDuplicateDependenciesRule {
	rule: LintRule,
}

impl NoDuplicateDependenciesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"npm/no-duplicate-dependencies",
				"No duplicate dependencies",
				"Prevents one dependency from appearing in multiple npm-family dependency sections",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that removes duplicate entries from later sections",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for NoDuplicateDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut seen = BTreeMap::<String, Vec<&'static str>>::new();
		for section in dependency_sections() {
			let Some(object) = file.manifest.get(section).and_then(Value::as_object) else {
				continue;
			};
			for dep_name in object.keys() {
				seen.entry(dep_name.clone()).or_default().push(section);
			}
		}

		let mut results = Vec::new();
		for (dep_name, sections) in seen {
			if sections.len() <= 1 {
				continue;
			}
			let mut result = LintResult::new(
				self.rule.id.clone(),
				location(ctx),
				format!(
					"dependency `{dep_name}` appears in multiple sections: {}",
					sections.join(", ")
				),
				config.severity(),
			);
			if config.bool_option("fix", true) {
				let mut rewritten = file.manifest.clone();
				if let Some(root) = manifest_object_mut(&mut rewritten) {
					let keep_in = if sections.contains(&"devDependencies") {
						"devDependencies"
					} else {
						sections.first().copied().unwrap_or("dependencies")
					};
					for section in &sections {
						if *section == keep_in {
							continue;
						}
						if let Some(section_obj) =
							root.get_mut(*section).and_then(Value::as_object_mut)
						{
							section_obj.remove(&dep_name);
						}
					}
				}
				result = result.with_fix(LintFix::single(
					"remove duplicate dependency entries from later sections",
					(0, ctx.contents.len()),
					serde_json::to_string_pretty(&rewritten)
						.unwrap_or_else(|_| ctx.contents.to_string()),
				));
			}
			results.push(result);
		}
		results
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
				"npm/unlisted-package-private",
				"Unlisted package must be private",
				"Requires unmanaged npm-family packages to declare private: true",
				LintCategory::Correctness,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that inserts private: true",
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
		if ctx.metadata.managed || ctx.metadata.private == Some(true) {
			return Vec::new();
		}
		let Some(file) = npm_file(ctx) else {
			return Vec::new();
		};
		let mut result = LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			"unmanaged npm-family packages must set private: true or be declared in monochange.toml",
			config.severity(),
		);
		if config.bool_option("fix", true) {
			let mut rewritten = file.manifest.clone();
			if let Some(root) = manifest_object_mut(&mut rewritten) {
				root.insert("private".to_string(), Value::Bool(true));
			}
			result = result.with_fix(LintFix::single(
				"insert private: true",
				(0, ctx.contents.len()),
				serde_json::to_string_pretty(&rewritten)
					.unwrap_or_else(|_| ctx.contents.to_string()),
			));
		}
		vec![result]
	}
}

#[cfg(test)]
mod tests {
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
}
