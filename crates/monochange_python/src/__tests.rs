use std::collections::BTreeMap;
use std::fs;
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
	assert_eq!(supported_versioned_file_kind("unknown.txt".as_ref()), None);
	assert_eq!(supported_versioned_file_kind("Cargo.toml".as_ref()), None);
}

// -- normalize_python_package_name --

#[test]
fn normalize_python_package_name_lowercases_and_replaces_separators() {
	assert_eq!(normalize_python_package_name("My-Package"), "my-package");
	assert_eq!(normalize_python_package_name("my_package"), "my-package");
	assert_eq!(normalize_python_package_name("My.Package"), "my-package");
	assert_eq!(normalize_python_package_name("UPPER__CASE"), "upper-case");
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
	assert_eq!(parse_dependency_name(">=1.0"), None);
}

// -- parse_pep440_as_semver --

#[test]
fn parse_pep440_as_semver_handles_standard_and_two_part_versions() {
	assert_eq!(parse_pep440_as_semver("1.2.3"), Some(Version::new(1, 2, 3)));
	assert_eq!(parse_pep440_as_semver("0.1.0"), Some(Version::new(0, 1, 0)));
	assert_eq!(parse_pep440_as_semver("1.2"), Some(Version::new(1, 2, 0)));
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
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	let names: Vec<&str> = discovery.packages.iter().map(|p| p.name.as_str()).collect();
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
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = &discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "standalone-tool");
	assert_eq!(pkg.current_version, Some(Version::new(2, 5, 0)));
	assert_eq!(pkg.ecosystem, Ecosystem::Python);
}

#[test]
fn discover_python_packages_finds_poetry_project() {
	let root = fixture_path("python/poetry-project");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

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
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "dynamic-pkg");
	assert_eq!(pkg.current_version, None, "dynamic version should be None");
}

#[test]
fn discover_python_packages_handles_two_part_version() {
	let root = fixture_path("python/two-part-version");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.current_version, Some(Version::new(1, 2, 0)));
}

#[test]
fn discover_python_packages_skips_files_without_project_section() {
	let root = fixture_path("python/no-project-section");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert!(discovery.packages.is_empty());
}

