#![forbid(clippy::indexing_slicing)]

//! Cargo manifest lint suite.

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
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::value as toml_value;

use crate::discover_cargo_packages;

/// Return the shared Cargo lint suite.
#[must_use]
pub fn lint_suite() -> CargoLintSuite {
	CargoLintSuite
}

/// Cargo lint suite implementation.
#[derive(Debug, Clone, Copy, Default)]
pub struct CargoLintSuite;

#[derive(Debug, Clone)]
struct CargoLintFile {
	document: DocumentMut,
	workspace_package_names: Arc<BTreeSet<String>>,
}

impl LintSuite for CargoLintSuite {
	fn suite_id(&self) -> &'static str {
		"cargo"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(DependencyFieldOrderRule::new()),
			Box::new(InternalDependencyWorkspaceRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(SortedDependenciesRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"cargo/recommended",
				"Cargo recommended",
				"Balanced Cargo manifest linting for most workspaces",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([
				(
					"cargo/dependency-field-order".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
				(
					"cargo/internal-dependency-workspace".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"cargo/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"cargo/sorted-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
				(
					"cargo/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
			])),
			LintPreset::new(
				"cargo/strict",
				"Cargo strict",
				"Opinionated Cargo manifest linting with style rules promoted to errors",
				LintMaturity::Strict,
			)
			.with_rules(BTreeMap::from([
				(
					"cargo/dependency-field-order".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"cargo/internal-dependency-workspace".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"cargo/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"cargo/sorted-dependencies".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"cargo/unlisted-package-private".to_string(),
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
		let discovery = discover_cargo_packages(workspace_root)?;
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
				let document = contents.parse::<DocumentMut>().map_err(|error| {
					monochange_core::MonochangeError::Parse {
						path: package.manifest_path.clone(),
						source: Box::new(error),
					}
				})?;

				let manifest_dir = package.manifest_path.parent().unwrap_or(workspace_root);
				let configured_package =
					configured_package(configuration, workspace_root, manifest_dir);
				let package_id = configured_package.map(|(package_id, _)| package_id.to_string());
				let group_id = configured_package.and_then(|(package_id, _)| {
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
						ecosystem: "cargo".to_string(),
						relative_path,
						package_name: Some(package.name),
						package_id,
						group_id,
						managed: configured_package.is_some(),
						private: Some(private),
						publishable: Some(!private),
					},
					Box::new(CargoLintFile {
						document,
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
) -> Option<(&'a str, &'a Path)> {
	let relative_dir = relative_to_root(workspace_root, manifest_dir)?;
	configuration.packages.iter().find_map(|package| {
		(package.path == relative_dir).then_some((package.id.as_str(), package.path.as_path()))
	})
}

fn cargo_file<'a>(ctx: &'a LintContext<'a>) -> Option<&'a CargoLintFile> {
	ctx.parsed_as::<CargoLintFile>()
}

fn location_from_span(
	manifest_path: &Path,
	contents: &str,
	span: Option<(usize, usize)>,
) -> LintLocation {
	let Some((start, end)) = span else {
		return LintLocation::new(manifest_path, 1, 1);
	};
	let prefix = &contents[..start.min(contents.len())];
	let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
	let column = prefix
		.rsplit('\n')
		.next()
		.map_or(1, |segment| segment.chars().count() + 1);
	LintLocation::new(manifest_path, line, column).with_span(start, end)
}

fn section_item<'a>(document: &'a DocumentMut, section: &str) -> Option<&'a Item> {
	document.get(section)
}

fn value_has_workspace_enabled(value: &Item) -> bool {
	match value {
		Item::Table(table) => {
			table
				.get("workspace")
				.is_some_and(|workspace| workspace.as_bool().is_some_and(|enabled| enabled))
		}
		Item::Value(raw) => {
			raw.as_inline_table()
				.and_then(|table| table.get("workspace"))
				.is_some_and(|workspace| workspace.as_bool().is_some_and(|enabled| enabled))
		}
		_ => false,
	}
}

fn preferred_dependency_order() -> [&'static str; 14] {
	[
		"workspace",
		"version",
		"default-features",
		"default_features",
		"features",
		"optional",
		"path",
		"registry",
		"registry-index",
		"package",
		"git",
		"branch",
		"tag",
		"rev",
	]
}

monochange_linting::declare_lint_rule! {
	DependencyFieldOrderRule,
	id: "cargo/dependency-field-order",
	name: "Dependency field order",
	description: "Enforces consistent ordering inside Cargo dependency tables",
	category: LintCategory::Style,
	maturity: LintMaturity::Stable,
	autofixable: true,
	options: vec![LintOptionDefinition::new(
		"fix",
		"apply an autofix that rewrites the dependency entry",
		LintOptionKind::Boolean,
	)],
}

impl DependencyFieldOrderRule {
	fn ordered_fields(table: &toml_edit::Table) -> Vec<&str> {
		let order = preferred_dependency_order();
		let mut keys = table
			.iter()
			.filter_map(|(key, _)| {
				order
					.iter()
					.position(|preferred| preferred == &key)
					.map(|pos| (key, pos))
			})
			.collect::<Vec<_>>();
		keys.sort_by_key(|(_, pos)| *pos);
		keys.into_iter().map(|(key, _)| key).collect()
	}
}

impl LintRuleRunner for DependencyFieldOrderRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = cargo_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
			let Some(item) = section_item(&file.document, section) else {
				continue;
			};
			let Some(table) = item.as_table() else {
				continue;
			};

			for (dep_name, value) in table {
				let Some(dep_table) = value.as_table() else {
					continue;
				};
				let actual = dep_table.iter().map(|(key, _)| key).collect::<Vec<_>>();
				let expected = Self::ordered_fields(dep_table);
				if actual == expected {
					continue;
				}

				let span = value.span().map(|span| (span.start, span.end));
				let location = location_from_span(ctx.manifest_path, ctx.contents, span);
				let mut result = LintResult::new(
					self.rule.id.clone(),
					location,
					format!(
						"dependency `{dep_name}` has fields in the wrong order; put workspace/version first, then default-features, then features"
					),
					config.severity(),
				);

				if config.bool_option("fix", true) {
					let mut reordered = toml_edit::Table::new();
					for key in expected {
						if let Some(value) = dep_table.get(key) {
							reordered.insert(key, value.clone());
						}
					}
					for (key, value) in dep_table {
						if !reordered.contains_key(key) {
							reordered.insert(key, value.clone());
						}
					}
					let replacement = format!("{dep_name} = {}", reordered.to_string().trim());
					result = result.with_fix(LintFix::single(
						"reorder dependency fields",
						span.unwrap_or((0, ctx.contents.len())),
						replacement,
					));
				}

				results.push(result);
			}
		}

		results
	}
}

monochange_linting::declare_lint_rule! {
	InternalDependencyWorkspaceRule,
	id: "cargo/internal-dependency-workspace",
	name: "Internal dependency workspace",
	description: "Requires workspace = true for internal Cargo dependencies",
	category: LintCategory::Correctness,
	maturity: LintMaturity::Stable,
	autofixable: true,
	options: vec![
		LintOptionDefinition::new(
			"require_workspace",
			"require internal dependencies to use workspace = true",
			LintOptionKind::Boolean,
		),
		LintOptionDefinition::new(
			"fix",
			"apply an autofix when a dependency can be rewritten safely",
			LintOptionKind::Boolean,
		),
	],
}

impl LintRuleRunner for InternalDependencyWorkspaceRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.bool_option("require_workspace", true) {
			return Vec::new();
		}

		let Some(file) = cargo_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
			let Some(item) = section_item(&file.document, section) else {
				continue;
			};
			let Some(table) = item.as_table() else {
				continue;
			};

			for (dep_name, value) in table {
				if !file.workspace_package_names.contains(dep_name) {
					continue;
				}

				let has_workspace = value_has_workspace_enabled(value);
				if has_workspace {
					continue;
				}

				let span = value.span().map(|span| (span.start, span.end));
				let location = location_from_span(ctx.manifest_path, ctx.contents, span);
				let mut result = LintResult::new(
					self.rule.id.clone(),
					location,
					format!("internal dependency `{dep_name}` should use workspace = true"),
					config.severity(),
				);

				if config.bool_option("fix", true) {
					let replacement = format!("{dep_name} = {{ workspace = true }}");
					result = result.with_fix(LintFix::single(
						"rewrite internal dependency to workspace = true",
						span.unwrap_or((0, ctx.contents.len())),
						replacement,
					));
				}

				results.push(result);
			}
		}

		results
	}
}

monochange_linting::declare_lint_rule! {
	RequiredPackageFieldsRule,
	id: "cargo/required-package-fields",
	name: "Required package fields",
	description: "Requires selected fields in the [package] table",
	category: LintCategory::Correctness,
	maturity: LintMaturity::Stable,
	autofixable: false,
	options: vec![LintOptionDefinition::new(
		"fields",
		"list of package fields that must be present",
		LintOptionKind::StringList,
	)],
}

impl LintRuleRunner for RequiredPackageFieldsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = cargo_file(ctx) else {
			return Vec::new();
		};
		let Some(package) = file.document.get("package") else {
			return Vec::new();
		};
		let Some(table) = package.as_table() else {
			return Vec::new();
		};

		config
			.string_list_option("fields")
			.unwrap_or_else(|| {
				vec![
					"description".to_string(),
					"license".to_string(),
					"repository".to_string(),
				]
			})
			.into_iter()
			.filter(|field| !table.contains_key(field))
			.map(|field| {
				LintResult::new(
					self.rule.id.clone(),
					LintLocation::new(ctx.manifest_path, 1, 1),
					format!("missing required package field `{field}`"),
					config.severity(),
				)
			})
			.collect()
	}
}

monochange_linting::declare_lint_rule! {
	SortedDependenciesRule,
	id: "cargo/sorted-dependencies",
	name: "Sorted dependencies",
	description: "Requires Cargo dependency tables to be alphabetically sorted",
	category: LintCategory::Style,
	maturity: LintMaturity::Stable,
	autofixable: true,
	options: vec![LintOptionDefinition::new(
		"fix",
		"apply an autofix that rewrites the dependency section",
		LintOptionKind::Boolean,
	)],
}

impl LintRuleRunner for SortedDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = cargo_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
			let Some(item) = section_item(&file.document, section) else {
				continue;
			};
			let Some(table) = item.as_table() else {
				continue;
			};

