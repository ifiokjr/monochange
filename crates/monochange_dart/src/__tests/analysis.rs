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

#[test]
fn snapshot_and_manifest_helpers_cover_additional_dart_branches() {
	let changed_files = vec![
		AnalyzedFileChange {
			path: PathBuf::from("packages/mobile/lib/mobile_app.dart"),
			package_path: PathBuf::from("lib/mobile_app.dart"),
			kind: FileChangeKind::Modified,
			before_contents: Some("String previous() => 'old';".to_string()),
			after_contents: None,
		},
		AnalyzedFileChange {
			path: PathBuf::from("packages/mobile/lib/src/widgets.dart"),
			package_path: PathBuf::from("lib/src/widgets.dart"),
			kind: FileChangeKind::Modified,
			before_contents: None,
			after_contents: Some(
				concat!(
					"// comment\n",
					"export 'more.dart';\n",
					"class Greeter {}\n",
					"String greet(String name) => name;\n",
					"if (true) {}\n",
				)
				.to_string(),
			),
		},
	];

	let symbols = snapshot_public_symbols(None, &changed_files);
	for expected in [
		"mobile_app::previous",
		"src::widgets::Greeter",
		"src::widgets::greet",
		"more.dart",
	] {
		assert!(symbols.values().any(|symbol| symbol.item_path == expected));
	}
	assert!(is_public_dart_source_file(Path::new("lib/main.dart")));
	assert!(!is_public_dart_source_file(Path::new(
		"lib/src/generated/file.dart"
	)));
	assert_eq!(trim_inline_comment("value // note"), "value ");
	assert_eq!(update_brace_depth(0, "class Greeter {"), 1);
	assert_eq!(
		find_keyword_name("sealed class Greeter {}", "class"),
		Some("Greeter".to_string())
	);
	assert_eq!(
		module_prefix_for_file(Path::new("lib/src/widgets.dart")),
		vec!["src".to_string(), "widgets".to_string()]
	);

	let mut warnings = Vec::new();
	assert!(parse_manifest(Some("name: ["), Path::new("pubspec.yaml"), &mut warnings).is_none());
	assert_eq!(warnings.len(), 1);

	let before = serde_yaml_ng::from_str::<Mapping>(concat!(
		"publish_to: none\n",
		"environment:\n  sdk: ^3.4.0\n",
		"executables:\n  mobile-app:\n  1: ignored\n",
		"dependencies:\n  flutter:\n    sdk: flutter\n  1: ignored\n",
		"flutter:\n  plugin:\n    platforms:\n      android:\n        package: com.example.mobile\n",
	))
	.unwrap_or_else(|error| panic!("parse before yaml: {error}"));
	let after = serde_yaml_ng::from_str::<Mapping>(concat!(
		"publish_to: hosted\n",
		"environment:\n  sdk: ^3.5.0\n",
		"executables:\n  mobile-admin:\n",
		"dependencies:\n  riverpod: ^2.5.0\n",
		"flutter:\n  plugin:\n    platforms:\n      ios:\n        pluginClass: MobilePlugin\n",
	))
	.unwrap_or_else(|error| panic!("parse after yaml: {error}"));

	let export_changes = compare_manifest_entries(
		SemanticChangeCategory::Export,
		Path::new("pubspec.yaml"),
		&extract_export_entries(&before),
		&extract_export_entries(&after),
	);
	assert!(export_changes.iter().any(|change| {
		change.item_path == "mobile-app" && change.kind == SemanticChangeKind::Removed
	}));
	assert!(export_changes.iter().any(|change| {
		change.item_path == "mobile-admin" && change.kind == SemanticChangeKind::Added
	}));

	let metadata_changes = compare_manifest_entries(
		SemanticChangeCategory::Metadata,
		Path::new("pubspec.yaml"),
		&extract_metadata_entries(&before),
		&extract_metadata_entries(&after),
	);
	assert!(metadata_changes.iter().any(|change| {
		change.item_path == "publish_to" && change.kind == SemanticChangeKind::Modified
	}));
	assert!(metadata_changes.iter().any(|change| {
		change.item_path == "flutter.plugin.platform.android"
			&& change.kind == SemanticChangeKind::Removed
	}));
	assert!(metadata_changes.iter().any(|change| {
		change.item_path == "flutter.plugin.platform.ios"
			&& change.kind == SemanticChangeKind::Added
	}));
	assert!(
		describe_yaml_value(
			&serde_yaml_ng::from_str::<Value>("!tag value")
				.unwrap_or_else(|error| panic!("parse tagged value: {error}"))
		)
		.contains("value")
	);
}

