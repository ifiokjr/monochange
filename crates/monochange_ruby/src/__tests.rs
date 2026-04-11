use std::path::PathBuf;

use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;

use crate::discover_ruby_gems;
use crate::parse_gem_name;
use crate::parse_gemspec_dependencies;
use crate::parse_version_constant;
use crate::update_version_file_text;
use crate::RubyAdapter;
use crate::RubyVersionedFileKind;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

// -- adapter --

#[test]
fn adapter_reports_ruby_ecosystem() {
	assert_eq!(RubyAdapter.ecosystem(), Ecosystem::Ruby);
}

#[test]
fn adapter_discover_delegates_to_discover_ruby_gems() {
	let root = fixture_path("ruby/single-gem");
	let discovery = RubyAdapter
		.discover(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(discovery.packages.first().unwrap().name, "my_gem");
}

// -- supported_versioned_file_kind --

#[test]
fn supported_versioned_file_kind_recognizes_ruby_files() {
	use crate::supported_versioned_file_kind;
	assert_eq!(
		supported_versioned_file_kind("my_gem.gemspec".as_ref()),
		Some(RubyVersionedFileKind::Gemspec)
	);
	assert_eq!(
		supported_versioned_file_kind("version.rb".as_ref()),
		Some(RubyVersionedFileKind::VersionFile)
	);
	assert_eq!(
		supported_versioned_file_kind("Gemfile.lock".as_ref()),
		Some(RubyVersionedFileKind::Lock)
	);
	assert_eq!(supported_versioned_file_kind("Cargo.toml".as_ref()), None);
	assert_eq!(supported_versioned_file_kind("other.rb".as_ref()), None);
}

// -- parse_gem_name --

#[test]
fn parse_gem_name_extracts_name_from_gemspec() {
	let contents = r#"Gem::Specification.new do |spec|
  spec.name = "my_gem"
  spec.version = MyGem::VERSION
end"#;
	assert_eq!(parse_gem_name(contents), Some("my_gem".to_string()));
}

#[test]
fn parse_gem_name_handles_single_quotes() {
	let contents = "Gem::Specification.new do |s|\n  s.name = 'cool-gem'\nend";
	assert_eq!(parse_gem_name(contents), Some("cool-gem".to_string()));
}

#[test]
fn parse_gem_name_handles_short_block_variable() {
	let contents = "Gem::Specification.new do |s|\n  s.name    = \"short\"\nend";
	assert_eq!(parse_gem_name(contents), Some("short".to_string()));
}

#[test]
fn parse_gem_name_returns_none_without_name() {
	let contents = "Gem::Specification.new do |spec|\n  spec.summary = \"no name\"\nend";
	assert_eq!(parse_gem_name(contents), None);
}

// -- parse_version_constant --

#[test]
fn parse_version_constant_extracts_semver() {
	assert_eq!(
		parse_version_constant("  VERSION = \"1.2.3\"\n"),
		Some(Version::new(1, 2, 3))
	);
	assert_eq!(
		parse_version_constant("  VERSION = '0.1.0'\n"),
		Some(Version::new(0, 1, 0))
	);
	assert_eq!(
		parse_version_constant("module Foo\n  VERSION = \"10.20.30\"\nend\n"),
		Some(Version::new(10, 20, 30))
	);
}

#[test]
fn parse_version_constant_returns_none_for_non_semver() {
	assert_eq!(parse_version_constant("VERSION = 'not-a-version'"), None);
	assert_eq!(parse_version_constant("no version here"), None);
	assert_eq!(parse_version_constant(""), None);
}

// -- parse_gemspec_dependencies --

#[test]
fn parse_gemspec_dependencies_extracts_runtime_and_dev_deps() {
	let contents = r#"Gem::Specification.new do |spec|
  spec.name = "test"
  spec.add_dependency "rails", "~> 7.0"
  spec.add_runtime_dependency "redis", ">= 4.0"
  spec.add_development_dependency "rspec", "~> 3.0"
end"#;
	let deps = parse_gemspec_dependencies(contents);

	let runtime: Vec<&str> = deps
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(runtime.contains(&"rails"), "missing rails: {runtime:?}");
	assert!(runtime.contains(&"redis"), "missing redis: {runtime:?}");

	let dev: Vec<&str> = deps
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(dev.contains(&"rspec"), "missing rspec: {dev:?}");
}

#[test]
fn parse_gemspec_dependencies_extracts_version_constraints() {
	let contents = "Gem::Specification.new do |s|\n  s.add_dependency \"rails\", \"~> 7.0\"\nend";
	let deps = parse_gemspec_dependencies(contents);
	assert_eq!(deps.len(), 1);
	assert_eq!(
		deps.first().unwrap().version_constraint.as_deref(),
		Some("~> 7.0")
	);
}

#[test]
fn parse_gemspec_dependencies_handles_no_deps() {
	let contents = "Gem::Specification.new do |spec|\n  spec.name = \"bare\"\nend";
	let deps = parse_gemspec_dependencies(contents);
	assert!(deps.is_empty());
}

// -- discover_ruby_gems --

#[test]
fn discover_ruby_gems_finds_single_gem() {
	let root = fixture_path("ruby/single-gem");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "my_gem");
	assert_eq!(pkg.ecosystem, Ecosystem::Ruby);
	assert_eq!(pkg.current_version, Some(Version::new(1, 2, 3)));
}