#[test]
fn discover_python_packages_extracts_dependency_edges_from_uv_workspace() {
	let root = fixture_path("python/uv-workspace");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

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
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

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
	let result = update_versioned_file_text(input, PythonVersionedFileKind::Manifest, None, &deps)
		.unwrap_or_else(|error| panic!("update: {error}"));

	assert!(result.contains("my-core>=2.0.0"));
	assert!(!result.contains("my-core>=1.0.0"));
	assert!(
		result.contains("click>=8.0"),
		"unrelated deps should be preserved"
	);
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
	let result = update_versioned_file_text(input, PythonVersionedFileKind::Manifest, None, &deps)
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
	assert_eq!(extract_version_constraint("requests", "requests"), None);
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

// -- discover_lockfiles fallback to manifest dir --

#[test]
fn discover_lockfiles_falls_back_to_manifest_directory() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let workspace_root = tempdir.path().to_path_buf();
	let pkg_dir = workspace_root.join("packages/mylib");
	fs::create_dir_all(&pkg_dir).unwrap();
	fs::write(
		pkg_dir.join("pyproject.toml"),
		"[project]\nname = \"mylib\"\nversion = \"1.0.0\"\n",
	)
	.unwrap();
	// Put lockfile in package dir, NOT workspace root
	fs::write(pkg_dir.join("uv.lock"), "").unwrap();

	let package = PackageRecord::new(
		Ecosystem::Python,
		"mylib",
		pkg_dir.join("pyproject.toml"),
		workspace_root,
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert!(lockfiles.first().unwrap().ends_with("uv.lock"));
}

#[test]
fn discover_lockfiles_prefers_workspace_root_then_manifest_directory() {
	let root = fixture_path("python/uv-workspace");
	// Simulate a member package whose workspace root has a lockfile
	let package = PackageRecord::new(
		Ecosystem::Python,
		"my-core",
		root.join("packages/core/pyproject.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert!(!lockfiles.is_empty());
	assert!(lockfiles.first().unwrap().ends_with("uv.lock"));
}

// -- workspace pattern warnings --

#[test]
fn discover_python_packages_warns_on_empty_workspace_patterns() {
	let root = fixture_path("python/uv-workspace-empty-pattern");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert!(
		discovery.warnings.iter().any(|w| w.contains("nonexistent")),
		"expected warning about unmatched pattern: {:?}",
		discovery.warnings
	);
}

// -- parse error paths --

#[test]
fn discover_python_packages_warns_on_invalid_toml_in_standalone_scan() {
	let root = fixture_path("python/invalid-toml");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("unexpected error: {error}"));
	assert!(discovery.packages.is_empty());
	assert!(
		discovery
			.warnings
			.iter()
			.any(|w| w.contains("failed to parse")),
		"expected warning about parse failure: {:?}",
		discovery.warnings
	);
}

// -- Poetry complex dependency parsing --

#[test]
fn discover_python_packages_parses_complex_poetry_dependencies() {
	let root = fixture_path("python/poetry-complex");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "complex-poetry");
	assert_eq!(pkg.current_version, Some(Version::new(2, 0, 0)));

	let runtime_deps: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(
		runtime_deps.contains(&"django"),
		"missing django: {runtime_deps:?}"
	);
	assert!(
		runtime_deps.contains(&"celery"),
		"missing celery: {runtime_deps:?}"
	);
	assert!(
		runtime_deps.contains(&"local-pkg"),
		"missing local-pkg: {runtime_deps:?}"
	);
	assert!(
		!runtime_deps.contains(&"python"),
		"python should be excluded"
	);

	// Check Poetry has table-style constraints
	let celery_dep = pkg
		.declared_dependencies
		.iter()
		.find(|d| d.name == "celery")
		.unwrap();
	assert_eq!(celery_dep.version_constraint.as_deref(), Some("^5.3"));

	// Path dependencies have no version constraint
	let local_dep = pkg
		.declared_dependencies
		.iter()
		.find(|d| d.name == "local-pkg")
		.unwrap();
	assert_eq!(local_dep.version_constraint, None);

	let dev_deps: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(
		dev_deps.contains(&"pytest"),
		"missing pytest from test group: {dev_deps:?}"
	);
	assert!(
		dev_deps.contains(&"coverage"),
		"missing coverage from test group: {dev_deps:?}"
	);
	assert!(
		dev_deps.contains(&"ruff"),
		"missing ruff from lint group: {dev_deps:?}"
	);
}

// -- update with non-string items in dependency array --

#[test]
fn update_versioned_file_text_skips_non_string_dependency_items() {
	let input = r#"[project]
name = "test"
version = "1.0.0"
dependencies = [
    "my-core>=1.0.0",
    {include-group = "shared"},
]
"#;
	let deps = BTreeMap::from([("my-core".to_string(), ">=2.0.0".to_string())]);
	let result = update_versioned_file_text(input, PythonVersionedFileKind::Manifest, None, &deps)
		.unwrap_or_else(|error| panic!("update: {error}"));
	assert!(result.contains("my-core>=2.0.0"));
}

// -- extract_version_constraint extras --

#[test]
fn extract_version_constraint_handles_extras_in_specifier() {
	assert_eq!(
		extract_version_constraint("httpx[cli,http2]>=0.20.0", "httpx"),
		Some(">=0.20.0".to_string())
	);
}

#[test]
fn extract_version_constraint_returns_none_for_no_constraint() {
	assert_eq!(
		extract_version_constraint("simple-package", "simple-package"),
		None
	);
}

// -- should_descend coverage --

#[test]
fn discover_python_packages_skips_venv_and_pycache_directories() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Create a valid package at root
	fs::write(
		root.join("pyproject.toml"),
		"[project]\nname = \"root\"\nversion = \"1.0.0\"\ndependencies = []\n",
	)
	.unwrap();

	// Create packages in directories that should be skipped
	for dir in &[
		".venv",
		"venv",
		"__pycache__",
		".mypy_cache",
		".tox",
		"dist",
		"build",
	] {
		let pkg_dir = root.join(dir);
		fs::create_dir_all(&pkg_dir).unwrap();
		fs::write(
			pkg_dir.join("pyproject.toml"),
			format!("[project]\nname = \"{dir}\"\nversion = \"0.0.1\"\ndependencies = []\n"),
		)
		.unwrap();
	}

	let discovery =
		discover_python_packages(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(
		discovery.packages.len(),
		1,
		"should only find root package, not packages in excluded dirs: {:?}",
		discovery
			.packages
			.iter()
			.map(|p| &p.name)
			.collect::<Vec<_>>()
	);
	assert_eq!(discovery.packages.first().unwrap().name, "root");
}

// -- IO error paths --

#[test]
fn discover_python_packages_reports_io_error_for_unreadable_workspace_root() {
	let error = discover_python_packages(std::path::Path::new("/nonexistent/path/to/repo"));
	// Should not error since the root pyproject.toml simply doesn't exist
	// and the walker finds no files either
	let discovery = error.unwrap_or_else(|error| panic!("unexpected error: {error}"));
	assert!(discovery.packages.is_empty());
}

// -- workspace member expansion with direct pyproject.toml files --

#[test]
fn expand_workspace_members_handles_glob_matching_pyproject_files() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Create workspace root with glob pattern that matches pyproject.toml directly
	let pkg_dir = root.join("packages/core");
	fs::create_dir_all(&pkg_dir).unwrap();
	fs::write(
		pkg_dir.join("pyproject.toml"),
		"[project]\nname = \"core\"\nversion = \"1.0.0\"\ndependencies = []\n",
	)
	.unwrap();
	fs::write(
		root.join("pyproject.toml"),
		"[project]\nname = \"root\"\nversion = \"0.1.0\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\"]\n",
	)
	.unwrap();

	let discovery =
		discover_python_packages(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(discovery.packages.first().unwrap().name, "core");
}

#[test]
fn default_lockfile_commands_handles_uv_poetry_and_unknown_lockfiles() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	fs::write(
		root.join("pyproject.toml"),
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write pyproject: {error}"));
	fs::write(root.join("uv.lock"), "").unwrap_or_else(|error| panic!("write uv.lock: {error}"));
	fs::write(root.join("poetry.lock"), "")
		.unwrap_or_else(|error| panic!("write poetry.lock: {error}"));
	fs::write(root.join("requirements.lock"), "")
		.unwrap_or_else(|error| panic!("write requirements.lock: {error}"));
	let package = PackageRecord::new(
		Ecosystem::Python,
		"app",
		root.join("pyproject.toml"),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Unpublished,
	);

	let commands = crate::default_lockfile_commands(&package);
	let command_names = commands
		.iter()
		.map(|command| command.command.as_str())
		.collect::<Vec<_>>();

	assert_eq!(command_names, vec!["uv lock", "poetry lock --no-update"]);
	assert_eq!(crate::lockfile_command("requirements.lock"), None);
	let root_name = root.file_name().unwrap_or_else(|| panic!("temp root name"));
	assert!(
		commands
			.iter()
			.all(|command| command.cwd.ends_with(root_name))
	);
}

#[test]
fn update_versioned_file_text_returns_early_without_project_dependencies() {
	let deps = BTreeMap::from([("core".to_string(), ">=2.0.0".to_string())]);

	let without_project = update_versioned_file_text(
		"[tool.custom]\nname = \"app\"\n",
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&deps,
	)
	.unwrap_or_else(|error| panic!("update without project: {error}"));
	assert!(!without_project.contains("2.0.0"));

	let without_dependencies = update_versioned_file_text(
		"[project]\nname = \"app\"\nversion = \"1.0.0\"\n",
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&deps,
	)
	.unwrap_or_else(|error| panic!("update without dependencies: {error}"));
	assert!(without_dependencies.contains("version = \"2.0.0\""));
	assert!(!without_dependencies.contains("core>=2.0.0"));
}

#[test]
fn private_workspace_helpers_cover_error_and_match_branches() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let manifest_path = root.join("pyproject.toml");
	let packages = root.join("packages");
	let package_dir = packages.join("core");
	fs::create_dir_all(&package_dir).unwrap_or_else(|error| panic!("create package dir: {error}"));
	fs::write(
		&manifest_path,
		"[project]\nname = \"root\"\nversion = \"1.0.0\"\n\n[tool.uv.workspace]\nmembers = [\"packages/*\", \"packages/core/pyproject.toml\", \"README.md\", \"missing/*\"]\n",
	)
	.unwrap_or_else(|error| panic!("write root pyproject: {error}"));
	fs::write(
		package_dir.join("pyproject.toml"),
		"[project]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write core pyproject: {error}"));
	fs::write(root.join("README.md"), "# docs\n")
		.unwrap_or_else(|error| panic!("write readme: {error}"));

	let members = crate::parse_uv_workspace_members(&manifest_path)
		.unwrap_or_else(|error| panic!("parse uv members: {error}"))
		.unwrap_or_else(|| panic!("expected uv workspace members"));
	assert!(members.contains(&"packages/*".to_string()));

	let missing_error = crate::parse_uv_workspace_members(&root.join("missing-pyproject.toml"))
		.err()
		.unwrap_or_else(|| panic!("expected missing pyproject error"));
	assert!(missing_error.to_string().contains("failed to read"));

	fs::write(root.join("invalid.toml"), "[project\n")
		.unwrap_or_else(|error| panic!("write invalid toml: {error}"));
	let parse_error = crate::parse_uv_workspace_members(&root.join("invalid.toml"))
		.err()
		.unwrap_or_else(|| panic!("expected invalid toml error"));
	assert!(parse_error.to_string().contains("failed to parse"));

	let mut warnings = Vec::new();
	let manifests = crate::expand_workspace_members(root, &members, &mut warnings);
	assert!(
		manifests
			.iter()
			.any(|manifest| manifest.ends_with("packages/core/pyproject.toml")),
		"missing core manifest in {manifests:?}"
	);
	assert!(warnings.iter().any(|warning| warning.contains("missing/*")));
}

