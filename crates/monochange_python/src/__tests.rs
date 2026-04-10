use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;

use crate::PythonAdapter;
use crate::PythonVersionedFileKind;
use crate::discover_python_packages;
use crate::extract_version_constraint;
use crate::normalize_python_package_name;
use crate::parse_dependency_name;
use crate::parse_pep440_as_semver;
use crate::update_dependency_specifier;
use crate::update_versioned_file_text;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

// -- adapter --

#[test]
fn adapter_reports_python_ecosystem() {
	use monochange_core::EcosystemAdapter;
	let adapter = crate::adapter();
	assert_eq!(adapter.ecosystem(), Ecosystem::Python);
}

// -- supported_versioned_file_kind --

#[test]
fn supported_versioned_file_kind_recognizes_manifest_and_lockfiles() {
	use crate::supported_versioned_file_kind;
	assert_eq!(
		supported_versioned_file_kind("pyproject.toml".as_ref()),
		Some(PythonVersionedFileKind::Manifest)
	);
	assert_eq!(
		supported_versioned_file_kind("uv.lock".as_ref()),
		Some(PythonVersionedFileKind::Lock)
	);
	assert_eq!(
		supported_versioned_file_kind("poetry.lock".as_ref()),
		Some(PythonVersionedFileKind::Lock)
	);
	assert_eq!(
		supported_versioned_file_kind("unknown.txt".as_ref()),
		None
	);
	assert_eq!(
		supported_versioned_file_kind("Cargo.toml".as_ref()),
		None
	);
}

// -- normalize_python_package_name --

#[test]
fn normalize_python_package_name_lowercases_and_replaces_separators() {
	assert_eq!(normalize_python_package_name("My-Package"), "my-package");
	assert_eq!(normalize_python_package_name("my_package"), "my-package");
	assert_eq!(normalize_python_package_name("My.Package"), "my-package");
	assert_eq!(
		normalize_python_package_name("UPPER__CASE"),
		"upper-case"
	);
	assert_eq!(normalize_python_package_name("simple"), "simple");
	assert_eq!(normalize_python_package_name(""), "");
	assert_eq!(normalize_python_package_name("a-_-b"), "a-b");
}

// -- parse_dependency_name --

#[test]
fn parse_dependency_name_extracts_name_from_pep508_specifiers() {
	assert_eq!(
		parse_dependency_name("httpx>=0.20.0"),
		Some("httpx".to_string())
	);
	assert_eq!(
		parse_dependency_name("Django>2.1; os_name != 'nt'"),
		Some("Django".to_string())
	);
	assert_eq!(
		parse_dependency_name("my-package>=1.0"),
		Some("my-package".to_string())
	);
	assert_eq!(
		parse_dependency_name("my_package>=1.0"),
		Some("my_package".to_string())
	);
	assert_eq!(
		parse_dependency_name("pkg.name>=1.0"),
		Some("pkg.name".to_string())
	);
	assert_eq!(parse_dependency_name(""), None);
	assert_eq!(
		parse_dependency_name(">=1.0"),
		None
	);
}

// -- parse_pep440_as_semver --

#[test]
fn parse_pep440_as_semver_handles_standard_and_two_part_versions() {
	assert_eq!(
		parse_pep440_as_semver("1.2.3"),
		Some(Version::new(1, 2, 3))
	);
	assert_eq!(
		parse_pep440_as_semver("0.1.0"),
		Some(Version::new(0, 1, 0))
	);
	assert_eq!(
		parse_pep440_as_semver("1.2"),
		Some(Version::new(1, 2, 0))
	);
	assert_eq!(parse_pep440_as_semver("1.2.3a1"), None);
	assert_eq!(parse_pep440_as_semver("not-a-version"), None);
	assert_eq!(parse_pep440_as_semver("1"), None);
}

// -- update_dependency_specifier --

#[test]
fn update_dependency_specifier_replaces_matching_version_constraints() {
	let deps = BTreeMap::from([("my-core".to_string(), ">=2.0.0".to_string())]);

	assert_eq!(
		update_dependency_specifier("my-core>=1.0.0", &deps),
		Some("my-core>=2.0.0".to_string())
	);
	assert_eq!(
		update_dependency_specifier("my_core>=1.0.0", &deps),
		Some("my_core>=2.0.0".to_string())
	);
	assert_eq!(
		update_dependency_specifier("other-package>=1.0", &deps),
		None
	);
}

#[test]
fn update_dependency_specifier_preserves_extras() {
	let deps = BTreeMap::from([("httpx".to_string(), ">=2.0.0".to_string())]);

	assert_eq!(
		update_dependency_specifier("httpx[cli]>=1.0.0", &deps),
		Some("httpx[cli]>=2.0.0".to_string())
	);
}

// -- discover_python_packages --

