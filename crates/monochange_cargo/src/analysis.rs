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
use quote::ToTokens;
use toml::Value;

use crate::CARGO_MANIFEST_FILE;

/// Cargo analyzer that extracts public Rust API, dependency, and manifest metadata diffs.
#[derive(Debug, Clone, Copy, Default)]
pub struct CargoSemanticAnalyzer;

/// Return the shared Cargo semantic analyzer.
#[must_use]
pub const fn semantic_analyzer() -> CargoSemanticAnalyzer {
	CargoSemanticAnalyzer
}

impl SemanticAnalyzer for CargoSemanticAnalyzer {
	fn analyzer_id(&self) -> &'static str {
		"cargo/public-api"
	}

	fn applies_to(&self, package: &PackageRecord) -> bool {
		package.ecosystem == Ecosystem::Cargo
	}

	fn analyze_package(
		&self,
		context: &PackageAnalysisContext<'_>,
	) -> MonochangeResult<PackageAnalysisResult> {
		let mut warnings = Vec::new();
		let mut semantic_changes = Vec::new();

		let (before_symbols, mut before_warnings) = snapshot_public_symbols(
			context.before_snapshot,
			context.changed_files,
			context.detection_level,
		);
		let (after_symbols, mut after_warnings) = snapshot_public_symbols(
			context.after_snapshot,
			context.changed_files,
			context.detection_level,
		);
		warnings.append(&mut before_warnings);
		warnings.append(&mut after_warnings);

		semantic_changes.extend(diff_public_symbols(&before_symbols, &after_symbols));

		if let Some(manifest_change) = context
			.changed_files
			.iter()
			.find(|change| change.package_path == Path::new(CARGO_MANIFEST_FILE))
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
	_detection_level: DetectionLevel,
) -> (BTreeMap<(String, String), PublicSymbol>, Vec<String>) {
	let mut warnings = Vec::new();
	let mut symbols = BTreeMap::new();

	if let Some(snapshot) = snapshot {
		for file in &snapshot.files {
			if !is_rust_source_file(file) {
				continue;
			}

			match collect_public_symbols(file) {
				Ok(file_symbols) => {
					for symbol in file_symbols {
						symbols
							.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
					}
				}
				Err(error) => warnings.push(error),
			}
		}

		return (symbols, warnings);
	}

	for change in changed_files {
		let contents = change
			.after_contents
			.as_deref()
			.or(change.before_contents.as_deref());
		let Some(contents) = contents else {
			continue;
		};
		let file = PackageSnapshotFile {
			path: change.package_path.clone(),
			contents: contents.to_string(),
		};
		if !is_rust_source_file(&file) {
			continue;
		}

		match collect_public_symbols(&file) {
			Ok(file_symbols) => {
				for symbol in file_symbols {
					symbols.insert((symbol.item_kind.clone(), symbol.item_path.clone()), symbol);
				}
			}
			Err(error) => warnings.push(error),
		}
	}

	(symbols, warnings)
}

fn is_rust_source_file(file: &PackageSnapshotFile) -> bool {
	file.path.extension().and_then(|ext| ext.to_str()) == Some("rs") && file.path.starts_with("src")
}

fn collect_public_symbols(file: &PackageSnapshotFile) -> Result<Vec<PublicSymbol>, String> {
	let module_prefix = module_prefix_for_file(&file.path);
	let parsed = syn::parse_file(&file.contents)
		.map_err(|error| format!("failed to parse {}: {error}", file.path.display()))?;
	let mut symbols = Vec::new();
	collect_public_symbols_from_items(&parsed.items, &module_prefix, &file.path, &mut symbols);
	Ok(symbols)
}