#[test]
fn private_package_parser_covers_error_and_dependency_value_branches() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let missing_error = crate::parse_python_package(&root.join("missing.toml"), root)
		.err()
		.unwrap_or_else(|| panic!("expected missing package error"));
	assert!(missing_error.to_string().contains("failed to read"));

	let invalid_path = root.join("invalid.toml");
	fs::write(&invalid_path, "[project\n").unwrap_or_else(|error| panic!("write invalid: {error}"));
	let parse_error = crate::parse_python_package(&invalid_path, root)
		.err()
		.unwrap_or_else(|| panic!("expected parse package error"));
	assert!(parse_error.to_string().contains("failed to parse"));

	let project = toml::from_str::<toml::Value>(
		r#"
[project]
dependencies = ["requests>=2", 1]

[project.optional-dependencies]
dev = ["pytest>=8", 1]
empty = "not-an-array"
"#,
	)
	.unwrap_or_else(|error| panic!("parse project fixture: {error}"));
	let project_table = project
		.get("project")
		.unwrap_or_else(|| panic!("expected project table"));
	let pep621_deps = crate::parse_pep621_dependencies(project_table);
	assert!(
		pep621_deps
			.iter()
			.any(|dep| dep.name == "requests" && !dep.optional)
	);
	assert!(
		pep621_deps
			.iter()
			.any(|dep| dep.name == "pytest" && dep.optional)
	);

	let poetry = toml::from_str::<toml::Value>(
		r#"
[dependencies]
python = ">=3.11"
django = "^4.2"
celery = { version = "^5.3" }
local = { path = "../local" }
unknown = 1

[group]
metadata = "not-a-group-table"

[group.empty]
dependencies = "not-a-dependency-table"

[group.dev.dependencies]
pytest = "^8.0"
ruff = { version = "^0.4" }
tooling = { path = "../tooling" }
numbered = 1
"#,
	)
	.unwrap_or_else(|error| panic!("parse poetry fixture: {error}"));
	let empty_poetry = toml::Value::Table(toml::map::Map::new());
	assert!(crate::parse_poetry_dependencies(&empty_poetry).is_empty());

	let deps = crate::parse_poetry_dependencies(&poetry);
	assert!(
		deps.iter()
			.any(|dep| dep.name == "django" && dep.version_constraint.as_deref() == Some("^4.2"))
	);
	assert!(
		deps.iter()
			.any(|dep| dep.name == "celery" && dep.version_constraint.as_deref() == Some("^5.3"))
	);
	assert!(
		deps.iter()
			.any(|dep| dep.name == "local" && dep.version_constraint.is_none())
	);
	assert!(
		deps.iter()
			.any(|dep| dep.name == "ruff" && dep.version_constraint.as_deref() == Some("^0.4"))
	);
	assert!(
		deps.iter()
			.any(|dep| dep.name == "tooling" && dep.version_constraint.is_none())
	);
	assert!(!deps.iter().any(|dep| dep.name == "python"));
}

