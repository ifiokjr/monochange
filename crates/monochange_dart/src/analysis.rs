use std::collections::BTreeMap;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::AnalyzedFileChange;
use monochange_core::DetectionLevel;
use monochange_core::Ecosystem;
use monochange_core::MonochangeResult;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageAnalysisResult;
use monochange_core::PackageRecord;
use monochange_core::PackageSnapshot;
use monochange_core::PackageSnapshotFile;
use monochange_core::SemanticAnalyzer;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use serde_yaml_ng::Mapping;
use serde_yaml_ng::Value;

use crate::PUBSPEC_FILE;

/// Dart and Flutter analyzer that extracts top-level Dart symbols and `pubspec.yaml` diffs.
#[derive(Debug, Clone, Copy, Default)]
pub struct DartSemanticAnalyzer;

/// Return the shared Dart and Flutter semantic analyzer.
#[must_use]
pub const fn semantic_analyzer() -> DartSemanticAnalyzer {
	DartSemanticAnalyzer
}

impl SemanticAnalyzer for DartSemanticAnalyzer {
	fn analyzer_id(&self) -> &'static str {
		"dart/pubspec"
	}

	fn applies_to(&self, package: &PackageRecord) -> bool {
		matches!(package.ecosystem, Ecosystem::Dart | Ecosystem::Flutter)
	}

	fn analyze_package(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> MonochangeResult<PackageAnalysisResult> {
		let mut semantic_changes = Vec::new();
		let mut warnings = Vec::new();

		if context.detection_level != DetectionLevel::Basic {
			let before_symbols =
				snapshot_public_symbols(context.before_snapshot, context.changed_files);
			let after_symbols =
				snapshot_public_symbols(context.after_snapshot, context.changed_files);
			semantic_changes.extend(diff_public_symbols(&before_symbols, &after_symbols));
		}

		if let Some(manifest_change) = context
			.changed_files
			.iter()
			.find(|change| change.package_path == Path::new(PUBSPEC_FILE))
		{
			semantic_changes.extend(analyze_manifest_change(manifest_change, &mut warnings));
		}

		semantic_changes.sort_by(|left, right| {
			(
				left.category,
				left.kind,
				left.item_kind.as_str(),
				left.item_path.as_str(),
			)
				.cmp(&(
					right.category,
					right.kind,
					right.item_kind.as_str(),
					right.item_path.as_str(),
				))
		});

		Ok(PackageAnalysisResult {
			analyzer_id: self.analyzer_id().to_string(),
			package_id: display_package_id(context.package),
			ecosystem: context.package.ecosystem,
			changed_files: context
				.changed_files
				.iter()
				.map(|file| file.package_path.clone())
				.collect(),
			semantic_changes,
			warnings,
		})
	}
}

fn display_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

#[derive(Debug, Clone, Eq, Ord, PartialEq, PartialOrd)]
struct PublicSymbol {
	item_kind: String,
	item_path: String,
	signature: String,
	file_path: PathBuf,
}

fn snapshot_public_symbols(
	snapshot: Option<&PackageSnapshot>,
	changed_files: &[AnalyzedFileChange],
) -> BTreeMap<(String, String), PublicSymbol> {
	let mut symbols = BTreeMap::new();

	if let Some(snapshot) = snapshot {
		for file in &snapshot.files {
			if !is_public_dart_source_file(&file.path) {
				continue;
			}

			for symbol in collect_public_symbols(file) {
				symbols.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
			}
		}

		return symbols;
	}

	for change in changed_files {
		let Some(contents) = change
			.after_contents
			.as_deref()
			.or(change.before_contents.as_deref())
		else {
			continue;
		};
		if !is_public_dart_source_file(&change.package_path) {
			continue;
		}

		let file = PackageSnapshotFile {
			path: change.package_path.clone(),
			contents: contents.to_string(),
		};
		for symbol in collect_public_symbols(&file) {
			symbols.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
		}
	}

	symbols
}

fn is_public_dart_source_file(path: &Path) -> bool {
	path.extension().and_then(|ext| ext.to_str()) == Some("dart")
		&& path.starts_with("lib")
		&& !path.starts_with("lib/src/generated")
}