fn collect_public_symbols_from_items(
	items: &[syn::Item],
	module_prefix: &[String],
	file_path: &Path,
	output: &mut Vec<PublicSymbol>,
) {
	for item in items {
		match item {
			syn::Item::Const(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"constant",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Enum(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"enum",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Fn(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"function",
					module_prefix,
					item.sig.ident.to_string(),
					render_signature(&item.sig),
					file_path,
				);
			}
			syn::Item::Mod(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"module",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);

				if let Some((_, nested_items)) = &item.content {
					let mut nested_prefix = module_prefix.to_vec();
					nested_prefix.push(item.ident.to_string());
					collect_public_symbols_from_items(
						nested_items,
						&nested_prefix,
						file_path,
						output,
					);
				}
			}
			syn::Item::Static(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"static",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Struct(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"struct",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Trait(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"trait",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Type(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"type_alias",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Union(item) if is_public(&item.vis) => {
				push_symbol(
					output,
					"union",
					module_prefix,
					item.ident.to_string(),
					render_signature(item),
					file_path,
				);
			}
			syn::Item::Use(item) if is_public(&item.vis) => {
				let use_tree = render_signature(&item.tree);
				push_symbol(
					output,
					"reexport",
					module_prefix,
					use_tree.clone(),
					format!("pub use {use_tree};"),
					file_path,
				);
			}
			_ => {}
		}
	}
}

#[allow(clippy::needless_pass_by_value)]
fn push_symbol(
	output: &mut Vec<PublicSymbol>,
	item_kind: &str,
	module_prefix: &[String],
	item_name: String,
	signature: String,
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
		signature,
		file_path: file_path.to_path_buf(),
	});
}

fn is_public(visibility: &syn::Visibility) -> bool {
	matches!(visibility, syn::Visibility::Public(_))
}

fn render_signature(value: &impl ToTokens) -> String {
	value.to_token_stream().to_string()
}

fn module_prefix_for_file(path: &Path) -> Vec<String> {
	let mut components = path
		.components()
		.map(|component| component.as_os_str().to_string_lossy().to_string())
		.collect::<Vec<_>>();

	if components
		.first()
		.is_some_and(|component| component == "src")
	{
		components.remove(0);
	}

	let Some(last) = components.pop() else {
		return Vec::new();
	};

	let stem = last.strip_suffix(".rs").unwrap_or(&last);
	if stem != "lib" && stem != "main" && stem != "mod" {
		components.push(stem.to_string());
	}

	components
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
	match toml::from_str::<Value>(contents) {
		Ok(value) => Some(value),
		Err(error) => {
			warnings.push(format!("failed to parse {}: {error}", path.display()));
			None
		}
	}
}

fn extract_dependency_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
		let Some(table) = value.get(section).and_then(Value::as_table) else {
			continue;
		};

		for (name, dependency) in table {
			entries.insert(
				name.clone(),
				ManifestEntry {
					item_kind: "dependency".to_string(),
					value: format!("[{section}] {}", describe_manifest_value(dependency)),
				},
			);
		}
	}

	entries
}

