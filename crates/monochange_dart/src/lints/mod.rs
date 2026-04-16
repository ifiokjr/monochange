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
use semver::Version;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Sequence;
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
	pub workspace_package_versions: Arc<BTreeMap<String, Version>>,
}

impl LintSuite for DartLintSuite {
	fn suite_id(&self) -> &'static str {
		"dart"
	}

	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>> {
		vec![
			Box::new(AssetsSortedRule::new()),
			Box::new(DependencySortedRule::new()),
			Box::new(FlutterPackageMetadataConsistentRule::new()),
			Box::new(InternalPathDependencyPolicyRule::new()),
			Box::new(NoGitDependenciesInPublishedPackagesRule::new()),
			Box::new(NoUnexpectedDependencyOverridesRule::new()),
			Box::new(RequiredPackageFieldsRule::new()),
			Box::new(SdkConstraintModernRule::new()),
			Box::new(SdkConstraintPresentRule::new()),
			Box::new(UnlistedPackagePrivateRule::new()),
			Box::new(WorkspaceInternalVersionConsistencyRule::new()),
		]
	}

	fn presets(&self) -> Vec<LintPreset> {
		vec![
			LintPreset::new(
				"dart/recommended",
				"Dart recommended",
				"Balanced Dart manifest linting for metadata, publishability, and baseline SDK hygiene",
				LintMaturity::Stable,
			)
			.with_rules(BTreeMap::from([
				(
					"dart/dependency-sorted".to_string(),
					LintRuleConfig::Severity(LintSeverity::Warning),
				),
				(
					"dart/no-git-dependencies-in-published-packages".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/sdk-constraint-present".to_string(),
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
				"Opinionated Dart manifest linting with workspace and Flutter policy rules enforced",
				LintMaturity::Strict,
			)
			.with_rules(BTreeMap::from([
				(
					"dart/assets-sorted".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/dependency-sorted".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/flutter-package-metadata-consistent".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/internal-path-dependency-policy".to_string(),
					LintRuleConfig::Detailed {
						level: LintSeverity::Error,
						options: BTreeMap::from([("mode".to_string(), serde_json::json!("path"))]),
					},
				),
				(
					"dart/no-git-dependencies-in-published-packages".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/no-unexpected-dependency-overrides".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/required-package-fields".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/sdk-constraint-modern".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/sdk-constraint-present".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/unlisted-package-private".to_string(),
					LintRuleConfig::Severity(LintSeverity::Error),
				),
				(
					"dart/workspace-internal-version-consistency".to_string(),
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
		let workspace_package_versions = Arc::new(
			discovery
				.packages
				.iter()
				.filter_map(|package| {
					package
						.current_version
						.clone()
						.map(|version| (package.name.clone(), version))
				})
				.collect::<BTreeMap<_, _>>(),
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
						workspace_package_versions: Arc::clone(&workspace_package_versions),
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

fn dependency_sections() -> [&'static str; 3] {
	["dependencies", "dev_dependencies", "dependency_overrides"]
}

fn sdk_constraint_value(mapping: &Mapping) -> Option<&str> {
	yaml_mapping(mapping, "environment")?
		.get(yaml_key("sdk"))
		.and_then(Value::as_str)
}

fn parse_constraint_version(value: &str) -> Option<Version> {
	let trimmed = value.trim().trim_matches('"').trim_matches('\'');
	if trimmed.is_empty() {
		return None;
	}
	let mut parts = trimmed.split('.').collect::<Vec<_>>();
	if parts.len() == 1 {
		parts.extend(["0", "0"]);
	} else if parts.len() == 2 {
		parts.push("0");
	}
	Version::parse(&parts.join(".")).ok()
}

#[derive(Debug, Default)]
struct ParsedSdkConstraint {
	lower_bound: Option<Version>,
	has_upper_bound: bool,
}

fn parse_sdk_constraint(value: &str) -> ParsedSdkConstraint {
	let trimmed = value.trim();
	if let Some(caret) = trimmed.strip_prefix('^') {
		return ParsedSdkConstraint {
			lower_bound: parse_constraint_version(caret),
			has_upper_bound: true,
		};
	}

	let mut parsed = ParsedSdkConstraint::default();
	for token in trimmed.split_whitespace() {
		if let Some(version) = token.strip_prefix(">=").or_else(|| token.strip_prefix('>'))
			&& parsed.lower_bound.is_none()
		{
			parsed.lower_bound = parse_constraint_version(version);
		}
		if token.starts_with('<') {
			parsed.has_upper_bound = true;
		}
	}

	parsed
}

fn insert_publish_to_none(contents: &str) -> String {
	if contents.is_empty() {
		return "publish_to: none\n".to_string();
	}

	let separator = if contents.ends_with('\n') { "" } else { "\n" };
	format!("{contents}{separator}publish_to: none\n")
}

fn render_manifest(mapping: &Mapping, fallback: &str) -> String {
	serde_yaml_ng::to_string(mapping).unwrap_or_else(|_| fallback.to_string())
}

fn sort_manifest_section(mapping: &mut Mapping, section: &str) {
	let Some(Value::Mapping(section_mapping)) = mapping.get_mut(yaml_key(section)) else {
		return;
	};
	let mut entries = section_mapping
		.iter()
		.map(|(key, value)| {
			(
				key.as_str().unwrap_or_default().to_string(),
				key.clone(),
				value.clone(),
			)
		})
		.collect::<Vec<_>>();
	entries.sort_by(|left, right| left.0.cmp(&right.0));
	section_mapping.clear();
	for (_, key, value) in entries {
		section_mapping.insert(key, value);
	}
}

fn yaml_line_ranges(contents: &str) -> Vec<(usize, usize)> {
	let mut ranges = Vec::new();
	let mut start = 0usize;
	for (index, ch) in contents.char_indices() {
		if ch == '\n' {
			ranges.push((start, index));
			start = index + 1;
		}
	}
	if start <= contents.len() {
		ranges.push((start, contents.len()));
	}
	ranges
}

struct ParsedYamlLine<'a> {
	indent: usize,
	key: &'a str,
}

fn parse_yaml_line(contents: &str, range: (usize, usize)) -> Option<ParsedYamlLine<'_>> {
	let line = &contents[range.0..range.1];
	let trimmed = line.trim_start_matches([' ', '\t']);
	if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('-') {
		return None;
	}
	let indent = line.len() - trimmed.len();
	let colon = trimmed.find(':')?;
	let key = trimmed[..colon].trim();
	if key.is_empty() {
		return None;
	}
	Some(ParsedYamlLine { indent, key })
}

fn find_yaml_key_line(
	contents: &str,
	line_ranges: &[(usize, usize)],
	indent: usize,
	key: &str,
) -> Option<usize> {
	line_ranges.iter().position(|range| {
		parse_yaml_line(contents, *range)
			.is_some_and(|line| line.indent == indent && line.key == key)
	})
}

fn source_key_order(contents: &str, section: &str) -> Option<Vec<String>> {
	let line_ranges = yaml_line_ranges(contents);
	let section_index = find_yaml_key_line(contents, &line_ranges, 0, section)?;
	let section_line = parse_yaml_line(contents, *line_ranges.get(section_index)?)?;
	let mut keys = Vec::new();
	let mut index = section_index + 1;
	while let Some(range) = line_ranges.get(index) {
		let Some(line) = parse_yaml_line(contents, *range) else {
			index += 1;
			continue;
		};
		if line.indent <= section_line.indent {
			break;
		}
		if line.indent == section_line.indent + 2 {
			keys.push(line.key.to_string());
		}
		index += 1;
	}
	Some(keys)
}

fn dependency_version_text(value: &Value) -> Option<String> {
	match value {
		Value::String(text) => Some(text.clone()),
		Value::Mapping(mapping) => {
			mapping
				.get(yaml_key("version"))
				.and_then(Value::as_str)
				.map(ToString::to_string)
		}
		_ => None,
	}
}

fn dependency_uses_path(value: &Value) -> bool {
	matches!(value, Value::Mapping(mapping) if mapping.contains_key(yaml_key("path")))
}

fn dependency_declares_flutter_sdk(value: &Value) -> bool {
	matches!(
		value,
		Value::Mapping(mapping)
			if mapping
				.get(yaml_key("sdk"))
				.and_then(Value::as_str)
				== Some("flutter")
	)
}

fn first_constraint_version(value: &str) -> Option<Version> {
	for token in value.split_whitespace() {
		let trimmed = token
			.trim()
			.trim_end_matches(',')
			.trim_start_matches(['^', '~', '>', '<', '=']);
		if let Some(version) = parse_constraint_version(trimmed) {
			return Some(version);
		}
	}
	None
}

fn flutter_section(mapping: &Mapping) -> Option<&Mapping> {
	yaml_mapping(mapping, "flutter")
}

fn flutter_section_mut(mapping: &mut Mapping) -> Option<&mut Mapping> {
	mapping
		.get_mut(yaml_key("flutter"))
		.and_then(Value::as_mapping_mut)
}

fn yaml_sequence<'a>(mapping: &'a Mapping, key: &str) -> Option<&'a Sequence> {
	mapping.get(yaml_key(key)).and_then(Value::as_sequence)
}

fn yaml_sequence_mut<'a>(mapping: &'a mut Mapping, key: &str) -> Option<&'a mut Sequence> {
	mapping
		.get_mut(yaml_key(key))
		.and_then(Value::as_sequence_mut)
}

fn value_string(value: &Value) -> String {
	value.as_str().unwrap_or_default().to_string()
}

fn sort_string_sequence(sequence: &mut Sequence) {
	sequence.sort_by_key(value_string);
}

fn sequence_order(sequence: &Sequence) -> Vec<String> {
	sequence.iter().map(value_string).collect()
}

fn sort_flutter_assets_and_fonts(mapping: &mut Mapping) {
	let Some(flutter) = flutter_section_mut(mapping) else {
		return;
	};
	if let Some(assets) = yaml_sequence_mut(flutter, "assets") {
		sort_string_sequence(assets);
	}
	if let Some(fonts) = yaml_sequence_mut(flutter, "fonts") {
		fonts.sort_by_key(|entry| {
			entry
				.as_mapping()
				.and_then(|mapping| mapping.get(yaml_key("family")))
				.and_then(Value::as_str)
				.unwrap_or_default()
				.to_string()
		});
		for entry in fonts {
			let Some(font_entry) = entry.as_mapping_mut() else {
				continue;
			};
			if let Some(font_assets) = yaml_sequence_mut(font_entry, "fonts") {
				font_assets.sort_by_key(|asset| {
					asset
						.as_mapping()
						.and_then(|mapping| mapping.get(yaml_key("asset")))
						.and_then(Value::as_str)
						.unwrap_or_default()
						.to_string()
				});
			}
		}
	}
}

#[derive(Debug)]
struct AssetsSortedRule {
	rule: LintRule,
}

impl AssetsSortedRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/assets-sorted",
				"Flutter assets sorted",
				"Requires Flutter asset and font lists to use a stable alphabetical order",
				LintCategory::Style,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that rewrites Flutter assets and fonts in sorted order",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for AssetsSortedRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		let Some(flutter) = flutter_section(&file.manifest) else {
			return Vec::new();
		};

		let mut messages = Vec::new();
		if let Some(assets) = yaml_sequence(flutter, "assets") {
			let current = sequence_order(assets);
			let mut sorted = current.clone();
			sorted.sort();
			if current != sorted {
				messages.push("flutter.assets is not sorted alphabetically".to_string());
			}
		}
		if let Some(fonts) = yaml_sequence(flutter, "fonts") {
			let current_families = fonts
				.iter()
				.map(|entry| {
					entry
						.as_mapping()
						.and_then(|mapping| mapping.get(yaml_key("family")))
						.and_then(Value::as_str)
						.unwrap_or_default()
						.to_string()
				})
				.collect::<Vec<_>>();
			let mut sorted_families = current_families.clone();
			sorted_families.sort();
			if current_families != sorted_families {
				messages.push("flutter.fonts families are not sorted alphabetically".to_string());
			}
			for entry in fonts {
				let Some(mapping) = entry.as_mapping() else {
					continue;
				};
				let family = mapping
					.get(yaml_key("family"))
					.and_then(Value::as_str)
					.unwrap_or("unknown");
				let Some(font_assets) = yaml_sequence(mapping, "fonts") else {
					continue;
				};
				let current_assets = font_assets
					.iter()
					.map(|asset| {
						asset
							.as_mapping()
							.and_then(|mapping| mapping.get(yaml_key("asset")))
							.and_then(Value::as_str)
							.unwrap_or_default()
							.to_string()
					})
					.collect::<Vec<_>>();
				let mut sorted_assets = current_assets.clone();
				sorted_assets.sort();
				if current_assets != sorted_assets {
					messages.push(format!(
						"flutter.fonts family `{family}` assets are not sorted alphabetically"
					));
				}
			}
		}

		if messages.is_empty() {
			return Vec::new();
		}

		let fix = if config.bool_option("fix", true) {
			let mut rewritten = file.manifest.clone();
			sort_flutter_assets_and_fonts(&mut rewritten);
			Some(LintFix::single(
				"sort Flutter assets and fonts alphabetically",
				(0, ctx.contents.len()),
				render_manifest(&rewritten, ctx.contents),
			))
		} else {
			None
		};

		messages
			.into_iter()
			.map(|message| {
				let mut result = LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					message,
					config.severity(),
				);
				if let Some(fix) = fix.clone() {
					result = result.with_fix(fix);
				}
				result
			})
			.collect()
	}
}

