#![forbid(clippy::indexing_slicing)]

//! Cargo-specific lint rules.

use std::collections::BTreeSet;

use monochange_core::lint::LintCategory;
use monochange_core::lint::LintContext;
use monochange_core::lint::LintFix;
use monochange_core::lint::LintLocation;
use monochange_core::lint::LintResult;
use monochange_core::lint::LintRule;
use monochange_core::lint::LintRuleConfig;
use monochange_core::lint::LintRuleRunner;
use monochange_core::lint::LintSeverity;
use toml_edit::DocumentMut;
use toml_edit::Item;
use toml_edit::Value;

/// Cargo lint rules.
pub struct CargoLintRules;

impl CargoLintRules {
	/// Return the default set of Cargo lint rules.
	#[must_use]
	pub fn default_rules() -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(DependencyFieldOrderRule::new()),
			Box::new(InternalDependencyWorkspaceRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(SortedDependenciesRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
		]
	}
}

/// Rule: dependency-field-order
///
/// Enforces consistent ordering of dependency specification fields.
/// The preferred order is:
/// 1. `workspace = true` OR `version = "..."` (mutually exclusive, must come first)
/// 2. `default-features = ...` (if present)
/// 3. `features = [...]`
/// 4. Other fields (optional, path, registry, etc.)
#[derive(Debug)]
struct DependencyFieldOrderRule {
	rule: LintRule,
}

impl DependencyFieldOrderRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"cargo/dependency-field-order",
				"Dependency field order",
				"Enforces consistent ordering of dependency specification fields",
				LintCategory::Style,
				true,
			),
		}
	}

	fn check_dependency_table(
		&self,
		dep_name: &str,
		table: &toml_edit::Table,
		location: &LintLocation,
		config: &LintRuleConfig,
	) -> Option<LintResult> {
		let preferred_order = [
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
		];

		let mut actual_order: Vec<(&str, usize)> = table
			.iter()
			.filter_map(|(key, _)| {
				preferred_order
					.iter()
					.position(|&p| p == key)
					.map(|pos| (key, pos))
			})
			.collect();

		actual_order.sort_by_key(|&(_, pos)| pos);

		let keys: Vec<_> = table.iter().map(|(k, _)| k).collect();
		let expected_keys: Vec<_> = actual_order.iter().map(|(k, _)| *k).collect();

		if keys != expected_keys {
			let message = format!(
				"Dependency '{}' has fields in incorrect order. Preferred order: workspace/version first, then default-features, then features, then others",
				dep_name
			);

			let span = location.span;
			let fix = if config.bool_option("fix", true) {
				// Generate fix by reordering fields
				Some(LintFix::single(
					"Reorder dependency fields",
					span.unwrap_or((0, 0)),
					"[reordered fields]",
				))
			} else {
				None
			};

			let mut result = LintResult::new(
				"cargo/dependency-field-order",
				location.clone(),
				message,
				config.severity(),
			);

			if fix.is_some() {
				result = result.with_fix(fix.unwrap());
			}

			return Some(result);
		}

		None
	}
}

impl LintRuleRunner for DependencyFieldOrderRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "Cargo.toml")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(doc) = ctx.contents.parse::<DocumentMut>() else {
			return Vec::new();
		};

		let mut results = Vec::new();

		// Check dependencies, dev-dependencies, build-dependencies
		for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
			let Some(deps) = doc.get(section) else {
				continue;
			};

			let Some(table) = deps.as_table() else {
				continue;
			};

			for (dep_name, value) in table.iter() {
				if let Some(table) = value.as_table() {
					let location = LintLocation::new(ctx.manifest_path, 1, 1);
					if let Some(result) =
						self.check_dependency_table(dep_name, table, &location, config)
					{
						results.push(result);
					}
				}
			}
		}

		results
	}
}

/// Rule: internal-dependency-workspace
///
/// Requires `workspace = true` for internal dependencies.
#[derive(Debug)]
struct InternalDependencyWorkspaceRule {
	rule: LintRule,
}

impl InternalDependencyWorkspaceRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"cargo/internal-dependency-workspace",
				"Internal dependency workspace",
				"Requires workspace = true for internal dependencies",
				LintCategory::Correctness,
				true,
			),
		}
	}
}

impl LintRuleRunner for InternalDependencyWorkspaceRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "Cargo.toml")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(doc) = ctx.contents.parse::<DocumentMut>() else {
			return Vec::new();
		};

		let mut results = Vec::new();
		let require_workspace = config.bool_option("require_workspace", true);

		// Check dependencies sections
		for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
			let Some(deps) = doc.get(section) else {
				continue;
			};

			let Some(table) = deps.as_table() else {
				continue;
			};

			for (dep_name, value) in table.iter() {
				// Check if this looks like an internal dependency (has path or is simple version)
				let is_internal = match value {
					Item::Value(v) if v.is_str() => {
						// Simple version like "1.0" - might be internal
						// Check if path exists in workspace
						let potential_path = ctx.workspace_root.join("crates").join(dep_name);
						potential_path.exists()
					}
					Item::Table(t) if t.contains_key("path") => true,
					_ => false,
				};

				if is_internal && require_workspace {
					// Check if workspace = true is set
					let has_workspace = match value {
						Item::Table(t) => {
							t.get("workspace")
								.is_some_and(|v| v.as_bool().is_some_and(|b| b))
						}
						_ => false,
					};

					if !has_workspace {
						// Find the position for the error
						let span = value.span();
						let location = LintLocation::new(ctx.manifest_path, 1, 1)
							.with_span(span.start, span.end);

						let message = format!(
							"Internal dependency '{}' should use 'workspace = true'",
							dep_name
						);

						let mut result = LintResult::new(
							"cargo/internal-dependency-workspace",
							location,
							message,
							config.severity(),
						);

						if config.bool_option("fix", true) {
							// Generate autofix
							let replacement = if let Some(table) = value.as_table() {
								let mut new_table = table.clone();
								new_table["workspace"] = toml_edit::value(true);
								// Remove version if present since workspace provides it
								new_table.remove("version");
								format!("{} = {}", dep_name, new_table)
							} else {
								format!("{} = {{ workspace = true }}", dep_name)
							};

							result = result.with_fix(LintFix::single(
								"Add workspace = true",
								(span.start, span.end),
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

/// Rule: required-package-fields
///
/// Enforces required fields in the [package] section.
#[derive(Debug)]
struct RequiredPackageFieldsRule {
	rule: LintRule,
}

impl RequiredPackageFieldsRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"cargo/required-package-fields",
				"Required package fields",
				"Enforces required fields in [package] section",
				LintCategory::Correctness,
				false, // Not autofixable
			),
		}
	}
}

impl LintRuleRunner for RequiredPackageFieldsRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn applies_to(&self, path: &std::path::Path) -> bool {
		path.file_name().is_some_and(|name| name == "Cargo.toml")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(doc) = ctx.contents.parse::<DocumentMut>() else {
			return Vec::new();
		};

		let Some(package) = doc.get("package") else {
			return Vec::new();
		};

		let Some(table) = package.as_table() else {
			return Vec::new();
		};

		let required_fields: Vec<String> =
			config.string_list_option("fields").unwrap_or_else(|| {
				vec![
					"description".to_string(),
					"license".to_string(),
					"repository".to_string(),
				]
			});

		let mut results = Vec::new();

		for field in required_fields {
			if !table.contains_key(&field) {
				let location = LintLocation::new(ctx.manifest_path, 1, 1);
				let message = format!("Missing required package field: '{}'", field);
				results.push(LintResult::new(
					"cargo/required-package-fields",
					location,
					message,
					config.severity(),
				));
			}
		}

		results
	}
}

/// Rule: sorted-dependencies
///
/// Requires alphabetically sorted dependency tables.
#[derive(Debug)]
struct SortedDependenciesRule {
	rule: LintRule,
}

impl SortedDependenciesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"cargo/sorted-dependencies",
				"Sorted dependencies",
				"Requires alphabetically sorted dependency tables",
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
		path.file_name().is_some_and(|name| name == "Cargo.toml")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(doc) = ctx.contents.parse::<DocumentMut>() else {
			return Vec::new();
		};

		let mut results = Vec::new();

		for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
			let Some(deps) = doc.get(section) else {
				continue;
			};

			let Some(table) = deps.as_table() else {
				continue;
			};

			let keys: Vec<_> = table.iter().map(|(k, _)| k).collect();
			let mut sorted_keys = keys.clone();
			sorted_keys.sort();

			if keys != sorted_keys {
				let location = LintLocation::new(ctx.manifest_path, 1, 1);
				let message = format!(
					"Dependencies in '{}' are not sorted alphabetically",
					section
				);

				let mut result = LintResult::new(
					"cargo/sorted-dependencies",
					location,
					message,
					config.severity(),
				);

				if config.bool_option("fix", true) {
					// Generate a fix by creating a new sorted table
					let mut new_table = toml_edit::Table::new();
					for key in sorted_keys.iter() {
						if let Some(value) = table.get(key) {
							new_table[key] = value.clone();
						}
					}
					let replacement = format!("[{}]\n{}", section, new_table);

					result = result.with_fix(LintFix::single(
						"Sort dependencies alphabetically",
						(0, 0), // Would need actual span
						replacement,
					));
				}

				results.push(result);
			}
		}

		results
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
				"cargo/unlisted-package-private",
				"Unlisted package must be private",
				"Packages not defined in monochange.toml must be marked as private to prevent accidental publishing",
				LintCategory::Correctness,
				true, // Autofixable by adding `publish = false`
			),
		}
	}

	fn is_package_in_monochange_toml(
		&self,
		package_name: &str,
		workspace_root: &std::path::Path,
	) -> bool {
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

		// Check if package is defined in [package] section
		if let Some(packages) = parsed.get("package") {
			if let Some(table) = packages.as_table() {
				for (id, package) in table {
					// Check by id
					if id == package_name {
						return true;
					}
					// Also check by name field if present
					if let Some(name) = package.get("name") {
						if let Some(name_str) = name.as_str() {
							if name_str == package_name {
								return true;
							}
						}
					}
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
		path.file_name().is_some_and(|name| name == "Cargo.toml")
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		if !config.severity().is_enabled() {
			return Vec::new();
		}

		let Ok(doc) = ctx.contents.parse::<DocumentMut>() else {
			return Vec::new();
		};

		// Get package name from Cargo.toml
		let Some(package) = doc.get("package") else {
			return Vec::new();
		};

		let Some(package_table) = package.as_table() else {
			return Vec::new();
		};

		let Some(name_value) = package_table.get("name") else {
			return Vec::new();
		};

		let Some(package_name) = name_value.as_str() else {
			return Vec::new();
		};

		// Check if package is in monochange.toml
		if self.is_package_in_monochange_toml(package_name, ctx.workspace_root) {
			return Vec::new();
		}

		// Check if package is already marked as private
		let is_private = package_table
			.get("publish")
			.and_then(|v| v.as_bool())
			.is_some_and(|b| !b);

		if is_private {
			return Vec::new();
		}

		// Package is not in monochange.toml and not private - report error
		let location = LintLocation::new(ctx.manifest_path, 1, 1);
		let message = format!(
			"Package '{}' is not defined in monochange.toml and must be marked as private. \
             Either add it to monochange.toml or set `publish = false` in Cargo.toml",
			package_name
		);

		let mut result = LintResult::new(
			"cargo/unlisted-package-private",
			location,
			message,
			config.severity(),
		);

		if config.bool_option("fix", true) {
			// Add `publish = false` to [package]
			let span = package.span();
			result = result.with_fix(LintFix::single(
				"Add `publish = false` to [package]",
				(span.end, span.end),
				"\npublish = false".to_string(),
			));
		}

		vec![result]
	}
}
