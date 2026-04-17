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
use serde_json::Value;

use crate::PACKAGE_JSON_FILE;

/// npm-family analyzer that extracts exported JS/TS symbols plus package manifest diffs.
#[derive(Debug, Clone, Copy, Default)]
pub struct NpmSemanticAnalyzer;

/// Return the shared npm-family semantic analyzer.
#[must_use]
pub const fn semantic_analyzer() -> NpmSemanticAnalyzer {
	NpmSemanticAnalyzer
}

impl SemanticAnalyzer for NpmSemanticAnalyzer {
	fn analyzer_id(&self) -> &'static str {
		"npm/package-json"
	}

	fn applies_to(&self, package: &PackageRecord) -> bool {
		package.ecosystem == Ecosystem::Npm
	}

	fn analyze_package(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> MonochangeResult<PackageAnalysisResult> {
		let mut semantic_changes = Vec::new();
		let mut warnings = Vec::new();

		if context.detection_level != DetectionLevel::Basic {
			let before_symbols =
				snapshot_exported_symbols(context.before_snapshot, context.changed_files);
			let after_symbols =
				snapshot_exported_symbols(context.after_snapshot, context.changed_files);
			semantic_changes.extend(diff_public_symbols(&before_symbols, &after_symbols));
		}

		if let Some(manifest_change) = context
			.changed_files
			.iter()
			.find(|change| change.package_path == Path::new(PACKAGE_JSON_FILE))
		{
			semantic_changes.extend(analyze_manifest_change(
				context.package,
				manifest_change,
				&mut warnings,
			));
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

fn snapshot_exported_symbols(
	snapshot: Option<&PackageSnapshot>,
	changed_files: &[AnalyzedFileChange],
) -> BTreeMap<(String, String), PublicSymbol> {
	let mut symbols = BTreeMap::new();

	if let Some(snapshot) = snapshot {
		for file in &snapshot.files {
			if !is_source_file(&file.path) {
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
		if !is_source_file(&change.package_path) {
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

fn is_source_file(path: &Path) -> bool {
	let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
		return false;
	};
	if !matches!(
		extension,
		"js" | "jsx" | "ts" | "tsx" | "mjs" | "cjs" | "mts" | "cts"
	) {
		return false;
	}

	!path.starts_with("dist") && !path.starts_with("build") && !path.starts_with("node_modules")
}

fn collect_public_symbols(file: &PackageSnapshotFile) -> Vec<PublicSymbol> {
	let module_prefix = module_prefix_for_file(&file.path);
	let mut symbols = Vec::new();

	for raw_line in file.contents.lines() {
		let line = normalize_signature(raw_line);
		if !line.starts_with("export ") || line == "export {}" || line == "export {};" {
			continue;
		}

		if let Some(target) = parse_wildcard_reexport(&line) {
			push_symbol(
				&mut symbols,
				"wildcard_reexport",
				&module_prefix,
				&target,
				&line,
				&file.path,
			);
			continue;
		}

		let named_exports = parse_named_exports(&line);
		if !named_exports.is_empty() {
			for (item_kind, item_name) in named_exports {
				push_symbol(
					&mut symbols,
					&item_kind,
					&module_prefix,
					&item_name,
					&line,
					&file.path,
				);
			}
			continue;
		}

		if let Some((item_kind, item_name)) = parse_declaration_export(&line) {
			push_symbol(
				&mut symbols,
				item_kind,
				&module_prefix,
				&item_name,
				&line,
				&file.path,
			);
		}
	}

	symbols
}

fn normalize_signature(line: &str) -> String {
	line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn parse_wildcard_reexport(line: &str) -> Option<String> {
	let rest = line.strip_prefix("export ")?;
	let target = rest.strip_prefix("* from ")?;
	extract_quoted_text(target)
}

fn parse_named_exports(line: &str) -> Vec<(String, String)> {
	let Some(mut rest) = line.strip_prefix("export ") else {
		return Vec::new();
	};
	let mut item_kind = "reexport";
	if let Some(stripped) = rest.strip_prefix("type ") {
		rest = stripped;
		item_kind = "type_reexport";
	}
	if !rest.starts_with('{') {
		return Vec::new();
	}
	let brace_start = rest
		.find('{')
		.expect("named exports should include an opening brace after the prefix check");
	let Some(brace_end) = rest[brace_start + 1..].find('}') else {
		return Vec::new();
	};
	let inside = &rest[brace_start + 1..brace_start + 1 + brace_end];

	inside
		.split(',')
		.filter_map(|entry| {
			let entry = entry.trim();
			if entry.is_empty() {
				return None;
			}
			let exported_name = entry.split(" as ").last().unwrap_or(entry).trim();
			(!exported_name.is_empty()).then(|| (item_kind.to_string(), exported_name.to_string()))
		})
		.collect()
}

fn parse_declaration_export(line: &str) -> Option<(&'static str, String)> {
	let mut rest = line.strip_prefix("export ")?;
	if let Some(stripped) = rest.strip_prefix("declare ") {
		rest = stripped;
	}
	if let Some(stripped) = rest.strip_prefix("default ") {
		rest = stripped;
	}
	if let Some(stripped) = rest.strip_prefix("async ") {
		rest = stripped;
	}
	if let Some(stripped) = rest.strip_prefix("abstract ") {
		rest = stripped;
	}

	for (prefix, item_kind) in [
		("function ", "function"),
		("class ", "class"),
		("const ", "constant"),
		("let ", "variable"),
		("var ", "variable"),
		("interface ", "interface"),
		("type ", "type_alias"),
		("enum ", "enum"),
		("namespace ", "namespace"),
	] {
		if let Some(stripped) = rest.strip_prefix(prefix)
			&& let Some(name) = take_identifier(stripped)
		{
			return Some((item_kind, name));
		}
	}

	Some(("default_export", "default".to_string())).filter(|_| line.starts_with("export default "))
}

fn take_identifier(text: &str) -> Option<String> {
	let identifier = text
		.chars()
		.take_while(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '$'))
		.collect::<String>();
	(!identifier.is_empty()).then_some(identifier)
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
		.is_some_and(|component| component == "src" || component == "lib")
	{
		components.remove(0);
	}

	let stem = path
		.file_stem()
		.and_then(|stem| stem.to_str())
		.map(|stem| stem.strip_suffix(".d").unwrap_or(stem))
		.unwrap_or_default();
	if !stem.is_empty() && stem != "index" {
		components.push(stem.to_string());
	}

	components
}

fn push_symbol(
	output: &mut Vec<PublicSymbol>,
	item_kind: &str,
	module_prefix: &[String],
	item_name: &str,
	signature: &str,
	file_path: &Path,
) {
	let item_path = if module_prefix.is_empty() {
		item_name.to_string()
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
	let verb = if kind == SemanticChangeKind::Added {
		"added"
	} else if kind == SemanticChangeKind::Removed {
		"removed"
	} else {
		"modified"
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
	package: &PackageRecord,
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

	let before_public_exports = before_manifest
		.as_ref()
		.map(|value| extract_public_exports(value, &package.name))
		.unwrap_or_default();
	let after_public_exports = after_manifest
		.as_ref()
		.map(|value| extract_public_exports(value, &package.name))
		.unwrap_or_default();
	changes.extend(compare_manifest_entries(
		SemanticChangeCategory::Export,
		&change.package_path,
		&before_public_exports,
		&after_public_exports,
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
) -> Option<Value> {
	let contents = contents?;
	match serde_json::from_str::<Value>(contents) {
		Ok(value) => Some(value),
		Err(error) => {
			warnings.push(format!("failed to parse {}: {error}", path.display()));
			None
		}
	}
}

fn extract_public_exports(value: &Value, package_name: &str) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	if let Some(exports) = value.get("exports") {
		collect_export_entries(".", exports, &mut entries);
	}

	if let Some(bin) = value.get("bin") {
		match bin {
			Value::String(path) => {
				entries.insert(
					package_name.to_string(),
					ManifestEntry {
						item_kind: "command".to_string(),
						value: path.clone(),
					},
				);
			}
			Value::Object(commands) => {
				for (name, path) in commands {
					entries.insert(
						name.clone(),
						ManifestEntry {
							item_kind: "command".to_string(),
							value: describe_json_value(path),
						},
					);
				}
			}
			_ => {}
		}
	}

	entries
}

fn collect_export_entries(
	item_path: &str,
	value: &Value,
	entries: &mut BTreeMap<String, ManifestEntry>,
) {
	if let Value::Object(object) = value {
		let has_subpath_keys = object.keys().any(|key| key.starts_with('.'));
		if has_subpath_keys {
			for (key, nested) in object {
				if key.starts_with('.') {
					collect_export_entries(key, nested, entries);
				}
			}
			return;
		}
	}

	entries.insert(
		item_path.to_string(),
		ManifestEntry {
			item_kind: "export".to_string(),
			value: describe_json_value(value),
		},
	);
}

fn extract_dependency_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for (section, item_kind) in [
		("dependencies", "dependency"),
		("devDependencies", "dev_dependency"),
		("peerDependencies", "peer_dependency"),
		("optionalDependencies", "optional_dependency"),
	] {
		let Some(section_object) = value.get(section).and_then(Value::as_object) else {
			continue;
		};

		for (name, entry) in section_object {
			entries.insert(
				name.clone(),
				ManifestEntry {
					item_kind: item_kind.to_string(),
					value: format!("[{section}] {}", describe_json_value(entry)),
				},
			);
		}
	}

	entries
}

fn extract_metadata_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for field in [
		"type",
		"main",
		"module",
		"types",
		"browser",
		"sideEffects",
		"packageManager",
	] {
		if let Some(field_value) = value.get(field) {
			entries.insert(
				field.to_string(),
				ManifestEntry {
					item_kind: "manifest_field".to_string(),
					value: describe_json_value(field_value),
				},
			);
		}
	}

	if let Some(scripts) = value.get("scripts").and_then(Value::as_object) {
		for (name, script) in scripts {
			entries.insert(
				format!("script.{name}"),
				ManifestEntry {
					item_kind: "script".to_string(),
					value: describe_json_value(script),
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
	let verb = if kind == SemanticChangeKind::Added {
		"added"
	} else if kind == SemanticChangeKind::Removed {
		"removed"
	} else {
		"modified"
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

fn describe_json_value(value: &Value) -> String {
	match value {
		Value::Null => "null".to_string(),
		Value::Bool(boolean) => boolean.to_string(),
		Value::Number(number) => number.to_string(),
		Value::String(text) => text.clone(),
		Value::Array(items) => {
			items
				.iter()
				.map(describe_json_value)
				.collect::<Vec<_>>()
				.join(", ")
		}
		Value::Object(object) => {
			let mut fields = object
				.iter()
				.map(|(key, value)| format!("{key}={}", describe_json_value(value)))
				.collect::<Vec<_>>();
			fields.sort();
			fields.join(", ")
		}
	}
}

#[cfg(test)]
mod tests {
	use monochange_core::FileChangeKind;

	use super::*;

	#[test]
	fn module_prefix_for_nested_source_file_tracks_directory_components() {
		assert_eq!(
			module_prefix_for_file(Path::new("src/api/index.ts")),
			vec!["api".to_string()]
		);
		assert_eq!(
			module_prefix_for_file(Path::new("src/utils/format.ts")),
			vec!["utils".to_string(), "format".to_string()]
		);
	}

	#[test]
	fn collect_public_symbols_finds_exported_declarations_and_reexports() {
		let file = PackageSnapshotFile {
			path: PathBuf::from("src/index.ts"),
			contents: concat!(
				"export function greet(name: string): string { return name; }\n",
				"export const version = '1.0.0';\n",
				"export { formatGreeting as format } from './format';\n",
				"export * from './types';\n",
			)
			.to_string(),
		};

		let symbols = collect_public_symbols(&file);

		assert!(symbols.iter().any(|symbol| symbol.item_path == "greet"));
		assert!(symbols.iter().any(|symbol| symbol.item_path == "version"));
		assert!(symbols.iter().any(|symbol| symbol.item_path == "format"));
		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "wildcard_reexport" && symbol.item_path == "./types"
		}));
	}

	#[test]
	fn analyze_manifest_change_reports_export_dependency_and_metadata_diffs() {
		let package = PackageRecord::new(
			Ecosystem::Npm,
			"@acme/web",
			PathBuf::from("/repo/packages/web/package.json"),
			PathBuf::from("/repo"),
			None,
			monochange_core::PublishState::Public,
		);
		let change = AnalyzedFileChange {
			path: PathBuf::from("packages/web/package.json"),
			package_path: PathBuf::from("package.json"),
			kind: FileChangeKind::Modified,
			before_contents: Some(
				serde_json::json!({
					"name": "@acme/web",
					"type": "module",
					"exports": "./src/index.ts",
					"dependencies": {"react": "18.2.0"}
				})
				.to_string(),
			),
			after_contents: Some(
				serde_json::json!({
					"name": "@acme/web",
					"type": "commonjs",
					"exports": {
						".": {"default": "./dist/index.js", "types": "./dist/index.d.ts"},
						"./cli": "./dist/cli.js"
					},
					"bin": {"acme-web": "./dist/cli.js"},
					"dependencies": {"react": "18.2.0", "zod": "3.24.0"},
					"scripts": {"build": "tsup"}
				})
				.to_string(),
			),
		};
		let mut warnings = Vec::new();
		let changes = analyze_manifest_change(&package, &change, &mut warnings);

		assert!(warnings.is_empty());
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Export
				&& change.item_path == "."
				&& change.kind == SemanticChangeKind::Modified
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Export
				&& change.item_path == "./cli"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Export
				&& change.item_path == "acme-web"
				&& change.item_kind == "command"
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Dependency
				&& change.item_path == "zod"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "type"
				&& change.kind == SemanticChangeKind::Modified
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "script.build"
				&& change.kind == SemanticChangeKind::Added
		}));
	}

	#[test]
	fn snapshot_and_symbol_helpers_cover_additional_export_forms() {
		let changed_files = vec![
			AnalyzedFileChange {
				path: PathBuf::from("packages/web/src/index.ts"),
				package_path: PathBuf::from("src/index.ts"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some(
					concat!(
						"export declare async function greet(name: string): Promise<string> { return name; }\n",
						"export default class Greeter {}\n",
						"export abstract class BaseGreeter {}\n",
						"export namespace Tools {}\n",
						"export type { Foo as Bar } from './types';\n",
					)
					.to_string(),
				),
			},
			AnalyzedFileChange {
				path: PathBuf::from("packages/web/README.md"),
				package_path: PathBuf::from("README.md"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some("ignored".to_string()),
			},
			AnalyzedFileChange {
				path: PathBuf::from("packages/web/src/legacy.ts"),
				package_path: PathBuf::from("src/legacy.ts"),
				kind: FileChangeKind::Modified,
				before_contents: Some("export const previous = true;".to_string()),
				after_contents: None,
			},
		];

		let symbols = snapshot_exported_symbols(None, &changed_files);

		for expected in [
			"greet",
			"Greeter",
			"BaseGreeter",
			"Tools",
			"Bar",
			"legacy::previous",
		] {
			assert!(
				symbols
					.iter()
					.any(|((_, item_path), _)| item_path == expected)
			);
		}
		assert!(is_source_file(Path::new("src/index.cts")));
		assert!(!is_source_file(Path::new("build/index.ts")));
		assert_eq!(
			module_prefix_for_file(Path::new("lib/utils/index.d.ts")),
			vec!["utils".to_string()]
		);
	}

	#[test]
	fn manifest_helpers_cover_parse_failures_removed_entries_and_scalar_bins() {
		let mut warnings = Vec::new();
		assert!(parse_manifest(Some("{"), Path::new("package.json"), &mut warnings).is_none());
		assert_eq!(warnings.len(), 1);

		let before = serde_json::json!({
			"exports": {".": "./dist/index.js", "./cli": "./dist/cli.js"},
			"bin": "./dist/index.js",
			"dependencies": {"react": "18"},
			"type": "module",
			"scripts": {"build": "tsup"}
		});
		let after = serde_json::json!({
			"exports": {".": "./dist/index.js"},
			"dependencies": {},
			"type": "commonjs"
		});

		let before_exports = extract_public_exports(&before, "pkg");
		let after_exports = extract_public_exports(&after, "pkg");
		let export_changes = compare_manifest_entries(
			SemanticChangeCategory::Export,
			Path::new("package.json"),
			&before_exports,
			&after_exports,
		);
		assert!(export_changes.iter().any(|change| {
			change.item_path == "./cli" && change.kind == SemanticChangeKind::Removed
		}));
		assert!(export_changes.iter().any(|change| {
			change.item_path == "pkg" && change.kind == SemanticChangeKind::Removed
		}));

		let metadata_changes = compare_manifest_entries(
			SemanticChangeCategory::Metadata,
			Path::new("package.json"),
			&extract_metadata_entries(&before),
			&extract_metadata_entries(&after),
		);
		assert!(metadata_changes.iter().any(|change| {
			change.item_path == "type" && change.kind == SemanticChangeKind::Modified
		}));
		assert!(metadata_changes.iter().any(|change| {
			change.item_path == "script.build" && change.kind == SemanticChangeKind::Removed
		}));

		assert_eq!(describe_json_value(&serde_json::json!(null)), "null");
		assert_eq!(describe_json_value(&serde_json::json!(true)), "true");
		assert_eq!(describe_json_value(&serde_json::json!(3)), "3");
		assert!(describe_json_value(&serde_json::json!(["a", "b"])).contains("a, b"));
		assert!(describe_json_value(&serde_json::json!({"b": 2, "a": 1})).contains("a=1"));
	}

	#[test]
	fn parser_diff_and_export_helpers_cover_remaining_npm_branches() {
		let skipped_symbols = snapshot_exported_symbols(
			None,
			&[
				AnalyzedFileChange {
					path: PathBuf::from("packages/web/src/empty.ts"),
					package_path: PathBuf::from("src/empty.ts"),
					kind: FileChangeKind::Modified,
					before_contents: None,
					after_contents: None,
				},
				AnalyzedFileChange {
					path: PathBuf::from("packages/web/README.md"),
					package_path: PathBuf::from("README.md"),
					kind: FileChangeKind::Modified,
					before_contents: None,
					after_contents: Some("ignored".to_string()),
				},
			],
		);
		assert!(skipped_symbols.is_empty());
		assert!(!is_source_file(Path::new("src/index")));

		assert!(parse_named_exports("const nope = true;").is_empty());
		assert_eq!(
			parse_named_exports("export type { Foo as Bar, , Baz } from './types';"),
			vec![
				("type_reexport".to_string(), "Bar".to_string()),
				("type_reexport".to_string(), "Baz".to_string()),
			]
		);
		assert!(parse_named_exports("export { Foo as Bar from './types';").is_empty());
		assert_eq!(
			parse_declaration_export("export default {};"),
			Some(("default_export", "default".to_string()))
		);

		let before = BTreeMap::from([
			(
				("function".to_string(), "greet".to_string()),
				PublicSymbol {
					item_kind: "function".to_string(),
					item_path: "greet".to_string(),
					signature: "export function greet() {}".to_string(),
					file_path: PathBuf::from("src/index.ts"),
				},
			),
			(
				("class".to_string(), "Greeter".to_string()),
				PublicSymbol {
					item_kind: "class".to_string(),
					item_path: "Greeter".to_string(),
					signature: "export class Greeter {}".to_string(),
					file_path: PathBuf::from("src/index.ts"),
				},
			),
		]);
		let after = BTreeMap::from([(
			("function".to_string(), "greet".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "greet".to_string(),
				signature: "export function greet() {}".to_string(),
				file_path: PathBuf::from("src/index.ts"),
			},
		)]);
		let changes = diff_public_symbols(&before, &after);
		assert_eq!(changes.len(), 1);
		let change = changes
			.first()
			.unwrap_or_else(|| panic!("expected one removed change"));
		assert_eq!(change.kind, SemanticChangeKind::Removed);
		assert!(change.summary.contains("removed"));

		let exports = extract_public_exports(
			&serde_json::json!({
				"exports": {".": "./dist/index.js", "./cli": "./dist/cli.js"},
				"bin": 7
			}),
			"pkg",
		);
		assert!(exports.contains_key("."));
		assert!(exports.contains_key("./cli"));
		assert!(!exports.contains_key("pkg"));
	}
}