#[derive(Debug)]
struct DependencySortedRule {
	rule: LintRule,
}

impl DependencySortedRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/dependency-sorted",
				"Dependency sections sorted",
				"Requires Dart dependency sections to be alphabetically sorted",
				LintCategory::Style,
				LintMaturity::Stable,
				true,
			)
			.with_options(vec![LintOptionDefinition::new(
				"fix",
				"apply an autofix that rewrites dependency sections in sorted order",
				LintOptionKind::Boolean,
			)]),
		}
	}
}

impl LintRuleRunner for DependencySortedRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in dependency_sections() {
			let Some(mapping) = yaml_mapping(&file.manifest, section) else {
				continue;
			};
			let mut sorted_keys = mapping
				.keys()
				.filter_map(Value::as_str)
				.map(ToString::to_string)
				.collect::<Vec<_>>();
			sorted_keys.sort();
			let source_order = source_key_order(ctx.contents, section).unwrap_or_else(|| {
				mapping
					.keys()
					.filter_map(Value::as_str)
					.map(ToString::to_string)
					.collect::<Vec<_>>()
			});
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
				for sortable in dependency_sections() {
					sort_manifest_section(&mut rewritten, sortable);
				}
				result = result.with_fix(LintFix::single(
					"sort dependency sections alphabetically",
					(0, ctx.contents.len()),
					render_manifest(&rewritten, ctx.contents),
				));
			}

			results.push(result);
		}

		results
	}
}

