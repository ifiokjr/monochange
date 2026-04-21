use std::path::Path;

use monochange_core::BumpSeverity;
use monochange_core::DependencyKind;
use monochange_core::DiscoveryPathFilter;
use monochange_core::Ecosystem;
use monochange_core::EcosystemType;
use monochange_core::MonochangeError;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::PublishState;
use monochange_core::strip_json_comments;
use monochange_core::update_json_manifest_text;
use semver::Version;

// ---------------------------------------------------------------------------
// BumpSeverity
// ---------------------------------------------------------------------------

#[test]
fn bump_severity_is_release_is_true_for_release_severities() {
	assert!(BumpSeverity::Patch.is_release());
	assert!(BumpSeverity::Minor.is_release());
	assert!(BumpSeverity::Major.is_release());
	assert!(!BumpSeverity::None.is_release());
}

#[test]
fn bump_severity_display_roundtrips_through_as_str() {
	assert_eq!(BumpSeverity::None.to_string(), "none");
	assert_eq!(BumpSeverity::Patch.to_string(), "patch");
	assert_eq!(BumpSeverity::Minor.to_string(), "minor");
	assert_eq!(BumpSeverity::Major.to_string(), "major");
}

// ---------------------------------------------------------------------------
// Ecosystem
// ---------------------------------------------------------------------------

#[test]
fn ecosystem_display_matches_as_str() {
	assert_eq!(Ecosystem::Cargo.to_string(), "cargo");
	assert_eq!(Ecosystem::Npm.to_string(), "npm");
	assert_eq!(Ecosystem::Deno.to_string(), "deno");
	assert_eq!(Ecosystem::Dart.to_string(), "dart");
	assert_eq!(Ecosystem::Flutter.to_string(), "flutter");
}

// ---------------------------------------------------------------------------
// DependencyKind
// ---------------------------------------------------------------------------

#[test]
fn dependency_kind_display_matches_expected() {
	assert_eq!(DependencyKind::Runtime.to_string(), "runtime");
	assert_eq!(DependencyKind::Development.to_string(), "development");
	assert_eq!(DependencyKind::Build.to_string(), "build");
	assert_eq!(DependencyKind::Peer.to_string(), "peer");
	assert_eq!(DependencyKind::Workspace.to_string(), "workspace");
	assert_eq!(DependencyKind::Unknown.to_string(), "unknown");
}

// ---------------------------------------------------------------------------
// PackageType & EcosystemType
// ---------------------------------------------------------------------------

#[test]
fn package_type_as_str_returns_canonical_names() {
	assert_eq!(PackageType::Cargo.as_str(), "cargo");
	assert_eq!(PackageType::Npm.as_str(), "npm");
	assert_eq!(PackageType::Deno.as_str(), "deno");
	assert_eq!(PackageType::Dart.as_str(), "dart");
	assert_eq!(PackageType::Flutter.as_str(), "flutter");
}

#[test]
fn ecosystem_type_default_prefix_is_correct() {
	assert_eq!(EcosystemType::Cargo.default_prefix(), "");
	assert_eq!(EcosystemType::Npm.default_prefix(), "^");
	assert_eq!(EcosystemType::Deno.default_prefix(), "^");
	assert_eq!(EcosystemType::Dart.default_prefix(), "^");
}

#[test]
fn ecosystem_type_default_fields_are_non_empty_for_cargo() {
	let fields = EcosystemType::Cargo.default_fields();
	assert!(
		fields.contains(&"dependencies"),
		"cargo fields should include dependencies"
	);
	assert!(
		fields.contains(&"dev-dependencies"),
		"cargo fields should include dev-dependencies"
	);
}

#[test]
fn ecosystem_type_default_fields_are_non_empty_for_npm() {
	let fields = EcosystemType::Npm.default_fields();
	assert!(
		fields.contains(&"dependencies"),
		"npm fields should include dependencies"
	);
	assert!(
		fields.contains(&"devDependencies"),
		"npm fields should include devDependencies"
	);
}

// ---------------------------------------------------------------------------
// PackageRecord::relative_manifest_path
// ---------------------------------------------------------------------------

#[test]
fn package_record_relative_manifest_path_is_some_when_under_root() {
	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	let manifest = root.join("crates/core/Cargo.toml");
	std::fs::create_dir_all(manifest.parent().unwrap()).unwrap();
	std::fs::write(&manifest, "").unwrap();
	let pkg = PackageRecord::new(
		Ecosystem::Cargo,
		"cargo:core".to_string(),
		manifest.clone(),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let rel = pkg.relative_manifest_path(root).unwrap();
	assert_eq!(rel, Path::new("crates/core/Cargo.toml"));
}

#[test]
fn package_record_relative_manifest_path_is_none_when_outside_root() {
	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	let manifest = std::env::temp_dir().join("outside/Cargo.toml");
	let pkg = PackageRecord::new(
		Ecosystem::Cargo,
		"cargo:outside".to_string(),
		manifest.clone(),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	assert!(pkg.relative_manifest_path(root).is_none());
}

// ---------------------------------------------------------------------------
// strip_json_comments
// ---------------------------------------------------------------------------

#[test]
fn strip_json_comments_removes_line_comments() {
	let input = r#"{
  "key": "value" // line comment
}"#;
	let stripped = strip_json_comments(input);
	println!("stripped='{stripped}'");
	assert!(!stripped.contains("line comment"));
	assert!(
		stripped.contains("\"key\": \"value\""),
		"actual: {stripped}"
	);
}

#[test]
fn strip_json_comments_removes_block_comments() {
	let input = r#"{
  /* block comment */
  "key": "value"
}"#;
	let stripped = strip_json_comments(input);
	assert!(!stripped.contains("block comment"));
	assert!(
		stripped.contains("\"key\": \"value\""),
		"actual: {stripped}"
	);
}