#[test]
fn discover_ruby_gems_finds_monorepo_gems() {
	let root = fixture_path("ruby/monorepo");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	let names: Vec<&str> = discovery.packages.iter().map(|p| p.name.as_str()).collect();
	assert!(names.contains(&"core"), "missing core: {names:?}");
	assert!(names.contains(&"api"), "missing api: {names:?}");

	let core = discovery
		.packages
		.iter()
		.find(|p| p.name == "core")
		.unwrap();
	assert_eq!(core.current_version, Some(Version::new(2, 0, 0)));

	let api = discovery.packages.iter().find(|p| p.name == "api").unwrap();
	assert_eq!(api.current_version, Some(Version::new(1, 5, 0)));
}

#[test]
fn discover_ruby_gems_extracts_dependencies() {
	let root = fixture_path("ruby/single-gem");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let pkg = discovery.packages.first().unwrap();
	let runtime: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(runtime.contains(&"rails"));
	assert!(runtime.contains(&"redis"));

	let dev: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(dev.contains(&"rspec"));
	assert!(dev.contains(&"rubocop"));
}

#[test]
fn discover_ruby_gems_handles_runtime_dependency_alias() {
	let root = fixture_path("ruby/monorepo");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let core = discovery
		.packages
		.iter()
		.find(|p| p.name == "core")
		.unwrap();
	let dep_names: Vec<&str> = core
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(
		dep_names.contains(&"concurrent-ruby"),
		"add_runtime_dependency should be treated as runtime: {dep_names:?}"
	);
}

#[test]
fn discover_ruby_gems_skips_gemspec_without_name() {
	let root = fixture_path("ruby/no-name");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert!(discovery.packages.is_empty());
}

#[test]
fn discover_ruby_gems_handles_gem_without_version_file() {
	let root = fixture_path("ruby/no-version");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "no_version");
	assert_eq!(pkg.current_version, None);
}

#[test]
fn discover_ruby_gems_handles_nonexistent_directory() {
	let discovery = discover_ruby_gems(std::path::Path::new("/nonexistent/path"));
	let result = discovery.unwrap_or_else(|error| panic!("unexpected error: {error}"));
	assert!(result.packages.is_empty());
}

// -- discover_lockfiles --