#[derive(Debug)]
struct FlutterPackageMetadataConsistentRule {
	rule: LintRule,
}

impl FlutterPackageMetadataConsistentRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/flutter-package-metadata-consistent",
				"Flutter package metadata consistent",
				"Requires packages with a flutter section to declare the Flutter SDK dependency consistently",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for FlutterPackageMetadataConsistentRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		if flutter_section(&file.manifest).is_none() {
			return Vec::new();
		}
		let has_flutter_dependency = yaml_mapping(&file.manifest, "dependencies")
			.and_then(|dependencies| dependencies.get(yaml_key("flutter")))
			.is_some_and(dependency_declares_flutter_sdk);
		if has_flutter_dependency {
			return Vec::new();
		}

		vec![LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			"packages with a `flutter` section must declare `dependencies.flutter = { sdk: flutter }`",
			config.severity(),
		)]
	}
}

#[derive(Debug)]
struct InternalPathDependencyPolicyRule {
	rule: LintRule,
}

impl InternalPathDependencyPolicyRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/internal-path-dependency-policy",
				"Internal path dependency policy",
				"Enforces how internal Dart workspace packages reference each other",
				LintCategory::BestPractice,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![LintOptionDefinition::new(
				"mode",
				"dependency policy mode: `path` or `hosted`",
				LintOptionKind::String,
			)]),
		}
	}
}

