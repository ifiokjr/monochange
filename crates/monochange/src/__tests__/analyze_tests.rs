#![allow(clippy::disallowed_methods)]
use std::path::Path;

use monochange_core::VersionFormat;
use monochange_test_helpers::copy_directory;
use monochange_test_helpers::fs::fixture_path_from;
use monochange_test_helpers::git::git;
use tempfile::TempDir;

use super::*;

fn setup_analyze_repo(tag_release: bool, with_origin_head: bool) -> TempDir {
	let scenario_root = fixture_path_from(
		env!("CARGO_MANIFEST_DIR"),
		"cli-output/analyze-group-release-trajectory",
	);
	let release = scenario_root.join("release");
	let main = scenario_root.join("main");
	let head = scenario_root.join("head");
	let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	copy_directory(&release, root);
	git(root, &["init"]);
	git(root, &["config", "user.name", "monochange-tests"]);
	git(
		root,
		&["config", "user.email", "monochange-tests@example.com"],
	);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "release"]);
	git(root, &["branch", "-M", "main"]);
	if tag_release {
		git(root, &["tag", "v1.0.0"]);
	}
	if with_origin_head {
		let remote_dir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		git(remote_dir.path(), &["init", "--bare"]);
		let remote = remote_dir.keep();
		git(
			root,
			&["remote", "add", "origin", remote.to_string_lossy().as_ref()],
		);
		git(root, &["push", "-u", "origin", "main"]);
		git(root, &["remote", "set-head", "origin", "main"]);
	}

	copy_directory(&main, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "main evolution"]);
	git(root, &["checkout", "-b", "feature"]);
	copy_directory(&head, root);
	git(root, &["add", "."]);
	git(root, &["commit", "-m", "head evolution"]);

	tempdir
}

#[test]
fn filter_change_analysis_keeps_only_selected_package() {
	let mut package_analyses = std::collections::BTreeMap::new();
	package_analyses.insert(
		"core".to_string(),
		monochange_analysis::PackageChangeAnalysis {
			package_id: "core".to_string(),
			package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
			package_name: "core".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			analyzer_id: Some("cargo/public-api".to_string()),
			changed_files: vec![PathBuf::from("src/lib.rs")],
			semantic_changes: Vec::new(),
			warnings: Vec::new(),
		},
	);
	package_analyses.insert(
		"app".to_string(),
		monochange_analysis::PackageChangeAnalysis {
			package_id: "app".to_string(),
			package_record_id: "cargo:crates/app/Cargo.toml".to_string(),
			package_name: "app".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			analyzer_id: Some("cargo/public-api".to_string()),
			changed_files: Vec::new(),
			semantic_changes: Vec::new(),
			warnings: Vec::new(),
		},
	);
	let filtered = filter_change_analysis(
		ChangeAnalysis {
			frame: ChangeFrame::WorkingDirectory,
			detection_level: DetectionLevel::Signature,
			package_analyses,
			warnings: vec!["warning".to_string()],
		},
		"core",
	);

	assert_eq!(filtered.package_analyses.len(), 1);
	assert!(filtered.package_analyses.contains_key("core"));
	assert_eq!(filtered.warnings, vec!["warning"]);
}

#[test]
fn tag_prefix_for_identity_matches_primary_and_namespaced_tags() {
	let namespaced = EffectiveReleaseIdentity {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		group_id: None,
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		members: vec!["core".to_string()],
	};
	let primary = EffectiveReleaseIdentity {
		owner_id: "sdk".to_string(),
		owner_kind: ReleaseOwnerKind::Group,
		group_id: Some("sdk".to_string()),
		tag: true,
		release: true,
		version_format: VersionFormat::Primary,
		members: vec!["core".to_string(), "app".to_string()],
	};

	assert_eq!(tag_prefix_for_identity(&namespaced), "core/v");
	assert_eq!(tag_prefix_for_identity(&primary), "v");
}

#[tokio::test(flavor = "multi_thread")]
async fn render_analyze_report_supports_json_with_explicit_refs() {
	let tempdir = setup_analyze_repo(true, true);
	let rendered = render_analyze_report(
		tempdir.path(),
		"core",
		Some("v1.0.0"),
		Some("main"),
		Some("HEAD"),
		"semantic",
		OutputFormat::Json,
	)
	.await
	.unwrap_or_else(|error| panic!("render analyze json: {error}"));

	assert!(rendered.contains("\"release\": \"v1.0.0\""));
	assert!(rendered.contains("\"firstRelease\": false"));
	assert!(rendered.contains("\"releaseToHead\""));
	assert!(rendered.contains("\"itemPath\": \"shout\""));
}

