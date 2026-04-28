use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;

use crate::GoAdapter;
use crate::GoVersionedFileKind;
use crate::adapter;
use crate::derive_module_name;
use crate::discover_go_modules;
use crate::is_major_version_suffix;
use crate::parse_go_version;
use crate::parse_module_path;
use crate::parse_require_directives;
use crate::update_go_mod_text;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

// -- adapter --

#[test]
fn adapter_reports_go_ecosystem() {
	let go_adapter = GoAdapter;
	assert_eq!(go_adapter.ecosystem(), Ecosystem::Go);
	assert_eq!(adapter().ecosystem(), Ecosystem::Go);
}

#[test]
fn discover_go_modules_reports_warnings_for_unreadable_go_mod_files() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	#[cfg(unix)]
	std::os::unix::fs::symlink(root.join("missing-go-mod"), root.join("go.mod"))
		.unwrap_or_else(|error| panic!("symlink go.mod: {error}"));
	#[cfg(not(unix))]
	std::fs::write(root.join("go.mod"), [0xff, 0xfe])
		.unwrap_or_else(|error| panic!("write invalid go.mod: {error}"));

	let discovery =
		discover_go_modules(root).unwrap_or_else(|error| panic!("go discovery: {error}"));

	assert!(discovery.packages.is_empty());
	assert_eq!(discovery.warnings.len(), 1);
	assert!(
		discovery
			.warnings
			.first()
			.expect("warning")
			.contains("skipped")
	);
}