fn extract_metadata_entries(value: &Value) -> BTreeMap<String, ManifestEntry> {
	let mut entries = BTreeMap::new();

	for field in ["edition", "rust-version", "publish"] {
		if let Some(field_value) = value
			.get("package")
			.and_then(Value::as_table)
			.and_then(|table| table.get(field))
		{
			entries.insert(
				format!("package.{field}"),
				ManifestEntry {
					item_kind: "manifest_field".to_string(),
					value: describe_manifest_value(field_value),
				},
			);
		}
	}

	if let Some(features) = value.get("features").and_then(Value::as_table) {
		for (name, feature) in features {
			entries.insert(
				format!("feature.{name}"),
				ManifestEntry {
					item_kind: "feature".to_string(),
					value: describe_manifest_value(feature),
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

fn describe_manifest_value(value: &Value) -> String {
	match value {
		Value::String(text) => text.clone(),
		Value::Array(items) => {
			items
				.iter()
				.map(describe_manifest_value)
				.collect::<Vec<_>>()
				.join(", ")
		}
		Value::Table(table) => {
			table
				.iter()
				.map(|(key, value)| format!("{key}={}", describe_manifest_value(value)))
				.collect::<Vec<_>>()
				.join(", ")
		}
		other => other.to_string(),
	}
}

#[cfg(test)]
mod tests {
	use monochange_core::AnalyzedFileChange;
	use monochange_core::FileChangeKind;

	use super::*;

	#[test]
	fn module_prefix_for_root_library_file_is_empty() {
		assert!(module_prefix_for_file(Path::new("src/lib.rs")).is_empty());
	}

	#[test]
	fn module_prefix_for_nested_module_tracks_path_components() {
		assert_eq!(
			module_prefix_for_file(Path::new("src/api/render.rs")),
			vec!["api".to_string(), "render".to_string()]
		);
		assert_eq!(
			module_prefix_for_file(Path::new("src/api/mod.rs")),
			vec!["api".to_string()]
		);
	}

	#[test]
	fn analyze_manifest_change_reports_dependency_and_feature_diffs() {
		let change = AnalyzedFileChange {
			path: PathBuf::from("crates/core/Cargo.toml"),
			package_path: PathBuf::from("Cargo.toml"),
			kind: FileChangeKind::Modified,
			before_contents: Some(
				"[package]\nname = \"core\"\nedition = \"2021\"\n\n[dependencies]\nserde = \"1\"\n\n[features]\ndefault = []\n"
					.to_string(),
			),
			after_contents: Some(
				"[package]\nname = \"core\"\nedition = \"2024\"\n\n[dependencies]\nserde = \"1\"\ntracing = \"0.1\"\n\n[features]\ndefault = [\"cli\"]\ncli = []\n"
					.to_string(),
			),
		};
		let mut warnings = Vec::new();
		let changes = analyze_manifest_change(&change, &mut warnings);

		assert!(warnings.is_empty());
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Dependency
				&& change.item_path == "tracing"
				&& change.kind == SemanticChangeKind::Added
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "package.edition"
				&& change.kind == SemanticChangeKind::Modified
		}));
		assert!(changes.iter().any(|change| {
			change.category == SemanticChangeCategory::Metadata
				&& change.item_path == "feature.cli"
				&& change.kind == SemanticChangeKind::Added
		}));
	}

	#[test]
	fn collect_public_symbols_finds_public_items() {
		let file = PackageSnapshotFile {
			path: PathBuf::from("src/lib.rs"),
			contents: concat!(
				"pub struct Greeter;\n",
				"pub fn greet() {}\n",
				"pub mod api { pub fn render() {} }\n",
				"fn helper() {}\n",
			)
			.to_string(),
		};

		let symbols = collect_public_symbols(&file)
			.unwrap_or_else(|error| panic!("symbol extraction should succeed: {error}"));

		assert!(symbols.iter().any(|symbol| symbol.item_path == "Greeter"));
		assert!(symbols.iter().any(|symbol| symbol.item_path == "greet"));
		assert!(symbols.iter().any(|symbol| symbol.item_path == "api"));
		assert!(
			symbols
				.iter()
				.any(|symbol| symbol.item_path == "api::render")
		);
		assert!(!symbols.iter().any(|symbol| symbol.item_path == "helper"));
	}

	#[test]
	fn snapshot_public_symbols_uses_changed_files_and_collects_warnings() {
		let changed_files = vec![
			AnalyzedFileChange {
				path: PathBuf::from("crates/core/src/lib.rs"),
				package_path: PathBuf::from("src/lib.rs"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some("pub struct Greeter;".to_string()),
			},
			AnalyzedFileChange {
				path: PathBuf::from("crates/core/src/helper.txt"),
				package_path: PathBuf::from("src/helper.txt"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some("ignored".to_string()),
			},
			AnalyzedFileChange {
				path: PathBuf::from("crates/core/src/bad.rs"),
				package_path: PathBuf::from("src/bad.rs"),
				kind: FileChangeKind::Modified,
				before_contents: Some("pub fn broken(".to_string()),
				after_contents: None,
			},
			AnalyzedFileChange {
				path: PathBuf::from("crates/core/src/empty.rs"),
				package_path: PathBuf::from("src/empty.rs"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: None,
			},
		];

		let (symbols, warnings) =
			snapshot_public_symbols(None, &changed_files, DetectionLevel::Signature);

		assert!(symbols.contains_key(&("struct".to_string(), "Greeter".to_string())));
		assert_eq!(warnings.len(), 1);
		assert!(
			warnings
				.first()
				.unwrap_or_else(|| panic!("expected one parse warning"))
				.contains("failed to parse src/bad.rs")
		);
	}

	#[test]
	fn collect_public_symbols_covers_all_supported_public_item_kinds() {
		let file = PackageSnapshotFile {
			path: PathBuf::from("src/api.rs"),
			contents: concat!(
				"pub const LIMIT: usize = 3;\n",
				"pub enum Mode { Fast }\n",
				"pub static NAME: &str = \"core\";\n",
				"pub struct Greeter;\n",
				"pub trait Renderer {}\n",
				"pub type Greeting = String;\n",
				"pub union Number { value: u32 }\n",
				"pub use crate::helpers::render;\n",
			)
			.to_string(),
		};

		let symbols = collect_public_symbols(&file)
			.unwrap_or_else(|error| panic!("symbol extraction should succeed: {error}"));

		for expected in [
			"LIMIT",
			"Mode",
			"NAME",
			"Greeter",
			"Renderer",
			"Greeting",
			"Number",
			"crate :: helpers :: render",
		] {
			assert!(
				symbols
					.iter()
					.any(|symbol| symbol.item_path.ends_with(expected))
			);
		}
	}

	#[test]
	fn module_prefix_and_symbol_diff_cover_root_removed_and_unchanged_paths() {
		assert!(module_prefix_for_file(Path::new("src/lib.rs")).is_empty());
		assert!(module_prefix_for_file(Path::new("src/main.rs")).is_empty());
		assert!(module_prefix_for_file(Path::new("lib.rs")).is_empty());

		let before = BTreeMap::from([
			(
				("function".to_string(), "greet".to_string()),
				PublicSymbol {
					item_kind: "function".to_string(),
					item_path: "greet".to_string(),
					signature: "pub fn greet()".to_string(),
					file_path: PathBuf::from("src/lib.rs"),
				},
			),
			(
				("struct".to_string(), "Greeter".to_string()),
				PublicSymbol {
					item_kind: "struct".to_string(),
					item_path: "Greeter".to_string(),
					signature: "pub struct Greeter;".to_string(),
					file_path: PathBuf::from("src/lib.rs"),
				},
			),
		]);
		let after = BTreeMap::from([
			(
				("function".to_string(), "greet".to_string()),
				PublicSymbol {
					item_kind: "function".to_string(),
					item_path: "greet".to_string(),
					signature: "pub fn greet(name: &str)".to_string(),
					file_path: PathBuf::from("src/lib.rs"),
				},
			),
			(
				("constant".to_string(), "LIMIT".to_string()),
				PublicSymbol {
					item_kind: "constant".to_string(),
					item_path: "LIMIT".to_string(),
					signature: "pub const LIMIT: usize = 3;".to_string(),
					file_path: PathBuf::from("src/lib.rs"),
				},
			),
		]);

		let changes = diff_public_symbols(&before, &after);

		assert!(
			changes
				.iter()
				.any(|change| change.kind == SemanticChangeKind::Modified)
		);
		assert!(
			changes
				.iter()
				.any(|change| change.kind == SemanticChangeKind::Removed)
		);
		assert!(changes.iter().all(|change| {
			change.summary.contains("added")
				|| change.summary.contains("modified")
				|| change.summary.contains("removed")
		}));
	}

	#[test]
	fn snapshot_public_symbols_collects_snapshot_parse_warnings() {
		let snapshot = PackageSnapshot {
			label: "HEAD".to_string(),
			files: vec![PackageSnapshotFile {
				path: PathBuf::from("src/lib.rs"),
				contents: "pub fn broken(".to_string(),
			}],
		};

		let (_, warnings) =
			snapshot_public_symbols(Some(&snapshot), &[], DetectionLevel::Signature);

		assert_eq!(warnings.len(), 1);
		assert!(
			warnings
				.first()
				.unwrap_or_else(|| panic!("expected one parse warning"))
				.contains("failed to parse src/lib.rs")
		);
	}

	#[test]
	fn manifest_helpers_cover_parse_failures_removed_entries_and_table_values() {
		let mut warnings = Vec::new();
		assert!(
			parse_manifest(Some("not = [valid"), Path::new("Cargo.toml"), &mut warnings).is_none()
		);
		assert_eq!(warnings.len(), 1);

		let before = toml::from_str::<Value>(
			"[package]\nedition = \"2021\"\n\n[features]\ndefault = [\"cli\"]\n",
		)
		.unwrap_or_else(|error| panic!("parse before manifest: {error}"));
		let after = toml::from_str::<Value>("[package]\nedition = \"2024\"\n")
			.unwrap_or_else(|error| panic!("parse after manifest: {error}"));

		let before_metadata = extract_metadata_entries(&before);
		let after_metadata = extract_metadata_entries(&after);
		let changes = compare_manifest_entries(
			SemanticChangeCategory::Metadata,
			Path::new("Cargo.toml"),
			&before_metadata,
			&after_metadata,
		);

		assert!(changes.iter().any(|change| {
			change.item_path == "package.edition" && change.kind == SemanticChangeKind::Modified
		}));
		assert!(changes.iter().any(|change| {
			change.item_path == "feature.default" && change.kind == SemanticChangeKind::Removed
		}));
		let after_edition = after
			.get("package")
			.and_then(Value::as_table)
			.and_then(|package| package.get("edition"))
			.unwrap_or_else(|| panic!("expected package.edition"));
		assert_eq!(describe_manifest_value(after_edition), "2024");
		let before_default_feature = before
			.get("features")
			.and_then(Value::as_table)
			.and_then(|features| features.get("default"))
			.unwrap_or_else(|| panic!("expected features.default"));
		assert!(describe_manifest_value(before_default_feature).contains("cli"));
		let dependency_table = toml::from_str::<Value>("[dep]\nserde = \"1\"\n")
			.unwrap_or_else(|error| panic!("parse table manifest: {error}"));
		let dependency_value = dependency_table
			.get("dep")
			.unwrap_or_else(|| panic!("expected dep table"));
		assert!(describe_manifest_value(dependency_value).contains("serde=1"));
	}

	#[test]
	fn module_prefix_diff_and_manifest_helpers_cover_remaining_branches() {
		assert!(module_prefix_for_file(Path::new("src")).is_empty());

		let before = BTreeMap::from([
			(
				("function".to_string(), "greet".to_string()),
				PublicSymbol {
					item_kind: "function".to_string(),
					item_path: "greet".to_string(),
					signature: "pub fn greet()".to_string(),
					file_path: PathBuf::from("src/lib.rs"),
				},
			),
			(
				("struct".to_string(), "Greeter".to_string()),
				PublicSymbol {
					item_kind: "struct".to_string(),
					item_path: "Greeter".to_string(),
					signature: "pub struct Greeter;".to_string(),
					file_path: PathBuf::from("src/lib.rs"),
				},
			),
		]);
		let after = BTreeMap::from([(
			("function".to_string(), "greet".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "greet".to_string(),
				signature: "pub fn greet()".to_string(),
				file_path: PathBuf::from("src/lib.rs"),
			},
		)]);

		let changes = diff_public_symbols(&before, &after);

		assert_eq!(changes.len(), 1);
		let change = changes
			.first()
			.unwrap_or_else(|| panic!("expected one removed change"));
		assert_eq!(change.kind, SemanticChangeKind::Removed);
		assert!(change.summary.contains("removed"));
		assert_eq!(describe_manifest_value(&Value::Boolean(true)), "true");
	}
}
