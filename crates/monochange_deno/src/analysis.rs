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

/// Deno analyzer that extracts exported JS/TS symbols and `deno.json` semantic diffs.
#[derive(Debug, Clone, Copy, Default)]
pub struct DenoSemanticAnalyzer;

/// Return the shared Deno semantic analyzer.
#[must_use]
pub const fn semantic_analyzer() -> DenoSemanticAnalyzer {
	DenoSemanticAnalyzer
}

impl SemanticAnalyzer for DenoSemanticAnalyzer {
	fn analyzer_id(&self) -> &'static str {
		"deno/manifest"
	}

	fn applies_to(&self, package: &PackageRecord) -> bool {
		package.ecosystem == Ecosystem::Deno
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
			.find(|change| is_manifest_file(&change.package_path))
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
	if !matches!(extension, "js" | "jsx" | "ts" | "tsx" | "mjs" | "mts") {
		return false;
	}

	!path.starts_with("dist") && !path.starts_with("build") && !path.starts_with("node_modules")
}

fn is_manifest_file(path: &Path) -> bool {
	matches!(
		path.file_name().and_then(|name| name.to_str()),
		Some("deno.json" | "deno.jsonc")
	)
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
				target,
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
					item_name,
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
				item_name,
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
		.is_some_and(|component| component == "src")
	{
		components.remove(0);
	}

	let stem = path
		.file_stem()
		.and_then(|stem| stem.to_str())
		.unwrap_or_default();
	if !stem.is_empty() && stem != "index" && stem != "mod" {
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
) -> Option<Value> {
	let contents = contents?;
	let normalized = monochange_core::strip_json_comments(contents);
	match serde_json::from_str::<Value>(&normalized) {
		Ok(value) => Some(value),
		Err(error) => {
			warnings.push(format!("failed to parse {}: {error}", path.display()));
			None
		}
	}
}

fn extract_export_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	if let Some(exports) = value.get("exports") {
		collect_export_entries(".", exports, &mut entries);
	}

	entries
}