fn collect_public_symbols(file: &PackageSnapshotFile) -> Vec<PublicSymbol> {
	let module_prefix = module_prefix_for_file(&file.path);
	let mut symbols = Vec::new();
	let mut brace_depth = 0_i32;

	for raw_line in file.contents.lines() {
		let line = trim_inline_comment(raw_line).trim();
		if line.is_empty() {
			brace_depth = update_brace_depth(brace_depth, line);
			continue;
		}

		if brace_depth == 0 {
			let normalized_line = normalize_signature(line);
			if let Some(target) = parse_export_directive(&normalized_line) {
				push_symbol(
					&mut symbols,
					"reexport",
					&[],
					target,
					&normalized_line,
					&file.path,
				);
			}

			if let Some((item_kind, item_name)) = parse_type_declaration(&normalized_line) {
				push_symbol(
					&mut symbols,
					item_kind,
					&module_prefix,
					item_name,
					&normalized_line,
					&file.path,
				);
			}

			if let Some(function_name) = parse_top_level_function(&normalized_line) {
				push_symbol(
					&mut symbols,
					"function",
					&module_prefix,
					function_name,
					&normalized_line,
					&file.path,
				);
			}
		}

		brace_depth = update_brace_depth(brace_depth, line);
	}

	symbols
}

fn trim_inline_comment(line: &str) -> &str {
	line.split("//").next().unwrap_or(line)
}

