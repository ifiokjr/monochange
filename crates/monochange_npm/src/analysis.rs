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
			let before_symbols = snapshot_exported_symbols(
				context.before_snapshot,
				context.changed_files,
				&NPM_ECMASCRIPT_EXPORT_CONFIG,
			);
			let after_symbols = snapshot_exported_symbols(
				context.after_snapshot,
				context.changed_files,
				&NPM_ECMASCRIPT_EXPORT_CONFIG,
			);
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

const NPM_ECMASCRIPT_EXPORT_CONFIG: EcmascriptExportConfig = EcmascriptExportConfig {
	source_extensions: &["js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts"],
	module_roots_to_strip: &["src", "lib"],
	ignored_module_stems: &["index"],
	strip_declaration_stem_suffix: true,
	legacy_supports_declare_prefix: true,
	legacy_supports_namespace_exports: true,
};

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
	use std::path::PathBuf;

	use monochange_core::FileChangeKind;

	use super::*;

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

		assert_eq!(describe_json_value(&serde_json::json!(null)), "null");
		assert_eq!(describe_json_value(&serde_json::json!(true)), "true");
		assert_eq!(describe_json_value(&serde_json::json!(3)), "3");
		assert!(describe_json_value(&serde_json::json!(["a", "b"])).contains("a, b"));
		assert!(describe_json_value(&serde_json::json!({"b": 2, "a": 1})).contains("a=1"));
	}
}
