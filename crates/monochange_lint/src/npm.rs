#![forbid(clippy::indexing_slicing)]

//! NPM-specific lint rules.

use std::collections::BTreeMap;

use monochange_core::lint::LintCategory;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use serde_json::Value;

/// NPM lint rules.
pub struct NpmLintRules;

impl NpmLintRules {
	/// Return the default set of NPM lint rules.
	#[must_use]
	pub fn default_rules() -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(WorkspaceProtocolRule::new()),
			Box::new(SortedDependenciesRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(RootNoProdDepsRule::new()),
			Box::new(NoDuplicateDependenciesRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
		]
	}
}

/// Rule: workspace-protocol
///
/// Requires `workspace:` protocol for internal dependencies.
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
				"Requires workspace: protocol for internal dependencies",
				LintCategory::Correctness,
				true,
			),
		}
	}

	fn is_internal_dependency(dep_name: &str, workspace_root: &std::path::Path) -> bool {
		// Check if this is a workspace package
		let potential_path = workspace_root.join("packages").join(dep_name);
		if potential_path.exists() {
			return true;
		}

		// Also check common patterns
		let scoped_path = workspace_root
			.join("packages")
			.join(dep_name.replacen('@', "", 1).replace('/', "-"));
		scoped_path.exists()
	}
}

impl LintRuleRunner for WorkspaceProtocolRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "package.json")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(parsed) = serde_json::from_str::<Value>(ctx.contents) else {
			return Vec::new();
		};

		let require_for_private = config.bool_option("require_for_private", false);

		let mut results = Vec::new();

		// Check if package is private
		let is_private = parsed
			.get("private")
			.and_then(Value::as_bool)
			.unwrap_or(false);
		if is_private && !require_for_private {
			return results;
		}

		// Check dependency sections
		for section in [
			"dependencies",
			"devDependencies",
			"peerDependencies",
			"optionalDependencies",
		] {
			let Some(deps) = parsed.get(section) else {
				continue;
			};

			let Some(obj) = deps.as_object() else {
				continue;
			};

			for (dep_name, version) in obj {
				let Some(version_str) = version.as_str() else {
					continue;
				};

				// Check if this is an internal dependency
				if Self::is_internal_dependency(dep_name, ctx.workspace_root) {
					// Check if it uses workspace: protocol
					if !version_str.starts_with("workspace:") {
						let location = LintLocation::new(ctx.manifest_path, 1, 1);
						let message = format!(
							"Internal dependency '{dep_name}' should use 'workspace:' protocol (got: '{version_str}')"
						);

						let mut result = LintResult::new(
							"npm/workspace-protocol",
							location,
							message,
							config.severity(),
						);

						if config.bool_option("fix", true) {
							let replacement = format!("\"{dep_name}\": \"workspace:*\"");
							result = result.with_fix(LintFix::single(
								"Use workspace protocol",
								(0, 0), // Would need actual position
								replacement,
							));
						}

						results.push(result);
					}
				}
			}
		}

		results
	}
}

/// Rule: sorted-dependencies
///
/// Requires alphabetically sorted dependencies in package.json.
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
				"Requires alphabetically sorted dependencies",
				LintCategory::Style,
				true,
			),
		}
	}
}

impl LintRuleRunner for SortedDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "package.json")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(parsed) = serde_json::from_str::<Value>(ctx.contents) else {
			return Vec::new();
		};

		let mut results = Vec::new();

		for section in [
			"dependencies",
			"devDependencies",
			"peerDependencies",
			"optionalDependencies",
		] {
			let Some(deps) = parsed.get(section) else {
				continue;
			};

			let Some(obj) = deps.as_object() else {
				continue;
			};

			let keys: Vec<_> = obj.keys().collect();
			let mut sorted_keys = keys.clone();
			sorted_keys.sort();

			if keys != sorted_keys {
				let location = LintLocation::new(ctx.manifest_path, 1, 1);
				let message = format!("Dependencies in '{section}' are not sorted alphabetically");

				let mut result = LintResult::new(
					"npm/sorted-dependencies",
					location,
					message,
					config.severity(),
				);

				if config.bool_option("fix", true) {
					// Create a sorted version
					let mut sorted: BTreeMap<&String, &Value> = BTreeMap::new();
					for (k, v) in obj {
						sorted.insert(k, v);
					}
					let replacement = serde_json::to_string_pretty(&sorted).unwrap_or_default();

					result = result.with_fix(LintFix::single(
						"Sort dependencies alphabetically",
						(0, 0),
						replacement,
					));
				}

				results.push(result);
			}
		}

		results
	}
}

/// Rule: required-package-fields
///
/// Enforces required fields in package.json.
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
				"Enforces required fields in package.json",
				LintCategory::Correctness,
				false,
			),
		}
	}
}

impl LintRuleRunner for RequiredPackageFieldsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "package.json")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(parsed) = serde_json::from_str::<Value>(ctx.contents) else {
			return Vec::new();
		};

		let required_fields: Vec<String> =
			config.string_list_option("fields").unwrap_or_else(|| {
				vec![
					"description".to_string(),
					"repository".to_string(),
					"license".to_string(),
				]
			});

		let mut results = Vec::new();

		for field in required_fields {
			if parsed.get(&field).is_none() {
				let location = LintLocation::new(ctx.manifest_path, 1, 1);
				let message = format!("Missing required package.json field: '{field}'");
				results.push(LintResult::new(
					"npm/required-package-fields",
					location,
					message,
					config.severity(),
				));
			}
		}

		results
	}
}

/// Rule: root-no-prod-deps
///
/// Root package.json shouldn't have production dependencies.
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
				"Root package.json should only have devDependencies",
				LintCategory::BestPractice,
				true,
			),
		}
	}
}