#[tokio::test(flavor = "multi_thread")]
async fn render_analyze_report_supports_first_release_fallback_and_text_warnings() {
	let tempdir = setup_analyze_repo(false, false);
	let rendered = render_analyze_report(
		tempdir.path(),
		"core",
		None,
		None,
		None,
		"signature",
		OutputFormat::Text,
	)
	.await
	.unwrap_or_else(|error| panic!("render analyze text: {error}"));

	assert!(rendered.contains("release: none"));
	assert!(rendered.contains("first release: yes"));
	assert!(rendered.contains("no prior release tag found for group `sdk`"));
	assert!(rendered.contains("main -> head:"));
}

#[tokio::test(flavor = "multi_thread")]
async fn parse_detection_level_and_default_branch_helpers_cover_error_paths() {
	let invalid_level = parse_detection_level("deep").unwrap_err().render();
	assert!(invalid_level.contains("unknown detection level"));

	let missing_repo = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let default_branch_error = default_branch_name(missing_repo.path())
		.await
		.unwrap_err()
		.render();
	assert!(default_branch_error.contains("could not determine default branch"));
}

#[test]
fn parse_origin_head_branch_covers_attached_and_unset_values() {
	assert_eq!(
		parse_origin_head_branch("origin/main"),
		Some("main".to_string())
	);
	assert_eq!(
		parse_origin_head_branch(" origin/master\n"),
		Some("master".to_string())
	);
	assert_eq!(parse_origin_head_branch("origin/HEAD"), None);
	assert_eq!(parse_origin_head_branch(""), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn default_branch_and_release_tag_resolution_cover_origin_head_and_missing_identity_paths() {
	let tempdir = setup_analyze_repo(true, true);
	assert_eq!(
		default_branch_name(tempdir.path())
			.await
			.unwrap_or_else(|error| panic!("default branch: {error}")),
		"main"
	);
	assert_eq!(
		latest_release_tag_for_identity(tempdir.path(), None)
			.await
			.unwrap_or_else(|error| panic!("no identity: {error}")),
		None
	);

	let no_tag_identity = EffectiveReleaseIdentity {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		group_id: None,
		tag: false,
		release: false,
		version_format: VersionFormat::Namespaced,
		members: vec!["core".to_string()],
	};
	assert_eq!(
		latest_release_tag_for_identity(tempdir.path(), Some(&no_tag_identity))
			.await
			.unwrap_or_else(|error| panic!("no tag identity: {error}")),
		None
	);
}

#[test]
fn release_owner_label_covers_package_and_group_variants() {
	assert_eq!(release_owner_label(ReleaseOwnerKind::Package), "package");
	assert_eq!(release_owner_label(ReleaseOwnerKind::Group), "group");
}

#[test]
fn render_frame_section_covers_change_and_warning_paths() {
	let mut lines = Vec::new();
	let mut package_analyses = std::collections::BTreeMap::new();
	package_analyses.insert(
		"core".to_string(),
		monochange_analysis::PackageChangeAnalysis {
			package_id: "core".to_string(),
			package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
			package_name: "core".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			analyzer_id: Some("cargo/public-api".to_string()),
			changed_files: vec![Path::new("src/lib.rs").to_path_buf()],
			semantic_changes: vec![monochange_analysis::SemanticChange {
				category: monochange_analysis::SemanticChangeCategory::PublicApi,
				kind: monochange_analysis::SemanticChangeKind::Added,
				item_kind: "function".to_string(),
				item_path: "shout".to_string(),
				summary: "function `shout` added".to_string(),
				file_path: Path::new("src/lib.rs").to_path_buf(),
				before_signature: None,
				after_signature: Some("fn shout (name : & str) -> String".to_string()),
			}],
			warnings: vec!["package warning".to_string()],
		},
	);
	render_frame_section(
		&mut lines,
		"main -> head",
		&ChangeAnalysis {
			frame: ChangeFrame::CustomRange {
				base: "main".to_string(),
				head: "HEAD".to_string(),
			},
			detection_level: DetectionLevel::Signature,
			package_analyses,
			warnings: vec!["frame warning".to_string()],
		},
		"core",
	);
	let rendered = lines.join("\n");

	assert!(rendered.contains("changed files:"));
	assert!(rendered.contains("function `shout` added"));
	assert!(rendered.contains("package warning"));
	assert!(rendered.contains("frame warning"));
}

#[tokio::test(flavor = "multi_thread")]
async fn latest_release_tag_and_text_rendering_cover_warning_branches() {
	let missing_repo = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let identity = EffectiveReleaseIdentity {
		owner_id: "sdk".to_string(),
		owner_kind: ReleaseOwnerKind::Group,
		group_id: Some("sdk".to_string()),
		tag: true,
		release: true,
		version_format: VersionFormat::Primary,
		members: vec!["core".to_string()],
	};
	let tag_error = latest_release_tag_for_identity(missing_repo.path(), Some(&identity))
		.await
		.unwrap_err()
		.render();
	assert!(tag_error.contains("failed to list git tags"));

	let rendered = render_text_report(&AnalyzeReport {
		package: AnalyzePackageSelection {
			requested_reference: "core".to_string(),
			package_id: "core".to_string(),
			package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
			package_name: "core".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: Path::new("crates/core/Cargo.toml").to_path_buf(),
			version_group_id: None,
		},
		release_identity: None,
		first_release: true,
		refs: AnalyzeRefs {
			release: None,
			main: "main".to_string(),
			head: "HEAD".to_string(),
		},
		frames: AnalyzeFrames {
			release_to_main: None,
			main_to_head: ChangeAnalysis {
				frame: ChangeFrame::CustomRange {
					base: "main".to_string(),
					head: "HEAD".to_string(),
				},
				detection_level: DetectionLevel::Signature,
				package_analyses: std::collections::BTreeMap::new(),
				warnings: vec!["frame warning".to_string()],
			},
			release_to_head: None,
		},
		warnings: vec![first_release_warning("core", "main", "HEAD", None)],
	});

	assert!(rendered.contains("has no configured release identity"));
	assert!(rendered.contains("no semantic changes detected for `core` in this frame"));
	assert!(rendered.contains("frame warning"));
}

#[tokio::test(flavor = "multi_thread")]
async fn render_analyze_report_propagates_workspace_errors() {
	let missing_workspace = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = render_analyze_report(
		missing_workspace.path(),
		"core",
		None,
		Some("main"),
		Some("HEAD"),
		"semantic",
		OutputFormat::Markdown,
	)
	.await
	.unwrap_err()
	.render();

	assert!(!error.is_empty());
}

#[test]
fn render_frame_section_covers_empty_lists_and_missing_warning_paths() {
	let mut lines = Vec::new();
	render_frame_section(
		&mut lines,
		"release -> main",
		&ChangeAnalysis {
			frame: ChangeFrame::CustomRange {
				base: "v1.0.0".to_string(),
				head: "main".to_string(),
			},
			detection_level: DetectionLevel::Signature,
			package_analyses: std::collections::BTreeMap::new(),
			warnings: Vec::new(),
		},
		"core",
	);
	let missing_package_rendered = lines.join("\n");
	assert!(
		missing_package_rendered.contains("no semantic changes detected for `core` in this frame")
	);
	assert!(!missing_package_rendered.contains("  warnings:"));

	lines.clear();
	let mut package_analyses = std::collections::BTreeMap::new();
	package_analyses.insert(
		"core".to_string(),
		monochange_analysis::PackageChangeAnalysis {
			package_id: "core".to_string(),
			package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
			package_name: "core".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			analyzer_id: Some("cargo/public-api".to_string()),
			changed_files: Vec::new(),
			semantic_changes: Vec::new(),
			warnings: Vec::new(),
		},
	);
	render_frame_section(
		&mut lines,
		"main -> head",
		&ChangeAnalysis {
			frame: ChangeFrame::CustomRange {
				base: "main".to_string(),
				head: "HEAD".to_string(),
			},
			detection_level: DetectionLevel::Signature,
			package_analyses,
			warnings: Vec::new(),
		},
		"core",
	);
	let empty_lists_rendered = lines.join("\n");

	assert!(empty_lists_rendered.contains("semantic changes: 0"));
	assert!(!empty_lists_rendered.contains("  changed files:"));
	assert!(!empty_lists_rendered.contains("  changes:"));
	assert!(!empty_lists_rendered.contains("  warnings:"));
}