fn normalize_signature(line: &str) -> String {
	line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn update_brace_depth(current: i32, line: &str) -> i32 {
	let opens = line.matches('{').count() as i32;
	let closes = line.matches('}').count() as i32;
	(current + opens - closes).max(0)
}

fn parse_export_directive(line: &str) -> Option<String> {
	let target = line.strip_prefix("export ")?;
	extract_quoted_text(target).filter(|path| !path.is_empty())
}

fn parse_type_declaration(line: &str) -> Option<(&'static str, String)> {
	for (keyword, item_kind) in [
		("class", "class"),
		("enum", "enum"),
		("mixin", "mixin"),
		("typedef", "typedef"),
		("extension", "extension"),
	] {
		if let Some(name) = find_keyword_name(line, keyword)
			&& !name.starts_with('_')
		{
			return Some((item_kind, name));
		}
	}

	None
}

fn find_keyword_name(line: &str, keyword: &str) -> Option<String> {
	let mut previous = "";
	for token in line.split_whitespace() {
		if previous == keyword {
			let name = token
				.trim_matches(|character: char| {
					!character.is_ascii_alphanumeric() && character != '_'
				})
				.to_string();
			return (!name.is_empty()).then_some(name);
		}
		previous = token;
	}

	None
}

fn parse_top_level_function(line: &str) -> Option<String> {
	if !line.contains('(') || line.starts_with("export ") {
		return None;
	}
	if line.starts_with("if ")
		|| line.starts_with("for ")
		|| line.starts_with("while ")
		|| line.starts_with("switch ")
		|| line.starts_with("return ")
		|| line.starts_with("assert ")
		|| line.starts_with("catch ")
		|| line.starts_with("typedef ")
	{
		return None;
	}

	let open_paren = line.find('(')?;
	let before = line[..open_paren].trim_end();
	if before.is_empty() || before.ends_with('=') {
		return None;
	}

	let name = before
		.split_whitespace()
		.last()?
		.trim_matches(|character: char| !character.is_ascii_alphanumeric() && character != '_')
		.to_string();
	(!name.is_empty() && !name.starts_with('_')).then_some(name)
}

fn extract_quoted_text(text: &str) -> Option<String> {
	let quote = text
		.chars()
		.find(|character| matches!(character, '\'' | '"'))?;
	let start = text.find(quote)? + 1;
	let end = text[start..].find(quote)? + start;
	Some(text[start..end].to_string())
}

fn module_prefix_for_file(path: &Path) -> Vec<String> {
	let mut components = path
		.parent()
		.map(|parent| {
			parent
				.components()
				.filter_map(|component| component.as_os_str().to_str())
				.map(ToString::to_string)
				.collect::<Vec<_>>()
		})
		.unwrap_or_default();

	if components
		.first()
		.is_some_and(|component| component == "lib")
	{
		components.remove(0);
	}

	let stem = path
		.file_stem()
		.and_then(|stem| stem.to_str())
		.unwrap_or_default();
	if !stem.is_empty() && stem != "index" {
		components.push(stem.to_string());
	}

	components
}

#[allow(clippy::needless_pass_by_value)]
fn push_symbol(
	output: &mut Vec<PublicSymbol>,
	item_kind: &str,
	module_prefix: &[String],
	item_name: String,
	signature: &str,
	file_path: &Path,
) {
	let item_path = if module_prefix.is_empty() {
		item_name.clone()
	} else {
		format!("{}::{item_name}", module_prefix.join("::"))
	};

	output.push(PublicSymbol {
		item_kind: item_kind.to_string(),
		item_path,
		signature: signature.to_string(),
		file_path: file_path.to_path_buf(),
	});
}

fn diff_public_symbols(
	before: &BTreeMap<(String, String), PublicSymbol>,
	after: &BTreeMap<(String, String), PublicSymbol>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	for (key, after_symbol) in after {
		match before.get(key) {
			None => {
				changes.push(build_symbol_change(
					SemanticChangeKind::Added,
					after_symbol,
					None,
					Some(after_symbol.signature.clone()),
				));
			}
			Some(before_symbol) if before_symbol.signature != after_symbol.signature => {
				changes.push(build_symbol_change(
					SemanticChangeKind::Modified,
					after_symbol,
					Some(before_symbol.signature.clone()),
					Some(after_symbol.signature.clone()),
				));
			}
			Some(_) => {}
		}
	}

	for (key, before_symbol) in before {
		if after.contains_key(key) {
			continue;
		}

		changes.push(build_symbol_change(
			SemanticChangeKind::Removed,
			before_symbol,
			Some(before_symbol.signature.clone()),
			None,
		));
	}

	changes
}

fn build_symbol_change(
	kind: SemanticChangeKind,
	symbol: &PublicSymbol,
	before_signature: Option<String>,
	after_signature: Option<String>,
) -> SemanticChange {
	let verb = match kind {
		SemanticChangeKind::Added => "added",
		SemanticChangeKind::Removed => "removed",
		SemanticChangeKind::Modified => "modified",
		_ => "changed",
	};

	SemanticChange {
		category: SemanticChangeCategory::PublicApi,
		kind,
		item_kind: symbol.item_kind.clone(),
		item_path: symbol.item_path.clone(),
		summary: format!("{} `{}` {verb}", symbol.item_kind, symbol.item_path),
		file_path: symbol.file_path.clone(),
		before_signature,
		after_signature,
	}
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ManifestEntry {
	item_kind: String,
	value: String,
}

fn analyze_manifest_change(
	change: &AnalyzedFileChange,
	warnings: &mut Vec<String>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	let before_manifest = parse_manifest(
		change.before_contents.as_deref(),
		&change.package_path,
		warnings,
	);
	let after_manifest = parse_manifest(
		change.after_contents.as_deref(),
		&change.package_path,
		warnings,
	);

	let before_exports = before_manifest
		.as_ref()
		.map(extract_export_entries)
		.unwrap_or_default();
	let after_exports = after_manifest
		.as_ref()
		.map(extract_export_entries)
		.unwrap_or_default();
	changes.extend(compare_manifest_entries(
		SemanticChangeCategory::Export,
		&change.package_path,
		&before_exports,
		&after_exports,
	));

	let before_dependencies = before_manifest
		.as_ref()
		.map(extract_dependency_entries)
		.unwrap_or_default();
	let after_dependencies = after_manifest
		.as_ref()
		.map(extract_dependency_entries)
		.unwrap_or_default();
	changes.extend(compare_manifest_entries(
		SemanticChangeCategory::Dependency,
		&change.package_path,
		&before_dependencies,
		&after_dependencies,
	));

	let before_metadata = before_manifest
		.as_ref()
		.map(extract_metadata_entries)
		.unwrap_or_default();
	let after_metadata = after_manifest
		.as_ref()
		.map(extract_metadata_entries)
		.unwrap_or_default();
	changes.extend(compare_manifest_entries(
		SemanticChangeCategory::Metadata,
		&change.package_path,
		&before_metadata,
		&after_metadata,
	));

	changes
}

fn parse_manifest(
	contents: Option<&str>,
	path: &Path,
	warnings: &mut Vec<String>,
) -> Option<Mapping> {
	let contents = contents?;
	match serde_yaml_ng::from_str::<Mapping>(contents) {
		Ok(value) => Some(value),
		Err(error) => {
			warnings.push(format!("failed to parse {}: {error}", path.display()));
			None
		}
	}
}

fn extract_export_entries(value: &Mapping) -> BTreeMap<String, ManifestEntry> {
	value
		.get(Value::String("executables".to_string()))
		.and_then(Value::as_mapping)
		.map(|executables| {
			executables
				.iter()
				.filter_map(|(name, value)| {
					name.as_str().map(|name| {
						(
							name.to_string(),
							ManifestEntry {
								item_kind: "command".to_string(),
								value: describe_yaml_value(value),
							},
						)
					})
				})
				.collect::<BTreeMap<_, _>>()
		})
		.unwrap_or_default()
}

fn extract_dependency_entries(value: &Mapping) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for (section, item_kind) in [
		("dependencies", "dependency"),
		("dev_dependencies", "dev_dependency"),
		("dependency_overrides", "dependency_override"),
	] {
		let Some(section_mapping) = value
			.get(Value::String(section.to_string()))
			.and_then(Value::as_mapping)
		else {
			continue;
		};

		for (name, entry) in section_mapping {
			let Some(name) = name.as_str() else {
				continue;
			};
			entries.insert(
				name.to_string(),
				ManifestEntry {
					item_kind: item_kind.to_string(),
					value: format!("[{section}] {}", describe_yaml_value(entry)),
				},
			);
		}
	}

	entries
}

fn extract_metadata_entries(value: &Mapping) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for field in ["publish_to"] {
		if let Some(field_value) = value.get(Value::String(field.to_string())) {
			entries.insert(
				field.to_string(),
				ManifestEntry {
					item_kind: "manifest_field".to_string(),
					value: describe_yaml_value(field_value),
				},
			);
		}
	}

	if let Some(environment) = value
		.get(Value::String("environment".to_string()))
		.and_then(Value::as_mapping)
	{
		for field in ["sdk", "flutter"] {
			if let Some(field_value) = environment.get(Value::String(field.to_string())) {
				entries.insert(
					format!("environment.{field}"),
					ManifestEntry {
						item_kind: "environment".to_string(),
						value: describe_yaml_value(field_value),
					},
				);
			}
		}
	}

	if let Some(flutter) = value
		.get(Value::String("flutter".to_string()))
		.and_then(Value::as_mapping)
		&& let Some(plugin) = flutter
			.get(Value::String("plugin".to_string()))
			.and_then(Value::as_mapping)
		&& let Some(platforms) = plugin
			.get(Value::String("platforms".to_string()))
			.and_then(Value::as_mapping)
	{
		for (platform_name, platform_value) in platforms {
			let Some(platform_name) = platform_name.as_str() else {
				continue;
			};
			entries.insert(
				format!("flutter.plugin.platform.{platform_name}"),
				ManifestEntry {
					item_kind: "plugin_platform".to_string(),
					value: describe_yaml_value(platform_value),
				},
			);
		}
	}

	entries
}

