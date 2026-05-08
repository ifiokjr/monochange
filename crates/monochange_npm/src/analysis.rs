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
#[path = "__tests/analysis.rs"]
mod tests;