			let keys = table.iter().map(|(key, _)| key).collect::<Vec<_>>();
			let mut sorted_keys = keys.clone();
			sorted_keys.sort_unstable();
			if keys == sorted_keys {
				continue;
			}

			let span = item.span().map(|span| (span.start, span.end));
			let location = location_from_span(ctx.manifest_path, ctx.contents, span);
			let mut result = LintResult::new(
				self.rule.id.clone(),
				location,
				format!("dependencies in `{section}` are not sorted alphabetically"),
				config.severity(),
			);

			if config.bool_option("fix", true) {
				let mut rewritten = toml_edit::Table::new();
				for key in &sorted_keys {
					if let Some(value) = table.get(key) {
						rewritten.insert(key, value.clone());
					}
				}
				let replacement = format!("[{section}]\n{}", rewritten.to_string().trim());
				result = result.with_fix(LintFix::single(
					"sort dependency section alphabetically",
					span.unwrap_or((0, ctx.contents.len())),
					replacement,
				));
			}

			results.push(result);
		}

		results
	}
}

monochange_linting::declare_lint_rule! {
	UnlistedPackagePrivateRule,
	id: "cargo/unlisted-package-private",
	name: "Unlisted package must be private",
	description: "Requires unmanaged Cargo packages to declare publish = false",
	category: LintCategory::Correctness,
	maturity: LintMaturity::Stable,
	autofixable: true,
	options: vec![LintOptionDefinition::new(
		"fix",
		"apply an autofix that inserts publish = false",
		LintOptionKind::Boolean,
	)],
}

impl LintRuleRunner for UnlistedPackagePrivateRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if ctx.metadata.managed || ctx.metadata.publishable != Some(true) {
			return Vec::new();
		}
		let Some(file) = cargo_file(ctx) else {
			return Vec::new();
		};
		let span = Some((0, ctx.contents.len()));
		let location = location_from_span(ctx.manifest_path, ctx.contents, span);
		let mut result = LintResult::new(
			self.rule.id.clone(),
			location,
			"unmanaged Cargo packages must set publish = false or be declared in monochange.toml",
			config.severity(),
		);

		if config.bool_option("fix", true) {
			let mut rewritten = file.document.clone();
			if let Some(package) = rewritten.get_mut("package").and_then(Item::as_table_mut) {
				package.insert("publish", toml_value(false));
			}
			result = result.with_fix(LintFix::single(
				"insert publish = false",
				(0, ctx.contents.len()),
				rewritten.to_string(),
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
}