#[test]
fn adapter_discover_delegates_to_discover_go_modules() {
	let root = fixture_path("go/single-module");
	let discovery = GoAdapter
		.discover(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(discovery.packages.first().unwrap().name, "myapp");
}

// -- supported_versioned_file_kind --

#[test]
fn supported_versioned_file_kind_recognizes_go_files() {
	use crate::supported_versioned_file_kind;
	assert_eq!(
		supported_versioned_file_kind("go.mod".as_ref()),
		Some(GoVersionedFileKind::GoMod)
	);
	assert_eq!(
		supported_versioned_file_kind("go.sum".as_ref()),
		Some(GoVersionedFileKind::GoSum)
	);
	assert_eq!(supported_versioned_file_kind("Cargo.toml".as_ref()), None);
	assert_eq!(supported_versioned_file_kind("package.json".as_ref()), None);
}

// -- parse_module_path --

#[test]
fn parse_module_path_extracts_trimmed_module_directive() {
	let contents = "module   github.com/org/repo  \n\ngo 1.22\n";
	assert_eq!(
		parse_module_path(contents),
		Some("github.com/org/repo".to_string())
	);
}

#[test]
fn parse_module_path_extracts_module_directive() {
	let contents = "module github.com/org/repo\n\ngo 1.22\n";
	assert_eq!(
		parse_module_path(contents),
		Some("github.com/org/repo".to_string())
	);
}

#[test]
fn parse_module_path_handles_submodule_paths() {
	let contents = "module github.com/org/repo/api/v2\n\ngo 1.22\n";
	assert_eq!(
		parse_module_path(contents),
		Some("github.com/org/repo/api/v2".to_string())
	);
}

#[test]
fn parse_module_path_returns_none_without_module_directive() {
	let contents = "go 1.22\nrequire golang.org/x/text v0.14.0\n";
	assert_eq!(parse_module_path(contents), None);
}

#[test]
fn parse_module_path_skips_empty_module_directive() {
	let contents = "module \n\ngo 1.22\n";
	assert_eq!(parse_module_path(contents), None);
}

// -- derive_module_name --

#[test]
fn derive_module_name_extracts_last_segment() {
	assert_eq!(derive_module_name("github.com/org/repo"), "repo");
	assert_eq!(derive_module_name("github.com/org/repo/api"), "api");
	assert_eq!(
		derive_module_name("github.com/org/repo/internal/worker"),
		"worker"
	);
}

#[test]
fn derive_module_name_strips_major_version_suffix() {
	assert_eq!(derive_module_name("github.com/org/repo/api/v2"), "api");
	assert_eq!(derive_module_name("github.com/org/sdk/v3"), "sdk");
}

#[test]
fn derive_module_name_handles_single_segment() {
	assert_eq!(derive_module_name("mymodule"), "mymodule");
}

// -- is_major_version_suffix --

#[test]
fn is_major_version_suffix_identifies_version_segments() {
	assert!(is_major_version_suffix("v2"));
	assert!(is_major_version_suffix("v3"));
	assert!(is_major_version_suffix("v10"));
	assert!(!is_major_version_suffix("v0"));
	assert!(!is_major_version_suffix("v"));
	assert!(!is_major_version_suffix("api"));
	assert!(!is_major_version_suffix("v1.2.3"));
	assert!(!is_major_version_suffix("version"));
}

// -- parse_go_version --

#[test]
fn parse_go_version_handles_standard_and_prefixed_versions() {
	assert_eq!(parse_go_version("v1.2.3"), Some(Version::new(1, 2, 3)));
	assert_eq!(parse_go_version("1.2.3"), Some(Version::new(1, 2, 3)));
	assert_eq!(parse_go_version("v0.1.0"), Some(Version::new(0, 1, 0)));
	assert_eq!(parse_go_version("not-a-version"), None);
	assert_eq!(parse_go_version("v1.2"), None);
}

// -- parse_require_directives --

#[test]
fn parse_require_directives_extracts_block_and_single_line_deps() {
	let contents = r"module github.com/example/app

go 1.22

require (
	github.com/gin-gonic/gin v1.9.1
	golang.org/x/sys v0.15.0 // indirect
)

require github.com/nats-io/nats.go v1.31.0
";
	let deps = parse_require_directives(contents);
	assert_eq!(deps.len(), 3);

	let gin = deps.iter().find(|d| d.name == "gin").unwrap();
	assert_eq!(gin.kind, DependencyKind::Runtime);
	assert_eq!(gin.version_constraint.as_deref(), Some("1.9.1"));

	let sys = deps.iter().find(|d| d.name == "sys").unwrap();
	assert_eq!(sys.kind, DependencyKind::Development);
	assert_eq!(sys.version_constraint.as_deref(), Some("0.15.0"));

	let nats = deps.iter().find(|d| d.name == "nats.go").unwrap();
	assert_eq!(nats.kind, DependencyKind::Runtime);
	assert_eq!(nats.version_constraint.as_deref(), Some("1.31.0"));
}

#[test]
fn parse_require_directives_handles_empty_file() {
	let deps = parse_require_directives("module github.com/example/app\n\ngo 1.22\n");
	assert!(deps.is_empty());
}

#[test]
fn parse_require_directives_skips_replace_and_exclude() {
	let contents = r"module github.com/example/app

go 1.22

require github.com/example/shared v1.0.0

replace github.com/example/shared => ../shared

exclude github.com/example/old v0.1.0
";
	let deps = parse_require_directives(contents);
	assert_eq!(deps.len(), 1);
	assert_eq!(deps.first().unwrap().name, "shared");
}

// -- discover_go_modules --

#[test]
fn discover_go_modules_finds_single_module() {
	let root = fixture_path("go/single-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "myapp");
	assert_eq!(pkg.ecosystem, Ecosystem::Go);
	assert_eq!(
		pkg.metadata.get("module_path").map(String::as_str),
		Some("github.com/example/myapp")
	);
	// Go versions come from git tags, not go.mod
	assert_eq!(pkg.current_version, None);
}

#[test]
fn discover_go_modules_finds_multi_module_monorepo() {
	let root = fixture_path("go/multi-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 3);
	let names: Vec<&str> = discovery.packages.iter().map(|p| p.name.as_str()).collect();
	assert!(names.contains(&"api"), "missing api: {names:?}");
	assert!(names.contains(&"shared"), "missing shared: {names:?}");
	assert!(names.contains(&"worker"), "missing worker: {names:?}");
}

#[test]
fn discover_go_modules_stores_relative_path_metadata() {
	let root = fixture_path("go/multi-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let api = discovery.packages.iter().find(|p| p.name == "api").unwrap();
	assert_eq!(
		api.metadata.get("relative_path").map(String::as_str),
		Some("api")
	);
}

#[test]
fn discover_go_modules_extracts_cross_module_dependencies() {
	let root = fixture_path("go/multi-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let api = discovery.packages.iter().find(|p| p.name == "api").unwrap();
	let dep_names: Vec<&str> = api
		.declared_dependencies
		.iter()
		.map(|d| d.name.as_str())
		.collect();
	assert!(
		dep_names.contains(&"shared"),
		"api should depend on shared: {dep_names:?}"
	);
	assert!(
		dep_names.contains(&"gin"),
		"api should depend on gin: {dep_names:?}"
	);
}

#[test]
fn discover_go_modules_handles_major_version_suffix() {
	let root = fixture_path("go/major-version");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "sdk", "should strip /v2 suffix from name");
	assert_eq!(
		pkg.metadata.get("module_path").map(String::as_str),
		Some("github.com/example/sdk/v2")
	);
}

#[test]
fn discover_go_modules_skips_files_without_module_directive() {
	let root = fixture_path("go/no-module-directive");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert!(discovery.packages.is_empty());
}

#[test]
fn discover_go_modules_warns_on_unparseable_files() {
	let root = fixture_path("go/invalid-gomod");
	let discovery =
		discover_go_modules(&root).unwrap_or_else(|error| panic!("unexpected error: {error}"));
	// Invalid go.mod without a module directive produces no packages (not an error)
	assert!(discovery.packages.is_empty());
}

#[test]
fn discover_go_modules_extracts_indirect_dependencies() {
	let root = fixture_path("go/single-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let pkg = discovery.packages.first().unwrap();
	let indirect_deps: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(
		indirect_deps.contains(&"sys"),
		"should mark indirect deps: {indirect_deps:?}"
	);
}

#[test]
fn discover_go_modules_handles_versioned_module_dependencies() {
	let root = fixture_path("go/single-module");
	let discovery = discover_go_modules(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let pkg = discovery.packages.first().unwrap();
	let redis_dep = pkg
		.declared_dependencies
		.iter()
		.find(|d| d.name == "redis")
		.unwrap();
	assert_eq!(redis_dep.version_constraint.as_deref(), Some("8.11.5"));
}

// -- discover_lockfiles --

#[test]
fn discover_lockfiles_finds_go_sum() {
	let root = fixture_path("go/single-module");
	let package = PackageRecord::new(
		Ecosystem::Go,
		"myapp",
		root.join("go.mod"),
		root.clone(),
		None,
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert!(lockfiles.first().unwrap().ends_with("go.sum"));
}

#[test]
fn discover_lockfiles_returns_empty_without_go_sum() {
	let root = fixture_path("go/no-module-directive");
	let package = PackageRecord::new(
		Ecosystem::Go,
		"test",
		root.join("go.mod"),
		root.clone(),
		None,
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert!(lockfiles.is_empty());
}

// -- default_lockfile_commands --

#[test]
fn default_lockfile_commands_infers_go_mod_tidy() {
	let root = fixture_path("go/single-module");
	let package = PackageRecord::new(
		Ecosystem::Go,
		"myapp",
		root.join("go.mod"),
		root.clone(),
		None,
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert_eq!(commands.first().unwrap().command, "go mod tidy");
}

// -- update_go_mod_text --

#[test]
fn update_go_mod_text_updates_require_versions() {
	let input = r"module github.com/example/monorepo/api

go 1.22

require (
	github.com/example/monorepo/shared v1.2.0
	github.com/gin-gonic/gin v1.9.1
)
";
	let deps = BTreeMap::from([("shared".to_string(), "v1.3.0".to_string())]);
	let result = update_go_mod_text(input, &deps);

	assert!(
		result.contains("github.com/example/monorepo/shared v1.3.0"),
		"should update shared version"
	);
	assert!(
		result.contains("github.com/gin-gonic/gin v1.9.1"),
		"should preserve unrelated deps"
	);
	assert!(
		!result.contains("shared v1.2.0"),
		"should not have old version"
	);
}

#[test]
fn update_go_mod_text_preserves_comments() {
	let input = "module github.com/example/app\n\nrequire golang.org/x/sys v0.15.0 // indirect\n";
	let deps = BTreeMap::from([("sys".to_string(), "v0.16.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(result.contains("golang.org/x/sys v0.16.0 // indirect"));
}

#[test]
fn update_go_mod_text_handles_single_line_require() {
	let input = "module github.com/example/app\n\nrequire github.com/example/shared v1.0.0\n";
	let deps = BTreeMap::from([("shared".to_string(), "v2.0.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(result.contains("require github.com/example/shared v2.0.0"));
}

#[test]
fn update_go_mod_text_adds_v_prefix_when_missing() {
	let input = "module github.com/example/app\n\nrequire github.com/example/shared v1.0.0\n";
	let deps = BTreeMap::from([("shared".to_string(), "2.0.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(result.contains("require github.com/example/shared v2.0.0"));
}

#[test]
fn update_go_mod_text_preserves_module_and_go_directives() {
	let input =
		"module github.com/example/app\n\ngo 1.22\n\nrequire github.com/example/shared v1.0.0\n";
	let deps = BTreeMap::from([("shared".to_string(), "v2.0.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(result.contains("module github.com/example/app"));
	assert!(result.contains("go 1.22"));
}

#[test]
fn update_go_mod_text_returns_original_when_no_deps() {
	let input = "module github.com/example/app\n\ngo 1.22\n";
	let result = update_go_mod_text(input, &BTreeMap::new());
	assert_eq!(result, input);
}

#[test]
fn update_go_mod_text_preserves_replace_directives() {
	let input = r"module github.com/example/app

require github.com/example/shared v1.0.0

replace github.com/example/shared => ../shared
";
	let deps = BTreeMap::from([("shared".to_string(), "v2.0.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(result.contains("replace github.com/example/shared => ../shared"));
	assert!(result.contains("require github.com/example/shared v2.0.0"));
}

// -- should_descend --

#[test]
fn discover_go_modules_skips_vendor_directory() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	fs::write(
		root.join("go.mod"),
		"module github.com/example/root\n\ngo 1.22\n",
	)
	.unwrap();

	let vendor_dir = root.join("vendor/github.com/dep");
	fs::create_dir_all(&vendor_dir).unwrap();
	fs::write(
		vendor_dir.join("go.mod"),
		"module github.com/dep\n\ngo 1.22\n",
	)
	.unwrap();

	let discovery = discover_go_modules(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(
		discovery.packages.len(),
		1,
		"should only find root module, not vendored: {:?}",
		discovery
			.packages
			.iter()
			.map(|p| &p.name)
			.collect::<Vec<_>>()
	);
	assert_eq!(discovery.packages.first().unwrap().name, "root");
}

// -- edge cases --

#[test]
fn discover_go_modules_handles_nonexistent_directory() {
	let discovery = discover_go_modules(std::path::Path::new("/nonexistent/path/to/repo"));
	let result = discovery.unwrap_or_else(|error| panic!("unexpected error: {error}"));
	assert!(result.packages.is_empty());
}

#[test]
fn parse_require_directives_handles_malformed_entries() {
	let contents = "module github.com/example/app\n\nrequire (\n\t\n\tincomplete\n)\n";
	let deps = parse_require_directives(contents);
	assert!(deps.is_empty());
}

#[test]
fn update_go_mod_text_preserves_content_without_trailing_newline() {
	let input = "module github.com/example/app\n\nrequire github.com/example/shared v1.0.0";
	let deps = BTreeMap::from([("shared".to_string(), "v2.0.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(!result.ends_with('\n'), "should not add trailing newline");
	assert!(result.contains("github.com/example/shared v2.0.0"));
}

#[test]
fn update_go_mod_text_handles_versioned_module_require() {
	let input = "module github.com/example/app\n\nrequire github.com/example/sdk/v2 v2.0.0\n";
	let deps = BTreeMap::from([("sdk".to_string(), "v2.1.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(
		result.contains("github.com/example/sdk/v2 v2.1.0"),
		"should update v2 module: {result}"
	);
}

#[test]
fn update_go_mod_text_skips_single_line_require_without_version() {
	let input = "module github.com/example/app\n\nrequire github.com/example/shared\n";
	let result = update_go_mod_text(
		input,
		&BTreeMap::from([("shared".to_string(), "v2.0.0".to_string())]),
	);

	assert_eq!(result, input);
}

#[test]
fn update_go_mod_text_skips_lines_with_too_few_parts() {
	let input = "module github.com/example/app\n\nrequire (\n\t\n)\n";
	let result = update_go_mod_text(
		input,
		&BTreeMap::from([("app".to_string(), "v1.0.0".to_string())]),
	);
	assert_eq!(result, input, "should not modify empty require block lines");
}

#[test]
fn derive_module_name_handles_all_version_segments() {
	// If every segment is a version suffix (extremely unlikely but tests the fallback)
	assert_eq!(derive_module_name("v2"), "v2");
}

#[test]
fn update_go_mod_text_handles_retract_directive() {
	let input = "module github.com/example/app\n\nretract v0.1.0\n\nrequire github.com/example/shared v1.0.0\n";
	let deps = BTreeMap::from([("shared".to_string(), "v2.0.0".to_string())]);
	let result = update_go_mod_text(input, &deps);
	assert!(result.contains("retract v0.1.0"));
	assert!(result.contains("require github.com/example/shared v2.0.0"));
}
