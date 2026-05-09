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