fn compare_manifest_entries(
	category: SemanticChangeCategory,
	file_path: &Path,
	before: &BTreeMap<String, ManifestEntry>,
	after: &BTreeMap<String, ManifestEntry>,
) -> Vec<SemanticChange> {
	let mut changes = Vec::new();

	for (name, after_entry) in after {
		match before.get(name) {
			None => {
				changes.push(build_manifest_change(
					category,
					SemanticChangeKind::Added,
					file_path,
					name,
					after_entry,
					None,
					Some(after_entry.value.clone()),
				));
			}
			Some(before_entry) if before_entry != after_entry => {
				changes.push(build_manifest_change(
					category,
					SemanticChangeKind::Modified,
					file_path,
					name,
					after_entry,
					Some(before_entry.value.clone()),
					Some(after_entry.value.clone()),
				));
			}
			Some(_) => {}
		}
	}

	for (name, before_entry) in before {
		if after.contains_key(name) {
			continue;
		}

		changes.push(build_manifest_change(
			category,
			SemanticChangeKind::Removed,
			file_path,
			name,
			before_entry,
			Some(before_entry.value.clone()),
			None,
		));
	}

	changes
}

fn build_manifest_change(
	category: SemanticChangeCategory,
	kind: SemanticChangeKind,
	file_path: &Path,
	item_path: &str,
	entry: &ManifestEntry,
	before_signature: Option<String>,
	after_signature: Option<String>,
) -> SemanticChange {
	let verb = match kind {
		SemanticChangeKind::Added => "added",
		SemanticChangeKind::Removed => "removed",
		SemanticChangeKind::Modified => "modified",
		_ => "changed",
	};

	SemanticChange {
		category,
		kind,
		item_kind: entry.item_kind.clone(),
		item_path: item_path.to_string(),
		summary: format!("{} `{}` {verb}", entry.item_kind, item_path),
		file_path: file_path.to_path_buf(),
		before_signature,
		after_signature,
	}
}