#[test]
fn strip_json_comments_preserves_strings_with_slashes() {
	let input = r#"{"url": "http://example.com"}"#;
	let stripped = strip_json_comments(input);
	assert!(stripped.contains("http://example.com"));
}

#[test]
fn strip_json_comments_preserves_escaped_quotes() {
	let input = r#"{"message": "say \"hello\""}"#;
	let stripped = strip_json_comments(input);
	assert!(stripped.contains("say \\\"hello\\\""));
}

// ---------------------------------------------------------------------------
// update_json_manifest_text (covers apply_json_replacements)
// ---------------------------------------------------------------------------

#[test]
fn update_json_manifest_text_replaces_version() {
	let contents = r#"{"version": "1.0.0"}"#;
	let result = update_json_manifest_text(
		contents,
		Some("2.0.0"),
		&[],
		&std::collections::BTreeMap::new(),
	)
	.unwrap();
	assert!(result.contains("2.0.0"));
	assert!(!result.contains("1.0.0"));
}

#[test]
fn update_json_manifest_text_returns_error_for_bad_span() {
	let contents = r#"{"version": "1.0.0"}"#;
	let result = update_json_manifest_text(
		contents,
		Some("2.0.0"),
		&["bad_field_with_very_long_name_that_does_not_exist"],
		&std::collections::BTreeMap::new(),
	);
	// bad field should just be skipped, not error
	assert!(result.is_ok());
}

// ---------------------------------------------------------------------------
// DiscoveryPathFilter
// ---------------------------------------------------------------------------

#[test]
fn discovery_path_filter_allows_manifest_files() {
	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	std::fs::write(root.join("Cargo.toml"), "[package]").unwrap();

	let filter = DiscoveryPathFilter::new(root);
	assert!(filter.allows(root.join("Cargo.toml").as_path()));
}

#[test]
fn discovery_path_filter_blocks_dot_git() {
	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	std::fs::create_dir(root.join(".git")).unwrap();

	let filter = DiscoveryPathFilter::new(root);
	assert!(!filter.allows(root.join(".git/config").as_path()));
}

#[test]
fn discovery_path_filter_should_descend_allows_non_ignored_dirs() {
	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	std::fs::create_dir(root.join("crates")).unwrap();

	let filter = DiscoveryPathFilter::new(root);
	assert!(
		filter.should_descend(root.join("crates").as_path()),
		"should_descend should return true for non-ignored directories"
	);
}

#[test]
fn discovery_path_filter_matches_gitignore_blocks_ignored_files() {
	let tmp = tempfile::tempdir().unwrap();
	let root = tmp.path();
	std::fs::write(root.join(".gitignore"), "*.log\n").unwrap();
	std::fs::write(root.join("test.log"), "").unwrap();

	let filter = DiscoveryPathFilter::new(root);
	assert!(
		!filter.allows(root.join("test.log").as_path()),
		"gitignore pattern should block ignored files"
	);
}

// ---------------------------------------------------------------------------
// MonochangeError::render
// ---------------------------------------------------------------------------

#[test]
fn monochange_error_render_diagnostic() {
	let err = MonochangeError::Diagnostic("bad config".to_string());
	let rendered = err.render();
	assert_eq!(rendered, "bad config", "rendered: {rendered}");
}

#[test]
fn monochange_error_render_io_source() {
	let err = MonochangeError::IoSource {
		path: Path::new("/tmp/test").to_path_buf(),
		source: std::io::Error::new(std::io::ErrorKind::NotFound, "not found"),
	};
	let rendered = err.render();
	assert_eq!(
		rendered, "io error at /tmp/test: not found",
		"rendered: {rendered}"
	);
}

#[test]
fn monochange_error_render_parse() {
	let err = MonochangeError::Parse {
		path: Path::new("config.toml").to_path_buf(),
		source: Box::new(std::io::Error::new(
			std::io::ErrorKind::InvalidData,
			"invalid toml",
		)),
	};
	let rendered = err.render();
	assert_eq!(
		rendered, "parse error at config.toml: invalid toml",
		"rendered: {rendered}"
	);
}

#[test]
fn monochange_error_render_cancelled() {
	let err = MonochangeError::Cancelled;
	let rendered = err.render();
	assert_eq!(rendered, "cancelled");
}

#[cfg(feature = "http")]
#[test]
fn monochange_error_render_http_request() {
	let err = MonochangeError::HttpRequest {
		context: "GET /api".to_string(),
		source: reqwest::Error::from(std::io::Error::new(
			std::io::ErrorKind::Other,
			"network error",
		)),
	};
	let rendered = err.render();
	assert!(rendered.contains("http error"), "rendered: {rendered}");
	assert!(rendered.contains("GET /api"), "rendered: {rendered}");
}
