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

	let (_, warnings) = snapshot_public_symbols(Some(&snapshot), &[], DetectionLevel::Signature);

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
	assert!(parse_manifest(Some("not = [valid"), Path::new("Cargo.toml"), &mut warnings).is_none());
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