fn describe_yaml_value(value: &Value) -> String {
	match value {
		Value::Null => "null".to_string(),
		Value::Bool(boolean) => boolean.to_string(),
		Value::Number(number) => number.to_string(),
		Value::String(text) => text.clone(),
		Value::Sequence(items) => {
			items
				.iter()
				.map(describe_yaml_value)
				.collect::<Vec<_>>()
				.join(", ")
		}
		Value::Mapping(mapping) => {
			let mut fields = mapping
				.iter()
				.filter_map(|(key, value)| {
					key.as_str()
						.map(|key| format!("{key}={}", describe_yaml_value(value)))
				})
				.collect::<Vec<_>>();
			fields.sort();
			fields.join(", ")
		}
		Value::Tagged(tagged) => describe_yaml_value(&tagged.value),
	}
}

#[cfg(test)]
mod tests {
	use monochange_core::FileChangeKind;
	use monochange_core::PublishState;

	use super::*;

	#[test]
	fn analyzer_applies_to_flutter_packages() {
		let package = PackageRecord::new(
			Ecosystem::Flutter,
			"mobile_app",
			PathBuf::from("/repo/packages/mobile/pubspec.yaml"),
			PathBuf::from("/repo"),
			None,
			PublishState::Public,
		);

		assert!(semantic_analyzer().applies_to(&package));
	}

	#[test]
	fn collect_public_symbols_finds_dart_types_functions_and_reexports() {
		let file = PackageSnapshotFile {
			path: PathBuf::from("lib/mobile_app.dart"),
			contents: concat!(
				"export 'src/widgets.dart';\n",
				"class Greeter {}\n",
				"String greet(String name) => 'hello $name';\n",
			)
			.to_string(),
		};

		let symbols = collect_public_symbols(&file);

		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "reexport" && symbol.item_path == "src/widgets.dart"
		}));
		assert!(
			symbols
				.iter()
				.any(|symbol| symbol.item_path == "mobile_app::Greeter")
		);
		assert!(
			symbols
				.iter()
				.any(|symbol| symbol.item_path == "mobile_app::greet")
		);
	}

	#[test]
	fn analyze_manifest_change_reports_dependency_environment_and_flutter_platform_diffs() {
		let change = AnalyzedFileChange {
			path: PathBuf::from("packages/mobile/pubspec.yaml"),
			package_path: PathBuf::from("pubspec.yaml"),
			kind: FileChangeKind::Modified,
			before_contents: Some(
				concat!(
					"name: mobile_app\n",
					"environment:\n",
					"  sdk: ^3.4.0\n",
					"dependencies:\n",
					"  flutter:\n",
					"    sdk: flutter\n",
					"executables:\n",
					"  mobile-app:\n",
					"flutter:\n",
					"  plugin:\n",
					"    platforms:\n",
					"      android:\n",
					"        package: com.example.mobile\n",
				)
				.to_string(),
			),
			after_contents: Some(
				concat!(
					"name: mobile_app\n",
					"publish_to: none\n",
					"environment:\n",
					"  sdk: ^3.5.0\n",
					"dependencies:\n",
					"  flutter:\n",
					"    sdk: flutter\n",
					"  riverpod: ^2.5.0\n",
					"executables:\n",
					"  mobile-app:\n",
					"  mobile-admin:\n",
					"flutter:\n",
					"  plugin:\n",
					"    platforms:\n",
					"      android:\n",
					"        package: com.example.mobile\n",
					"      ios:\n",
					"        pluginClass: MobilePlugin\n",
				)
				.to_string(),
			),
		};
		let mut warnings = Vec::new();
		let changes = analyze_manifest_change(&change, &mut warnings);

		assert!(warnings.is_empty());
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Dependency
				&& change.item_path == "riverpod"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Export
				&& change.item_path == "mobile-admin"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "environment.sdk"
				&& change.kind == SemanticChangeKind::Modified
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "publish_to"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "flutter.plugin.platform.ios"
				&& change.kind == SemanticChangeKind::Added
		}));
	}
}
