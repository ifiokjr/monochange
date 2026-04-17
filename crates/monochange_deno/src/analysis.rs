use std::collections::BTreeMap;
use std::path::Path;

use monochange_core::AnalyzedFileChange;
use monochange_core::DetectionLevel;
use monochange_core::Ecosystem;
use monochange_core::MonochangeResult;
use monochange_core::PackageAnalysisContext;
use monochange_core::PackageAnalysisResult;
use monochange_core::PackageRecord;
use monochange_core::SemanticAnalyzer;
use monochange_core::SemanticChange;
use monochange_core::SemanticChangeCategory;
use monochange_core::SemanticChangeKind;
use monochange_ecmascript::EcmascriptExportConfig;
use monochange_ecmascript::diff_public_symbols;
use monochange_ecmascript::snapshot_exported_symbols;
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
			let before_symbols = snapshot_exported_symbols(
				context.before_snapshot,
				context.changed_files,
				&DENO_ECMASCRIPT_EXPORT_CONFIG,
			);
			let after_symbols = snapshot_exported_symbols(
				context.after_snapshot,
				context.changed_files,
				&DENO_ECMASCRIPT_EXPORT_CONFIG,
			);
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

const DENO_ECMASCRIPT_EXPORT_CONFIG: EcmascriptExportConfig = EcmascriptExportConfig {
	source_extensions: &["js", "jsx", "ts", "tsx", "mjs", "mts"],
	module_roots_to_strip: &["src"],
	ignored_module_stems: &["index", "mod"],
	strip_declaration_stem_suffix: false,
	legacy_supports_declare_prefix: false,
	legacy_supports_namespace_exports: false,
};

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

fn is_manifest_file(path: &Path) -> bool {
	matches!(
		path.file_name().and_then(|name| name.to_str()),
		Some("deno.json" | "deno.jsonc")
	)
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
	use std::path::PathBuf;

	use monochange_core::FileChangeKind;

	use super::*;

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
	fn manifest_helpers_cover_parse_failures_removed_entries_and_metadata() {
		assert!(is_manifest_file(Path::new("deno.jsonc")));
		assert!(!is_manifest_file(Path::new("mod.ts")));

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

		let mut nested_exports = BTreeMap::new();
		collect_export_entries(
			"runtime",
			&serde_json::json!({".": "./mod.ts", "./cli": "./cli.ts", "types": "./mod.d.ts"}),
			&mut nested_exports,
		);
		assert!(nested_exports.contains_key("."));
		assert!(nested_exports.contains_key("./cli"));
		assert!(!nested_exports.contains_key("runtime"));

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

		let metadata_entries = extract_metadata_entries(&serde_json::json!({
			"tasks": {"lint": "deno lint"},
			"compilerOptions": {"jsxImportSource": "preact"},
			"vendor": true
		}));
		assert!(metadata_entries.contains_key("task.lint"));
		assert!(metadata_entries.contains_key("compiler_options.jsxImportSource"));
		assert!(metadata_entries.contains_key("vendor"));

		assert!(describe_json_value(&serde_json::json!({"b": 2, "a": [1, 2]})).contains("a=1, 2"));
		assert_eq!(describe_json_value(&serde_json::json!(null)), "null");
		assert_eq!(describe_json_value(&serde_json::json!(3)), "3");
		assert_eq!(describe_json_value(&serde_json::json!("deno")), "deno");
	}
}