impl LintRuleRunner for InternalPathDependencyPolicyRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		let mode = config
			.string_option("mode")
			.unwrap_or_else(|| "path".to_string());
		let require_path = mode != "hosted";
		let mut results = Vec::new();

		for section in ["dependencies", "dev_dependencies"] {
			let Some(dependencies) = yaml_mapping(&file.manifest, section) else {
				continue;
			};
			for (dependency_name, value) in dependencies {
				let Some(dependency_name) = dependency_name.as_str() else {
					continue;
				};
				if !file.workspace_package_names.contains(dependency_name) {
					continue;
				}
				let uses_path = dependency_uses_path(value);
				if (require_path && uses_path) || (!require_path && !uses_path) {
					continue;
				}
				let expectation = if require_path {
					"use `path:` references"
				} else {
					"use hosted version references"
				};
				results.push(LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!(
						"internal dependency `{dependency_name}` in `{section}` should {expectation}"
					),
					config.severity(),
				));
			}
		}

		results
	}
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
struct NoUnexpectedDependencyOverridesRule {
	rule: LintRule,
}

impl NoUnexpectedDependencyOverridesRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/no-unexpected-dependency-overrides",
				"No unexpected dependency overrides",
				"Warns when dependency_overrides appear outside explicitly allowed Dart packages",
				LintCategory::BestPractice,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![
				LintOptionDefinition::new(
					"allow_for_private",
					"allow dependency_overrides in private packages",
					LintOptionKind::Boolean,
				),
				LintOptionDefinition::new(
					"allow_packages",
					"list of package names that may declare dependency_overrides",
					LintOptionKind::StringList,
				),
			]),
		}
	}
}