impl LintRuleRunner for RootNoProdDepsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "package.json")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		// Only check if this is the root package.json
		if ctx.manifest_path.parent() != Some(ctx.workspace_root) {
			return Vec::new();
		}

		let Ok(parsed) = serde_json::from_str::<Value>(ctx.contents) else {
			return Vec::new();
		};

		let mut results = Vec::new();

		if let Some(deps) = parsed.get("dependencies")
			&& deps.as_object().is_some_and(|o| !o.is_empty())
		{
			let location = LintLocation::new(ctx.manifest_path, 1, 1);
			let message = String::from(
				"Root package.json should not have production dependencies. Use devDependencies instead.",
			);

			let mut result = LintResult::new(
				"npm/root-no-prod-deps",
				location,
				message,
				config.severity(),
			);

			if config.bool_option("fix", true) {
				result = result.with_fix(LintFix::single(
					"Move dependencies to devDependencies",
					(0, 0),
					"Move 'dependencies' to 'devDependencies' section",
				));
			}

			results.push(result);
		}

		results
	}
}

/// Rule: no-duplicate-dependencies
///
/// Prevents the same dependency appearing in multiple dependency sections.
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
				"Same dependency should not appear in multiple dependency sections",
				LintCategory::Correctness,
				true,
			),
		}
	}
}

impl LintRuleRunner for NoDuplicateDependenciesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "package.json")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(parsed) = serde_json::from_str::<Value>(ctx.contents) else {
			return Vec::new();
		};

		let mut all_deps: std::collections::HashMap<String, Vec<String>> =
			std::collections::HashMap::new();

		for section in [
			"dependencies",
			"devDependencies",
			"peerDependencies",
			"optionalDependencies",
		] {
			let Some(deps) = parsed.get(section) else {
				continue;
			};

			let Some(obj) = deps.as_object() else {
				continue;
			};

			for dep_name in obj.keys() {
				all_deps
					.entry(dep_name.clone())
					.or_default()
					.push(section.to_string());
			}
		}

		let mut results = Vec::new();

		for (dep_name, sections) in all_deps {
			if sections.len() > 1 {
				let location = LintLocation::new(ctx.manifest_path, 1, 1);
				let message = format!(
					"Dependency '{}' appears in multiple sections: {}",
					dep_name,
					sections.join(", ")
				);

				let mut result = LintResult::new(
					"npm/no-duplicate-dependencies",
					location,
					message,
					config.severity(),
				);

				if config.bool_option("fix", true) {
					// Suggest keeping only in devDependencies if present
					if sections.contains(&"devDependencies".to_string()) {
						result = result.with_fix(LintFix::single(
							"Remove duplicate from production dependencies",
							(0, 0),
							format!("Remove '{dep_name}' from non-dev dependency sections"),
						));
					}
				}

				results.push(result);
			}
		}

		results
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_workspace_protocol_rule_applies_to_package_json() {
		let rule = WorkspaceProtocolRule::new();
		assert!(rule.applies_to(std::path::Path::new("package.json")));
		assert!(!rule.applies_to(std::path::Path::new("Cargo.toml")));
	}

	#[test]
	fn test_sorted_dependencies_rule() {
		let rule = SortedDependenciesRule::new();
		assert!(rule.applies_to(std::path::Path::new("package.json")));
	}
}

/// Rule: unlisted-package-private
///
/// Requires packages not listed in monochange.toml to be marked as private.
/// This prevents accidental publishing of packages that aren't managed by monochange.
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
				"Packages not defined in monochange.toml must be marked as private to prevent accidental publishing",
				LintCategory::Correctness,
				true, // Autofixable by adding `"private": true`
			),
		}
	}

	fn is_package_in_monochange_toml(package_name: &str, workspace_root: &std::path::Path) -> bool {
		let config_path = workspace_root.join("monochange.toml");
		if !config_path.exists() {
			return false;
		}

		let Ok(contents) = std::fs::read_to_string(&config_path) else {
			return false;
		};

		let Ok(parsed) = toml::from_str::<toml::Value>(&contents) else {
			return false;
		};

		if let Some(packages) = parsed.get("package")
			&& let Some(table) = packages.as_table()
		{
			for (id, package) in table {
				if id == package_name {
					return true;
				}
				if let Some(name) = package.get("name")
					&& let Some(name_str) = name.as_str()
					&& name_str == package_name
				{
					return true;
				}
			}
		}

		false
	}
}

impl LintRuleRunner for UnlistedPackagePrivateRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "package.json")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(parsed) = serde_json::from_str::<Value>(ctx.contents) else {
			return Vec::new();
		};

		let Some(package_name) = parsed.get("name").and_then(|v| v.as_str()) else {
			return Vec::new();
		};

		// Check if package is in monochange.toml
		if Self::is_package_in_monochange_toml(package_name, ctx.workspace_root) {
			return Vec::new();
		}

		// Check if package is already marked as private
		let is_private = parsed
			.get("private")
			.and_then(Value::as_bool)
			.unwrap_or(false);

		if is_private {
			return Vec::new();
		}

		// Package is not in monochange.toml and not private - report error
		let location = LintLocation::new(ctx.manifest_path, 1, 1);
		let message = format!(
			"Package '{package_name}' is not defined in monochange.toml and must be marked as private. \
             Either add it to monochange.toml or set \"private\": true in package.json"
		);

		let mut result = LintResult::new(
			"npm/unlisted-package-private",
			location,
			message,
			config.severity(),
		);

		if config.bool_option("fix", true) {
			// Add `"private": true` after the name field
			result = result.with_fix(LintFix::single(
				"Add `private: true` to package.json",
				(0, 0),
				format!("\"name\": \"{package_name}\",\n  \"private\": true"),
			));
		}

		vec![result]
	}
}