fn collect_export_entries(
	item_path: &str,
	value: &Value,
	entries: &mut BTreeMap<String, ManifestEntry>,
) {
	if let Value::Object(object) = value
		&& object.keys().any(|key| key.starts_with('.'))
	{
		for (key, nested) in object {
			if key.starts_with('.') {
				collect_export_entries(key, nested, entries);
			}
		}
		return;
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
	value
		.get("imports")
		.and_then(Value::as_object)
		.map(|imports| {
			imports
				.iter()
				.map(|(name, value)| {
					(
						name.clone(),
						ManifestEntry {
							item_kind: "import_alias".to_string(),
							value: describe_json_value(value),
						},
					)
				})
				.collect::<BTreeMap<_, _>>()
		})
		.unwrap_or_default()
}

fn extract_metadata_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for field in ["lock", "nodeModulesDir", "vendor"] {
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

	entries.extend(
		value
			.get("tasks")
			.and_then(Value::as_object)
			.into_iter()
			.flat_map(|tasks| {
				tasks.iter().map(|(name, task)| {
					(
						format!("task.{name}"),
						ManifestEntry {
							item_kind: "task".to_string(),
							value: describe_json_value(task),
						},
					)
				})
			}),
	);

	if let Some(compiler_options) = value.get("compilerOptions").and_then(Value::as_object) {
		for field in ["jsx", "jsxImportSource"] {
			if let Some(field_value) = compiler_options.get(field) {
				entries.insert(
					format!("compiler_options.{field}"),
					ManifestEntry {
						item_kind: "compiler_option".to_string(),
						value: describe_json_value(field_value),
					},
				);
			}
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
	fn collect_public_symbols_finds_exported_items() {
		let file = PackageSnapshotFile {
			path: PathBuf::from("mod.ts"),
			contents: concat!(
				"export function run() {}\n",
				"export { build } from './build.ts';\n",
				"export * from './types.ts';\n",
			)
			.to_string(),
		};

		let symbols = collect_public_symbols(&file);

		assert!(symbols.iter().any(|symbol| symbol.item_path == "run"));
		assert!(symbols.iter().any(|symbol| symbol.item_path == "build"));
		assert!(symbols.iter().any(|symbol| {
			symbol.item_kind == "wildcard_reexport" && symbol.item_path == "./types.ts"
		}));
	}

	#[test]
	fn analyze_manifest_change_reports_export_import_and_task_diffs() {
		let change = AnalyzedFileChange {
			path: PathBuf::from("deno.json"),
			package_path: PathBuf::from("deno.json"),
			kind: FileChangeKind::Modified,
			before_contents: Some(
				serde_json::json!({
					"exports": "./mod.ts",
					"imports": {"@std/assert": "jsr:@std/assert@1.0.0"},
					"tasks": {"test": "deno test"}
				})
				.to_string(),
			),
			after_contents: Some(
				serde_json::json!({
					"exports": {".": "./mod.ts", "./cli": "./cli.ts"},
					"imports": {
						"@std/assert": "jsr:@std/assert@1.0.0",
						"npm:zod": "npm:zod@3.24.0"
					},
					"tasks": {"test": "deno test", "lint": "deno lint"},
					"compilerOptions": {"jsx": "react-jsx"}
				})
				.to_string(),
			),
		};
		let mut warnings = Vec::new();
		let changes = analyze_manifest_change(&change, &mut warnings);

		assert!(warnings.is_empty());
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Export
				&& change.item_path == "./cli"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Dependency
				&& change.item_path == "npm:zod"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "task.lint"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "compiler_options.jsx"
				&& change.kind == SemanticChangeKind::Added
		}));
	}

	#[test]
	fn snapshot_and_manifest_helpers_cover_additional_deno_branches() {
		let changed_files = vec![
			AnalyzedFileChange {
				path: PathBuf::from("mod.ts"),
				package_path: PathBuf::from("mod.ts"),
				kind: FileChangeKind::Modified,
				before_contents: Some("export const previous = true;".to_string()),
				after_contents: None,
			},
			AnalyzedFileChange {
				path: PathBuf::from("cli.mts"),
				package_path: PathBuf::from("cli.mts"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some(
					concat!(
						"export default async function run() {}\n",
						"export abstract class Runner {}\n",
						"export { build } from './build.ts';\n",
					)
					.to_string(),
				),
			},
		];

		let symbols = snapshot_exported_symbols(None, &changed_files);
		for expected in ["previous", "cli::run", "cli::Runner", "cli::build"] {
			assert!(
				symbols
					.iter()
					.any(|((_, item_path), _)| item_path == expected)
			);
		}
		assert!(is_source_file(Path::new("cli.mts")));
		assert!(!is_source_file(Path::new("build/mod.ts")));
		assert!(is_manifest_file(Path::new("deno.jsonc")));
		assert_eq!(
			module_prefix_for_file(Path::new("src/tools/index.ts")),
			vec!["tools".to_string()]
		);

		let mut warnings = Vec::new();
		assert!(parse_manifest(Some("{"), Path::new("deno.json"), &mut warnings).is_none());
		assert_eq!(warnings.len(), 1);

		let before = serde_json::json!({
			"exports": {".": "./mod.ts", "./cli": "./cli.ts"},
			"imports": {"@std/assert": "jsr:@std/assert@1.0.0"},
			"tasks": {"lint": "deno lint"},
			"compilerOptions": {"jsx": "react-jsx"},
			"lock": true
		});
		let after = serde_json::json!({
			"exports": {".": "./mod.ts"},
			"imports": {},
			"tasks": {},
			"compilerOptions": {},
			"lock": false
		});
		let export_changes = compare_manifest_entries(
			SemanticChangeCategory::Export,
			Path::new("deno.json"),
			&extract_export_entries(&before),
			&extract_export_entries(&after),
		);
		assert!(export_changes.iter().any(|change| {
			change.item_path == "./cli" && change.kind == SemanticChangeKind::Removed
		}));
		let metadata_changes = compare_manifest_entries(
			SemanticChangeCategory::Metadata,
			Path::new("deno.json"),
			&extract_metadata_entries(&before),
			&extract_metadata_entries(&after),
		);
		assert!(metadata_changes.iter().any(|change| {
			change.item_path == "lock" && change.kind == SemanticChangeKind::Modified
		}));
		assert!(metadata_changes.iter().any(|change| {
			change.item_path == "task.lint" && change.kind == SemanticChangeKind::Removed
		}));
		assert!(describe_json_value(&serde_json::json!({"b": 2, "a": [1, 2]})).contains("a=1, 2"));
	}

	#[test]
	fn parser_diff_and_metadata_helpers_cover_remaining_deno_branches() {
		let skipped_symbols = snapshot_exported_symbols(
			None,
			&[
				AnalyzedFileChange {
					path: PathBuf::from("mod.ts"),
					package_path: PathBuf::from("mod.ts"),
					kind: FileChangeKind::Modified,
					before_contents: None,
					after_contents: None,
				},
				AnalyzedFileChange {
					path: PathBuf::from("README.md"),
					package_path: PathBuf::from("README.md"),
					kind: FileChangeKind::Modified,
					before_contents: None,
					after_contents: Some("ignored".to_string()),
				},
			],
		);
		assert!(skipped_symbols.is_empty());
		assert!(!is_source_file(Path::new("mod")));

		assert!(parse_named_exports("const nope = true;").is_empty());
		assert_eq!(
			parse_named_exports("export type { Foo as Bar, , Baz } from './types.ts';"),
			vec![
				("type_reexport".to_string(), "Bar".to_string()),
				("type_reexport".to_string(), "Baz".to_string()),
			]
		);
		assert!(parse_named_exports("export { Foo as Bar from './types.ts';").is_empty());
		assert_eq!(
			parse_declaration_export("export default {};"),
			Some(("default_export", "default".to_string()))
		);

		let before = BTreeMap::from([
			(
				("function".to_string(), "run".to_string()),
				PublicSymbol {
					item_kind: "function".to_string(),
					item_path: "run".to_string(),
					signature: "export function run() {}".to_string(),
					file_path: PathBuf::from("mod.ts"),
				},
			),
			(
				("class".to_string(), "Runner".to_string()),
				PublicSymbol {
					item_kind: "class".to_string(),
					item_path: "Runner".to_string(),
					signature: "export class Runner {}".to_string(),
					file_path: PathBuf::from("mod.ts"),
				},
			),
		]);
		let after = BTreeMap::from([(
			("function".to_string(), "run".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "run".to_string(),
				signature: "export function run() {}".to_string(),
				file_path: PathBuf::from("mod.ts"),
			},
		)]);
		let changes = diff_public_symbols(&before, &after);
		assert_eq!(changes.len(), 1);
		let change = changes
			.first()
			.unwrap_or_else(|| panic!("expected one removed change"));
		assert_eq!(change.kind, SemanticChangeKind::Removed);
		assert!(change.summary.contains("removed"));

		let mut nested_exports = BTreeMap::new();
		collect_export_entries(
			"runtime",
			&serde_json::json!({".": "./mod.ts", "./cli": "./cli.ts", "types": "./mod.d.ts"}),
			&mut nested_exports,
		);
		assert!(nested_exports.contains_key("."));
		assert!(nested_exports.contains_key("./cli"));
		assert!(!nested_exports.contains_key("runtime"));

		let metadata_entries = extract_metadata_entries(&serde_json::json!({
			"tasks": {"lint": "deno lint"},
			"compilerOptions": {"jsxImportSource": "preact"},
			"vendor": true
		}));
		assert!(metadata_entries.contains_key("task.lint"));
		assert!(metadata_entries.contains_key("compiler_options.jsxImportSource"));
		assert!(metadata_entries.contains_key("vendor"));

		assert_eq!(describe_json_value(&serde_json::json!(null)), "null");
		assert_eq!(describe_json_value(&serde_json::json!(3)), "3");
		assert_eq!(describe_json_value(&serde_json::json!("deno")), "deno");
	}
}