#[test]
fn discover_python_packages_finds_uv_workspace_members() {
	let root = fixture_path("python/uv-workspace");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	let names: Vec<&str> = discovery
		.packages
		.iter()
		.map(|p| p.name.as_str())
		.collect();
	assert!(names.contains(&"my-core"), "missing my-core: {names:?}");
	assert!(names.contains(&"my-cli"), "missing my-cli: {names:?}");

	let core = discovery
		.packages
		.iter()
		.find(|p| p.name == "my-core")
		.unwrap();
	assert_eq!(core.ecosystem, Ecosystem::Python);
	assert_eq!(core.current_version, Some(Version::new(1, 0, 0)));
	assert!(core.declared_dependencies.len() >= 2);
}

#[test]
fn discover_python_packages_finds_standalone_package() {
	let root = fixture_path("python/standalone");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = &discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "standalone-tool");
	assert_eq!(pkg.current_version, Some(Version::new(2, 5, 0)));
	assert_eq!(pkg.ecosystem, Ecosystem::Python);
}

#[test]
fn discover_python_packages_finds_poetry_project() {
	let root = fixture_path("python/poetry-project");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "poetry-app");
	assert_eq!(pkg.current_version, Some(Version::new(3, 1, 0)));

	let runtime_deps: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(runtime_deps.contains(&"django"));
	assert!(runtime_deps.contains(&"celery"));
	assert!(
		!runtime_deps.contains(&"python"),
		"python itself should be excluded"
	);

	let dev_deps: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(dev_deps.contains(&"pytest"));
	assert!(dev_deps.contains(&"black"));
}

#[test]
fn discover_python_packages_handles_dynamic_version() {
	let root = fixture_path("python/dynamic-version");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "dynamic-pkg");
	assert_eq!(pkg.current_version, None, "dynamic version should be None");
}

#[test]
fn discover_python_packages_handles_two_part_version() {
	let root = fixture_path("python/two-part-version");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.current_version, Some(Version::new(1, 2, 0)));
}

#[test]
fn discover_python_packages_skips_files_without_project_section() {
	let root = fixture_path("python/no-project-section");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	assert!(discovery.packages.is_empty());
}

#[test]
fn discover_python_packages_extracts_dependency_edges_from_uv_workspace() {
	let root = fixture_path("python/uv-workspace");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	let cli = discovery
		.packages
		.iter()
		.find(|p| p.name == "my-cli")
		.unwrap();
	let dep_names: Vec<&str> = cli
		.declared_dependencies
		.iter()
		.map(|d| d.name.as_str())
		.collect();
	assert!(dep_names.contains(&"my-core"));
	assert!(dep_names.contains(&"click"));
}

#[test]
fn discover_python_packages_extracts_optional_dependency_edges() {
	let root = fixture_path("python/uv-workspace");
	let discovery = discover_python_packages(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));

	let core = discovery
		.packages
		.iter()
		.find(|p| p.name == "my-core")
		.unwrap();
	let optional_deps: Vec<&str> = core
		.declared_dependencies
		.iter()
		.filter(|d| d.optional)
		.map(|d| d.name.as_str())
		.collect();
	assert!(optional_deps.contains(&"pytest"));
	assert!(optional_deps.contains(&"ruff"));
}

// -- discover_lockfiles --