impl LintRuleRunner for NoUnexpectedDependencyOverridesRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		let Some(overrides) = yaml_mapping(&file.manifest, "dependency_overrides") else {
			return Vec::new();
		};
		if overrides.is_empty() {
			return Vec::new();
		}
		if ctx.metadata.private == Some(true) && config.bool_option("allow_for_private", true) {
			return Vec::new();
		}

		let allowed_packages = config
			.string_list_option("allow_packages")
			.unwrap_or_default()
			.into_iter()
			.collect::<BTreeSet<_>>();
		let package_name = ctx.metadata.package_name.as_deref().unwrap_or_default();
		if allowed_packages.contains(package_name) {
			return Vec::new();
		}

		vec![LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			format!(
				"package `{package_name}` declares dependency_overrides without an allow-list entry"
			),
			config.severity(),
		)]
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
struct SdkConstraintModernRule {
	rule: LintRule,
}

impl SdkConstraintModernRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/sdk-constraint-modern",
				"SDK constraint modern",
				"Requires Dart packages to use a modern environment.sdk lower bound and, by default, an upper bound",
				LintCategory::BestPractice,
				LintMaturity::Stable,
				false,
			)
			.with_options(vec![
				LintOptionDefinition::new(
					"minimum",
					"minimum supported SDK version, such as 3.0.0 or 3.6.0",
					LintOptionKind::String,
				),
				LintOptionDefinition::new(
					"require_upper_bound",
					"require environment.sdk to include an upper bound",
					LintOptionKind::Boolean,
				),
			]),
		}
	}
}

