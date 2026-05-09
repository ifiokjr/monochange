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
	assert!(
		export_changes.iter().any(|change| {
			change.item_path == "pkg" && change.kind == SemanticChangeKind::Removed
		})
	);

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