#[test]
fn discover_lockfiles_returns_empty_when_no_lockfile_exists() {
	let root = fixture_path("python/standalone");
	let package = PackageRecord::new(
		Ecosystem::Python,
		"standalone-tool",
		root.join("pyproject.toml"),
		root.clone(),
		Some(Version::new(2, 5, 0)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert!(lockfiles.is_empty());
}

// -- default_lockfile_commands --

#[test]
fn default_lockfile_commands_return_empty_when_no_lockfile_exists() {
	let root = fixture_path("python/standalone");
	let package = PackageRecord::new(
		Ecosystem::Python,
		"standalone-tool",
		root.join("pyproject.toml"),
		root.clone(),
		Some(Version::new(2, 5, 0)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert!(commands.is_empty());
}

// -- update_versioned_file_text --

#[test]
fn update_versioned_file_text_updates_project_version() {
	let input = r#"[project]
name = "my-core"
version = "1.0.0"
dependencies = []
"#;
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert!(result.contains(r#"version = "2.0.0""#));
	assert!(!result.contains(r#"version = "1.0.0""#));
}

#[test]
fn update_versioned_file_text_updates_dependency_versions() {
	let input = r#"[project]
name = "my-cli"
version = "1.0.0"
dependencies = [
    "my-core>=1.0.0",
    "click>=8.0",
]
"#;
	let deps = BTreeMap::from([("my-core".to_string(), ">=2.0.0".to_string())]);
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		None,
		&deps,
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert!(result.contains("my-core>=2.0.0"));
	assert!(!result.contains("my-core>=1.0.0"));
	assert!(result.contains("click>=8.0"), "unrelated deps should be preserved");
}

#[test]
fn update_versioned_file_text_preserves_lock_files_unchanged() {
	let input = "# uv.lock contents\n[[package]]\nname = \"test\"\n";
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Lock,
		Some("2.0.0"),
		&BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert_eq!(result, input, "lock files should not be mutated directly");
}

#[test]
fn update_versioned_file_text_handles_missing_project_section() {
	let input = "[build-system]\nrequires = [\"setuptools\"]\n";
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert_eq!(result, input, "no project section means no changes");
}

#[test]
fn update_versioned_file_text_handles_no_version_to_update() {
	let input = r#"[project]
name = "my-core"
version = "1.0.0"
dependencies = []
"#;
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		None,
		&BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert!(
		result.contains(r#"version = "1.0.0""#),
		"version should be unchanged when no new version is provided"
	);
}

#[test]
fn update_versioned_file_text_handles_empty_dependency_array() {
	let input = r#"[project]
name = "my-core"
version = "1.0.0"
dependencies = []
"#;
	let deps = BTreeMap::from([("nonexistent".to_string(), ">=2.0.0".to_string())]);
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		None,
		&deps,
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert_eq!(result, input, "no matching deps means no changes");
}

// -- adapter trait dispatch --

#[test]
fn adapter_discover_delegates_to_discover_python_packages() {
	use monochange_core::EcosystemAdapter;
	let root = fixture_path("python/standalone");
	let discovery = PythonAdapter
		.discover(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(discovery.packages.first().unwrap().name, "standalone-tool");
}

// -- discover_lockfiles with real lockfiles --

#[test]
fn discover_lockfiles_finds_uv_lock_at_workspace_root() {
	let root = fixture_path("python/uv-workspace");
	let package = PackageRecord::new(
		Ecosystem::Python,
		"my-core",
		root.join("packages/core/pyproject.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert!(lockfiles.first().unwrap().ends_with("uv.lock"));
}

#[test]
fn discover_lockfiles_finds_poetry_lock() {
	let root = fixture_path("python/poetry-project");
	let package = PackageRecord::new(
		Ecosystem::Python,
		"poetry-app",
		root.join("pyproject.toml"),
		root.clone(),
		Some(Version::new(3, 1, 0)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert!(lockfiles.first().unwrap().ends_with("poetry.lock"));
}

// -- default_lockfile_commands with real lockfiles --

#[test]
fn default_lockfile_commands_infers_uv_lock_for_uv_workspace() {
	let root = fixture_path("python/uv-workspace");
	let package = PackageRecord::new(
		Ecosystem::Python,
		"my-core",
		root.join("packages/core/pyproject.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert_eq!(commands.first().unwrap().command, "uv lock");
}

#[test]
fn default_lockfile_commands_infers_poetry_lock_for_poetry_project() {
	let root = fixture_path("python/poetry-project");
	let package = PackageRecord::new(
		Ecosystem::Python,
		"poetry-app",
		root.join("pyproject.toml"),
		root.clone(),
		Some(Version::new(3, 1, 0)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert_eq!(commands.first().unwrap().command, "poetry lock --no-update");
}

// -- extract_version_constraint --

#[test]
fn extract_version_constraint_handles_simple_and_complex_specifiers() {
	assert_eq!(
		extract_version_constraint("httpx>=0.20.0", "httpx"),
		Some(">=0.20.0".to_string())
	);
	assert_eq!(
		extract_version_constraint("httpx[cli]>=0.20.0", "httpx"),
		Some(">=0.20.0".to_string())
	);
	assert_eq!(
		extract_version_constraint("numpy>=1.20.0; python_version < '3.9'", "numpy"),
		Some(">=1.20.0".to_string())
	);
	assert_eq!(
		extract_version_constraint("requests", "requests"),
		None
	);
}

// -- update with missing dependencies array --

#[test]
fn update_versioned_file_text_handles_project_without_dependencies_key() {
	let input = r#"[project]
name = "minimal"
version = "1.0.0"
"#;
	let deps = BTreeMap::from([("my-core".to_string(), ">=2.0.0".to_string())]);
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&deps,
	)
	.unwrap_or_else(|error| panic!("update: {error}"));
	assert!(result.contains(r#"version = "2.0.0""#));
}

// -- update_dependency_specifier edge cases --

#[test]
fn update_dependency_specifier_returns_none_for_empty_spec() {
	let deps = BTreeMap::from([("pkg".to_string(), ">=1.0".to_string())]);
	assert_eq!(update_dependency_specifier("", &deps), None);
}

#[test]
fn update_dependency_specifier_handles_name_only_spec() {
	let deps = BTreeMap::from([("requests".to_string(), ">=2.0".to_string())]);
	assert_eq!(
		update_dependency_specifier("requests", &deps),
		Some("requests>=2.0".to_string())
	);
}