#[test]
fn discover_lockfiles_finds_gemfile_lock() {
	let root = fixture_path("ruby/single-gem");
	let package = PackageRecord::new(
		Ecosystem::Ruby,
		"my_gem",
		root.join("my_gem.gemspec"),
		root.clone(),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert_eq!(lockfiles.len(), 1);
	assert!(lockfiles.first().unwrap().ends_with("Gemfile.lock"));
}

#[test]
fn discover_lockfiles_returns_empty_without_gemfile_lock() {
	let root = fixture_path("ruby/no-version");
	let package = PackageRecord::new(
		Ecosystem::Ruby,
		"no_version",
		root.join("no_version.gemspec"),
		root.clone(),
		None,
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert!(lockfiles.is_empty());
}

// -- default_lockfile_commands --

#[test]
fn default_lockfile_commands_infers_bundle_lock() {
	let root = fixture_path("ruby/single-gem");
	let package = PackageRecord::new(
		Ecosystem::Ruby,
		"my_gem",
		root.join("my_gem.gemspec"),
		root.clone(),
		Some(Version::new(1, 2, 3)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert_eq!(commands.first().unwrap().command, "bundle lock --update");
}

#[test]
fn default_lockfile_commands_returns_empty_without_lockfile() {
	let root = fixture_path("ruby/no-version");
	let package = PackageRecord::new(
		Ecosystem::Ruby,
		"no_version",
		root.join("no_version.gemspec"),
		root.clone(),
		None,
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert!(commands.is_empty());
}

// -- update_version_file_text --

#[test]
fn update_version_file_text_replaces_double_quoted_version() {
	let input = "module MyGem\n  VERSION = \"1.2.3\"\nend\n";
	let result = update_version_file_text(input, "2.0.0");
	assert!(result.contains("VERSION = \"2.0.0\""));
	assert!(!result.contains("1.2.3"));
}

#[test]
fn update_version_file_text_replaces_single_quoted_version() {
	let input = "module MyGem\n  VERSION = '1.2.3'\nend\n";
	let result = update_version_file_text(input, "2.0.0");
	assert!(result.contains("VERSION = '2.0.0'"));
	assert!(!result.contains("1.2.3"));
}

#[test]
fn update_version_file_text_preserves_surrounding_content() {
	let input = "# frozen_string_literal: true\n\nmodule MyGem\n  VERSION = \"1.0.0\"\nend\n";
	let result = update_version_file_text(input, "1.1.0");
	assert!(result.contains("# frozen_string_literal: true"));
	assert!(result.contains("module MyGem"));
	assert!(result.contains("end"));
	assert!(result.contains("VERSION = \"1.1.0\""));
}

#[test]
fn update_version_file_text_handles_no_match() {
	let input = "module MyGem\n  # no version here\nend\n";
	let result = update_version_file_text(input, "2.0.0");
	assert_eq!(
		result, input,
		"should return unchanged when no VERSION found"
	);
}

// -- should_descend --

#[test]
fn discover_ruby_gems_skips_vendor_and_bundle_directories() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Create a valid gem at root
	fs::write(
		root.join("root.gemspec"),
		"Gem::Specification.new do |s|\n  s.name = \"root\"\nend\n",
	)
	.unwrap();

	// Create gems in directories that should be skipped
	for dir in &["vendor", ".bundle", "tmp", "pkg"] {
		let gem_dir = root.join(dir);
		fs::create_dir_all(&gem_dir).unwrap();
		fs::write(
			gem_dir.join("hidden.gemspec"),
			format!("Gem::Specification.new do |s|\n  s.name = \"{dir}\"\nend\n"),
		)
		.unwrap();
	}

	let discovery = discover_ruby_gems(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(
		discovery.packages.len(),
		1,
		"should only find root gem, not gems in excluded dirs: {:?}",
		discovery
			.packages
			.iter()
			.map(|p| &p.name)
			.collect::<Vec<_>>()
	);
	assert_eq!(discovery.packages.first().unwrap().name, "root");
}

#[test]
fn discover_ruby_gems_finds_version_in_deeply_nested_path() {
	let root = fixture_path("ruby/nested-version");
	let discovery = discover_ruby_gems(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "nested_version");
	assert_eq!(
		pkg.current_version,
		Some(Version::new(3, 0, 0)),
		"should find version.rb via recursive search"
	);
}

#[test]
fn parse_gemspec_dependencies_with_single_quotes() {
	let contents = "Gem::Specification.new do |s|\n  s.add_dependency 'rack', '~> 2.0'\n  s.add_development_dependency 'minitest', '~> 5.0'\nend";
	let deps = parse_gemspec_dependencies(contents);
	let runtime: Vec<&str> = deps
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(runtime.contains(&"rack"));

	let dev: Vec<&str> = deps
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(dev.contains(&"minitest"));
}