// -- update_versioned_file with both version and deps --

#[test]
fn update_versioned_file_text_updates_version_and_deps_simultaneously() {
	let input = r#"[project]
name = "my-cli"
version = "1.0.0"
dependencies = [
    "my-core>=1.0.0",
    "httpx[cli]>=0.20.0",
]
"#;
	let deps = BTreeMap::from([
		("my-core".to_string(), ">=2.0.0".to_string()),
		("httpx".to_string(), ">=1.0.0".to_string()),
	]);
	let result = update_versioned_file_text(
		input,
		PythonVersionedFileKind::Manifest,
		Some("2.0.0"),
		&deps,
	)
	.unwrap_or_else(|error| panic!("update: {error}"));

	assert!(result.contains(r#"version = "2.0.0""#));
	assert!(result.contains("my-core>=2.0.0"));
	assert!(result.contains("httpx[cli]>=1.0.0"));
}

// -- PEP 440 edge cases --

#[test]
fn parse_pep440_as_semver_handles_pre_release_and_invalid_formats() {
	// Pre-release suffixes are not supported
	assert_eq!(parse_pep440_as_semver("1.0.0rc1"), None);
	assert_eq!(parse_pep440_as_semver("1.0.0.post1"), None);
	assert_eq!(parse_pep440_as_semver("1.0.0.dev0"), None);
	// Four-part versions are not semver
	assert_eq!(parse_pep440_as_semver("1.2.3.4"), None);
	// Valid two-part
	assert_eq!(parse_pep440_as_semver("0.1"), Some(Version::new(0, 1, 0)));
}

// -- normalize edge cases --

#[test]
fn normalize_python_package_name_handles_leading_and_trailing_separators() {
	assert_eq!(normalize_python_package_name("-leading"), "leading");
	assert_eq!(normalize_python_package_name("trailing-"), "trailing-");
	assert_eq!(normalize_python_package_name("a"), "a");
}

#[test]
fn discover_python_packages_skips_project_without_name() {
	let root = fixture_path("python/project-no-name");
	let discovery =
		discover_python_packages(&root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert!(
		discovery.packages.is_empty(),
		"should skip packages without a name"
	);
}

#[test]
fn update_versioned_file_text_handles_version_and_deps_with_env_markers() {
	let input = r#"[project]
name = "cli"
version = "1.0.0"
dependencies = [
    "numpy>=1.20.0; python_version < '3.9'",
    "my-core>=1.0.0",
]
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
	assert!(result.contains("my-core>=2.0.0"));
	// Environment markers should be stripped from the constraint
	assert!(result.contains("numpy"));
}