#[test]
fn parser_diff_and_metadata_helpers_cover_remaining_dart_branches() {
	let skipped_symbols = snapshot_public_symbols(
		None,
		&[
			AnalyzedFileChange {
				path: PathBuf::from("packages/mobile/lib/empty.dart"),
				package_path: PathBuf::from("lib/empty.dart"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: None,
			},
			AnalyzedFileChange {
				path: PathBuf::from("packages/mobile/test/widget_test.dart"),
				package_path: PathBuf::from("test/widget_test.dart"),
				kind: FileChangeKind::Modified,
				before_contents: None,
				after_contents: Some("String ignored() => 'nope';".to_string()),
			},
		],
	);
	assert!(skipped_symbols.is_empty());

	let direct_symbols = collect_public_symbols(&PackageSnapshotFile {
		path: PathBuf::from("lib/api.dart"),
		contents: concat!(
			"export 'shared.dart';\n",
			"class Greeter {\n",
			"  String member() => 'hi';\n",
			"}\n",
			"String greet(String name) => name;\n",
		)
		.to_string(),
	});
	assert!(
		direct_symbols
			.iter()
			.any(|symbol| symbol.item_path == "shared.dart")
	);
	assert!(
		direct_symbols
			.iter()
			.any(|symbol| symbol.item_path == "api::Greeter")
	);
	assert!(
		direct_symbols
			.iter()
			.any(|symbol| symbol.item_path == "api::greet")
	);
	assert_eq!(parse_top_level_function("final callback = ("), None);

	let before = BTreeMap::from([
		(
			("function".to_string(), "greet".to_string()),
			PublicSymbol {
				item_kind: "function".to_string(),
				item_path: "greet".to_string(),
				signature: "String greet()".to_string(),
				file_path: PathBuf::from("lib/api.dart"),
			},
		),
		(
			("class".to_string(), "Greeter".to_string()),
			PublicSymbol {
				item_kind: "class".to_string(),
				item_path: "Greeter".to_string(),
				signature: "class Greeter {}".to_string(),
				file_path: PathBuf::from("lib/api.dart"),
			},
		),
	]);
	let after = BTreeMap::from([(
		("function".to_string(), "greet".to_string()),
		PublicSymbol {
			item_kind: "function".to_string(),
			item_path: "greet".to_string(),
			signature: "String greet()".to_string(),
			file_path: PathBuf::from("lib/api.dart"),
		},
	)]);
	let changes = diff_public_symbols(&before, &after);
	assert_eq!(changes.len(), 1);
	let change = changes
		.first()
		.unwrap_or_else(|| panic!("expected one removed change"));
	assert_eq!(change.kind, SemanticChangeKind::Removed);
	assert!(change.summary.contains("removed"));

	let manifest = serde_yaml_ng::from_str::<Mapping>(concat!(
		"environment:\n",
		"  sdk: ^3.5.0\n",
		"  flutter: '>=3.22.0'\n",
		"dependencies:\n",
		"  flutter:\n",
		"    sdk: flutter\n",
		"  1: ignored\n",
		"flutter:\n",
		"  plugin:\n",
		"    platforms:\n",
		"      ios:\n",
		"        pluginClass: MobilePlugin\n",
		"      1:\n",
		"        ignored: true\n",
	))
	.unwrap_or_else(|error| panic!("parse helper manifest: {error}"));
	let dependency_entries = extract_dependency_entries(&manifest);
	assert!(dependency_entries.contains_key("flutter"));
	let metadata_entries = extract_metadata_entries(&manifest);
	assert!(metadata_entries.contains_key("environment.sdk"));
	assert!(metadata_entries.contains_key("environment.flutter"));
	assert!(metadata_entries.contains_key("flutter.plugin.platform.ios"));

	assert_eq!(
		describe_yaml_value(
			&serde_yaml_ng::from_str::<Value>("true")
				.unwrap_or_else(|error| panic!("parse bool yaml value: {error}"))
		),
		"true"
	);
	assert_eq!(
		describe_yaml_value(
			&serde_yaml_ng::from_str::<Value>("7")
				.unwrap_or_else(|error| panic!("parse number yaml value: {error}"))
		),
		"7"
	);
	assert_eq!(
		describe_yaml_value(
			&serde_yaml_ng::from_str::<Value>("text")
				.unwrap_or_else(|error| panic!("parse string yaml value: {error}"))
		),
		"text"
	);
	assert_eq!(
		describe_yaml_value(
			&serde_yaml_ng::from_str::<Value>("[a, b]")
				.unwrap_or_else(|error| panic!("parse sequence yaml value: {error}"))
		),
		"a, b"
	);
}