impl LintRuleRunner for SdkConstraintModernRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		let Some(sdk_constraint) = sdk_constraint_value(&file.manifest) else {
			return Vec::new();
		};

		let minimum = config
			.string_option("minimum")
			.and_then(|value| parse_constraint_version(&value))
			.unwrap_or_else(|| Version::new(3, 0, 0));
		let require_upper_bound = config.bool_option("require_upper_bound", true);
		let parsed = parse_sdk_constraint(sdk_constraint);
		let mut reasons = Vec::new();

		match parsed.lower_bound {
			Some(ref lower_bound) if lower_bound < &minimum => {
				reasons.push(format!("lower bound must be at least {minimum}"));
			}
			None => reasons.push("lower bound could not be determined".to_string()),
			_ => {}
		}
		if require_upper_bound && !parsed.has_upper_bound {
			reasons.push("constraint should include an upper bound".to_string());
		}
		if reasons.is_empty() {
			return Vec::new();
		}

		vec![LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			format!(
				"environment.sdk `{sdk_constraint}` is not modern enough: {}",
				reasons.join("; ")
			),
			config.severity(),
		)]
	}
}

#[derive(Debug)]
struct SdkConstraintPresentRule {
	rule: LintRule,
}

impl SdkConstraintPresentRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/sdk-constraint-present",
				"SDK constraint present",
				"Requires an explicit environment.sdk constraint in pubspec.yaml",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for SdkConstraintPresentRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		if sdk_constraint_value(&file.manifest).is_some() {
			return Vec::new();
		}

		vec![LintResult::new(
			self.rule.id.clone(),
			location(ctx),
			"pubspec.yaml must declare environment.sdk explicitly",
			config.severity(),
		)]
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

#[derive(Debug)]
struct WorkspaceInternalVersionConsistencyRule {
	rule: LintRule,
}

impl WorkspaceInternalVersionConsistencyRule {
	fn new() -> Self {
		Self {
			rule: LintRule::new(
				"dart/workspace-internal-version-consistency",
				"Workspace internal version consistency",
				"Requires internal Dart dependency version references to match the current workspace package version",
				LintCategory::Correctness,
				LintMaturity::Stable,
				false,
			),
		}
	}
}

impl LintRuleRunner for WorkspaceInternalVersionConsistencyRule {
	fn rule(&self) -> &LintRule {
		&self.rule
	}

	fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult> {
		let Some(file) = dart_file(ctx) else {
			return Vec::new();
		};
		let mut results = Vec::new();

		for section in dependency_sections() {
			let Some(dependencies) = yaml_mapping(&file.manifest, section) else {
				continue;
			};
			for (dependency_name, value) in dependencies {
				let Some(dependency_name) = dependency_name.as_str() else {
					continue;
				};
				let Some(expected_version) = file.workspace_package_versions.get(dependency_name)
				else {
					continue;
				};
				let Some(version_text) = dependency_version_text(value) else {
					continue;
				};
				let Some(referenced_version) = first_constraint_version(&version_text) else {
					continue;
				};
				if referenced_version == *expected_version {
					continue;
				}
				results.push(LintResult::new(
					self.rule.id.clone(),
					location(ctx),
					format!(
						"internal dependency `{dependency_name}` in `{section}` references `{version_text}` but the workspace version is `{expected_version}`"
					),
					config.severity(),
				));
			}
		}

		results
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
}
