use std::collections::BTreeMap;
use std::path::PathBuf;

use monochange_core::DependencyKind;
use monochange_core::Ecosystem;
use monochange_core::EcosystemAdapter;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use semver::Version;

use crate::discover_jvm_projects;
use crate::parse_gradle_dependencies;
use crate::parse_gradle_subprojects;
use crate::parse_gradle_version;
use crate::parse_maven_artifact_id;
use crate::parse_maven_dependencies;
use crate::parse_maven_version;
use crate::update_gradle_build_version;
use crate::update_pom_version;
use crate::update_version_catalog_text;
use crate::JvmAdapter;
use crate::JvmVersionedFileKind;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

// -- adapter --

#[test]
fn adapter_reports_jvm_ecosystem() {
	assert_eq!(JvmAdapter.ecosystem(), Ecosystem::Jvm);
}

#[test]
fn adapter_discover_delegates_to_discover_jvm_projects() {
	let root = fixture_path("jvm/maven-single");
	let discovery = JvmAdapter
		.discover(&root)
		.unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(discovery.packages.first().unwrap().name, "my-app");
}

// -- supported_versioned_file_kind --

#[test]
fn supported_versioned_file_kind_recognizes_jvm_files() {
	use crate::supported_versioned_file_kind;
	assert_eq!(
		supported_versioned_file_kind("build.gradle.kts".as_ref()),
		Some(JvmVersionedFileKind::GradleBuild)
	);
	assert_eq!(
		supported_versioned_file_kind("build.gradle".as_ref()),
		Some(JvmVersionedFileKind::GradleBuild)
	);
	assert_eq!(
		supported_versioned_file_kind("libs.versions.toml".as_ref()),
		Some(JvmVersionedFileKind::VersionCatalog)
	);
	assert_eq!(
		supported_versioned_file_kind("gradle.lockfile".as_ref()),
		Some(JvmVersionedFileKind::GradleLock)
	);
	assert_eq!(
		supported_versioned_file_kind("pom.xml".as_ref()),
		Some(JvmVersionedFileKind::MavenPom)
	);
	assert_eq!(supported_versioned_file_kind("Cargo.toml".as_ref()), None);
}

// -- parse_gradle_version --

#[test]
fn parse_gradle_version_extracts_version_from_build_file() {
	let contents = "group = \"com.example\"\nversion = \"1.2.3\"\n";
	assert_eq!(parse_gradle_version(contents), Some(Version::new(1, 2, 3)));
}

#[test]
fn parse_gradle_version_returns_none_without_version() {
	let contents = "group = \"com.example\"\n";
	assert_eq!(parse_gradle_version(contents), None);
}

#[test]
fn parse_gradle_version_handles_non_semver() {
	let contents = "version = \"SNAPSHOT\"\n";
	assert_eq!(parse_gradle_version(contents), None);
}

// -- parse_gradle_subprojects --

#[test]
fn parse_gradle_subprojects_extracts_include_directives() {
	let path = fixture_path("jvm/gradle-multi/settings.gradle.kts");
	let subprojects = parse_gradle_subprojects(&path);
	assert!(
		subprojects.contains(&"core".to_string()),
		"missing core: {subprojects:?}"
	);
	assert!(
		subprojects.contains(&"api".to_string()),
		"missing api: {subprojects:?}"
	);
}

#[test]
fn parse_gradle_subprojects_handles_colon_prefixed_names() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap();
	let settings = tempdir.path().join("settings.gradle.kts");
	fs::write(&settings, "include(\":core\", \":api\")\n").unwrap();
	let subprojects = parse_gradle_subprojects(&settings);
	assert!(subprojects.contains(&"core".to_string()));
	assert!(subprojects.contains(&"api".to_string()));
}

#[test]
fn parse_gradle_subprojects_returns_empty_for_no_includes() {
	let path = fixture_path("jvm/gradle-single/settings.gradle.kts");
	let subprojects = parse_gradle_subprojects(&path);
	assert!(subprojects.is_empty());
}

// -- parse_gradle_dependencies --

#[test]
fn parse_gradle_dependencies_extracts_all_configurations() {
	let contents = r#"dependencies {
    implementation("com.google.guava:guava:33.0.0-jre")
    api("com.example:core:1.0.0")
    compileOnly("org.projectlombok:lombok:1.18.30")
    testImplementation("junit:junit:4.13.2")
}"#;
	let deps = parse_gradle_dependencies(contents);

	let runtime: Vec<&str> = deps
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(runtime.contains(&"guava"), "missing guava: {runtime:?}");
	assert!(runtime.contains(&"core"), "missing core: {runtime:?}");
	assert!(
		runtime.contains(&"lombok"),
		"compileOnly should be runtime: {runtime:?}"
	);

	let dev: Vec<&str> = deps
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(dev.contains(&"junit"), "missing junit: {dev:?}");

	let optional = deps.iter().find(|d| d.name == "lombok").unwrap();
	assert!(optional.optional, "compileOnly should be optional");
}

#[test]
fn parse_gradle_dependencies_handles_empty_file() {
	let deps = parse_gradle_dependencies("plugins { application }\n");
	assert!(deps.is_empty());
}

// -- parse_maven_artifact_id --

#[test]
fn parse_maven_artifact_id_extracts_artifact() {
	let contents = "<project>\n  <artifactId>my-app</artifactId>\n</project>";
	assert_eq!(
		parse_maven_artifact_id(contents),
		Some("my-app".to_string())
	);
}

#[test]
fn parse_maven_artifact_id_returns_none_without_artifact() {
	let contents = "<project>\n  <groupId>com.example</groupId>\n</project>";
	assert_eq!(parse_maven_artifact_id(contents), None);
}

// -- parse_maven_version --

#[test]
fn parse_maven_version_extracts_semver() {
	let contents = "<project>\n  <version>2.1.0</version>\n</project>";
	assert_eq!(parse_maven_version(contents), Some(Version::new(2, 1, 0)));
}

#[test]
fn parse_maven_version_skips_property_references() {
	let contents = "<project>\n  <version>${revision}</version>\n</project>";
	assert_eq!(parse_maven_version(contents), None);
}

#[test]
fn parse_maven_version_returns_none_without_version() {
	let contents = "<project>\n  <artifactId>test</artifactId>\n</project>";
	assert_eq!(parse_maven_version(contents), None);
}

// -- parse_maven_dependencies --

#[test]
fn parse_maven_dependencies_extracts_deps_with_scopes() {
	let contents = r"<project>
  <dependencies>
    <dependency>
      <groupId>org.springframework</groupId>
      <artifactId>spring-core</artifactId>
      <version>6.1.0</version>
    </dependency>
    <dependency>
      <groupId>junit</groupId>
      <artifactId>junit</artifactId>
      <version>4.13.2</version>
      <scope>test</scope>
    </dependency>
    <dependency>
      <groupId>javax.servlet</groupId>
      <artifactId>javax.servlet-api</artifactId>
      <version>4.0.1</version>
      <scope>provided</scope>
    </dependency>
  </dependencies>
</project>";
	let deps = parse_maven_dependencies(contents);
	assert_eq!(deps.len(), 3);

	let spring = deps.iter().find(|d| d.name == "spring-core").unwrap();
	assert_eq!(spring.kind, DependencyKind::Runtime);
	assert_eq!(spring.version_constraint.as_deref(), Some("6.1.0"));

	let junit = deps.iter().find(|d| d.name == "junit").unwrap();
	assert_eq!(junit.kind, DependencyKind::Development);

	let servlet = deps.iter().find(|d| d.name == "javax.servlet-api").unwrap();
	assert_eq!(servlet.kind, DependencyKind::Build);
}

#[test]
fn parse_maven_dependencies_skips_property_versions() {
	let contents = r"<project>
  <dependencies>
    <dependency>
      <groupId>com.example</groupId>
      <artifactId>core</artifactId>
      <version>${project.version}</version>
    </dependency>
  </dependencies>
</project>";
	let deps = parse_maven_dependencies(contents);
	assert_eq!(deps.len(), 1);
	assert_eq!(deps.first().unwrap().version_constraint, None);
}

// -- discover_jvm_projects --

#[test]
fn discover_jvm_projects_finds_gradle_multi_project() {
	let root = fixture_path("jvm/gradle-multi");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 2);
	let names: Vec<&str> = discovery.packages.iter().map(|p| p.name.as_str()).collect();
	assert!(names.contains(&"core"), "missing core: {names:?}");
	assert!(names.contains(&"api"), "missing api: {names:?}");

	let core = discovery
		.packages
		.iter()
		.find(|p| p.name == "core")
		.unwrap();
	assert_eq!(core.current_version, Some(Version::new(1, 0, 0)));
	assert_eq!(core.ecosystem, Ecosystem::Jvm);
	assert_eq!(
		core.metadata.get("build_tool").map(String::as_str),
		Some("gradle")
	);
}

#[test]
fn discover_jvm_projects_finds_maven_single_project() {
	let root = fixture_path("jvm/maven-single");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "my-app");
	assert_eq!(pkg.current_version, Some(Version::new(2, 1, 0)));
	assert_eq!(
		pkg.metadata.get("build_tool").map(String::as_str),
		Some("maven")
	);
}

#[test]
fn discover_jvm_projects_finds_maven_multi_module() {
	let root = fixture_path("jvm/maven-multi");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let names: Vec<&str> = discovery.packages.iter().map(|p| p.name.as_str()).collect();
	assert!(names.contains(&"core"), "missing core: {names:?}");
	assert!(names.contains(&"api"), "missing api: {names:?}");
	assert!(names.contains(&"parent"), "missing parent: {names:?}");
}

#[test]
fn discover_jvm_projects_extracts_gradle_dependencies() {
	let root = fixture_path("jvm/gradle-multi");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let api = discovery.packages.iter().find(|p| p.name == "api").unwrap();
	let dep_names: Vec<&str> = api
		.declared_dependencies
		.iter()
		.map(|d| d.name.as_str())
		.collect();
	assert!(
		dep_names.contains(&"core"),
		"api should depend on core: {dep_names:?}"
	);
	assert!(
		dep_names.contains(&"guava"),
		"api should depend on guava: {dep_names:?}"
	);
}

#[test]
fn discover_jvm_projects_extracts_maven_dependencies() {
	let root = fixture_path("jvm/maven-single");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	let pkg = discovery.packages.first().unwrap();
	let runtime: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Runtime)
		.map(|d| d.name.as_str())
		.collect();
	assert!(runtime.contains(&"spring-core"));

	let dev: Vec<&str> = pkg
		.declared_dependencies
		.iter()
		.filter(|d| d.kind == DependencyKind::Development)
		.map(|d| d.name.as_str())
		.collect();
	assert!(dev.contains(&"junit"));
}

#[test]
fn discover_jvm_projects_handles_maven_property_version() {
	let root = fixture_path("jvm/no-version");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));

	assert_eq!(discovery.packages.len(), 1);
	let pkg = discovery.packages.first().unwrap();
	assert_eq!(pkg.name, "no-version");
	assert_eq!(
		pkg.current_version, None,
		"property-based version should be None"
	);
}

#[test]
fn discover_jvm_projects_handles_nonexistent_directory() {
	let discovery = discover_jvm_projects(std::path::Path::new("/nonexistent/path"));
	let result = discovery.unwrap_or_else(|error| panic!("unexpected error: {error}"));
	assert!(result.packages.is_empty());
}

// -- discover_lockfiles --

#[test]
fn discover_lockfiles_returns_empty_without_lockfile() {
	let root = fixture_path("jvm/gradle-single");
	let package = PackageRecord::new(
		Ecosystem::Jvm,
		"standalone-app",
		root.join("build.gradle.kts"),
		root.clone(),
		Some(Version::new(3, 0, 0)),
		PublishState::Public,
	);
	let lockfiles = crate::discover_lockfiles(&package);
	assert!(lockfiles.is_empty());
}

// -- default_lockfile_commands --

#[test]
fn default_lockfile_commands_infers_gradle_for_gradle_projects() {
	let root = fixture_path("jvm/gradle-single");
	let package = PackageRecord::new(
		Ecosystem::Jvm,
		"standalone-app",
		root.join("build.gradle.kts"),
		root.clone(),
		Some(Version::new(3, 0, 0)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert!(
		commands.first().unwrap().command.contains("gradle"),
		"should infer gradle command"
	);
}

#[test]
fn default_lockfile_commands_returns_empty_for_maven() {
	let root = fixture_path("jvm/maven-single");
	let package = PackageRecord::new(
		Ecosystem::Jvm,
		"my-app",
		root.join("pom.xml"),
		root.clone(),
		Some(Version::new(2, 1, 0)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert!(commands.is_empty(), "Maven has no lockfile commands");
}

// -- update_gradle_build_version --

#[test]
fn update_gradle_build_version_replaces_version() {
	let input = "group = \"com.example\"\nversion = \"1.0.0\"\n";
	let result = update_gradle_build_version(input, "2.0.0");
	assert!(result.contains("version = \"2.0.0\""));
	assert!(!result.contains("1.0.0"));
}

#[test]
fn update_gradle_build_version_preserves_other_content() {
	let input = "plugins { application }\ngroup = \"com.example\"\nversion = \"1.0.0\"\n";
	let result = update_gradle_build_version(input, "2.0.0");
	assert!(result.contains("plugins { application }"));
	assert!(result.contains("group = \"com.example\""));
	assert!(result.contains("version = \"2.0.0\""));
}

#[test]
fn update_gradle_build_version_handles_no_version() {
	let input = "group = \"com.example\"\n";
	let result = update_gradle_build_version(input, "2.0.0");
	assert_eq!(result, input, "should not modify when no version found");
}

// -- update_version_catalog_text --

#[test]
fn update_version_catalog_text_updates_versions() {
	let input = "[versions]\nguava = \"33.0.0-jre\"\njunit = \"4.13.2\"\n";
	let deps = BTreeMap::from([("guava".to_string(), "34.0.0-jre".to_string())]);
	let result =
		update_version_catalog_text(input, &deps).unwrap_or_else(|error| panic!("update: {error}"));
	assert!(result.contains("guava = \"34.0.0-jre\""));
	assert!(
		result.contains("junit = \"4.13.2\""),
		"should preserve junit"
	);
}

#[test]
fn update_version_catalog_text_returns_original_when_no_deps() {
	let input = "[versions]\nguava = \"33.0.0\"\n";
	let result = update_version_catalog_text(input, &BTreeMap::new())
		.unwrap_or_else(|error| panic!("update: {error}"));
	assert_eq!(result, input);
}

#[test]
fn update_version_catalog_text_handles_missing_versions_section() {
	let input = "[libraries]\nguava = { module = \"com.google.guava:guava\" }\n";
	let deps = BTreeMap::from([("guava".to_string(), "34.0.0".to_string())]);
	let result =
		update_version_catalog_text(input, &deps).unwrap_or_else(|error| panic!("update: {error}"));
	assert_eq!(
		result, input,
		"should not modify when no [versions] section"
	);
}

// -- update_pom_version --

#[test]
fn update_pom_version_replaces_project_version() {
	let input =
		"<project>\n  <artifactId>test</artifactId>\n  <version>1.0.0</version>\n</project>";
	let result = update_pom_version(input, "2.0.0");
	assert!(result.contains("<version>2.0.0</version>"));
	assert!(!result.contains("1.0.0"));
}

#[test]
fn update_pom_version_preserves_other_content() {
	let input =
		"<project>\n  <groupId>com.example</groupId>\n  <version>1.0.0</version>\n</project>";
	let result = update_pom_version(input, "2.0.0");
	assert!(result.contains("<groupId>com.example</groupId>"));
	assert!(result.contains("<version>2.0.0</version>"));
}

#[test]
fn update_pom_version_handles_no_version() {
	let input = "<project>\n  <artifactId>test</artifactId>\n</project>";
	let result = update_pom_version(input, "2.0.0");
	assert_eq!(result, input);
}

// -- should_descend --

#[test]
fn discover_jvm_projects_skips_build_and_gradle_directories() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Create a valid Maven project at root
	fs::write(
		root.join("pom.xml"),
		"<project>\n  <artifactId>root</artifactId>\n  <version>1.0.0</version>\n</project>\n",
	)
	.unwrap();

	// Create projects in directories that should be skipped
	for dir in &[".gradle", "build", ".mvn", "target"] {
		let sub_dir = root.join(dir);
		fs::create_dir_all(&sub_dir).unwrap();
		fs::write(
			sub_dir.join("pom.xml"),
			format!(
				"<project>\n  <artifactId>{dir}</artifactId>\n  <version>0.0.1</version>\n</project>\n"
			),
		)
		.unwrap();
	}

	let discovery = discover_jvm_projects(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(
		discovery.packages.len(),
		1,
		"should only find root project: {:?}",
		discovery
			.packages
			.iter()
			.map(|p| &p.name)
			.collect::<Vec<_>>()
	);
	assert_eq!(discovery.packages.first().unwrap().name, "root");
}

#[test]
fn discover_jvm_projects_warns_on_missing_subproject_directory() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap();
	let root = tempdir.path();
	fs::write(
		root.join("settings.gradle.kts"),
		"include(\"nonexistent\")\n",
	)
	.unwrap();
	let discovery = discover_jvm_projects(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert!(
		discovery.warnings.iter().any(|w| w.contains("nonexistent")),
		"expected warning about missing subproject: {:?}",
		discovery.warnings
	);
}

#[test]
fn discover_jvm_projects_finds_groovy_build_files() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap();
	let root = tempdir.path();
	fs::write(root.join("settings.gradle.kts"), "include(\"lib\")\n").unwrap();
	let lib_dir = root.join("lib");
	fs::create_dir_all(&lib_dir).unwrap();
	fs::write(
		lib_dir.join("build.gradle"),
		"group = 'com.example'\nversion = \"2.0.0\"\n",
	)
	.unwrap();
	let discovery = discover_jvm_projects(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert_eq!(discovery.packages.len(), 1);
	assert_eq!(
		discovery.packages.first().unwrap().current_version,
		Some(Version::new(2, 0, 0))
	);
}

#[test]
fn discover_jvm_projects_skips_subproject_without_build_file() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap();
	let root = tempdir.path();
	fs::write(root.join("settings.gradle.kts"), "include(\"empty\")\n").unwrap();
	let empty_dir = root.join("empty");
	fs::create_dir_all(&empty_dir).unwrap();
	let discovery = discover_jvm_projects(root).unwrap_or_else(|error| panic!("discover: {error}"));
	assert!(discovery.packages.is_empty());
}

#[test]
fn default_lockfile_commands_prefers_gradlew_wrapper() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap();
	let root = tempdir.path();
	fs::write(root.join("gradlew"), "#!/bin/sh\n").unwrap();
	fs::write(root.join("build.gradle.kts"), "version = \"1.0.0\"\n").unwrap();
	let package = PackageRecord::new(
		Ecosystem::Jvm,
		"test",
		root.join("build.gradle.kts"),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let commands = crate::default_lockfile_commands(&package);
	assert_eq!(commands.len(), 1);
	assert!(
		commands.first().unwrap().command.contains("gradlew"),
		"should prefer gradlew: {}",
		commands.first().unwrap().command
	);
}

#[test]
fn parse_gradle_subprojects_handles_simple_include_syntax() {
	use std::fs;
	let tempdir = tempfile::tempdir().unwrap();
	let settings = tempdir.path().join("settings.gradle");
	fs::write(&settings, "include \"core\"\ninclude \"api\"\n").unwrap();
	let subprojects = parse_gradle_subprojects(&settings);
	assert!(subprojects.contains(&"core".to_string()));
	assert!(subprojects.contains(&"api".to_string()));
}

#[test]
fn parse_maven_dependencies_handles_no_dependencies() {
	let contents = "<project>\n  <artifactId>test</artifactId>\n</project>";
	let deps = parse_maven_dependencies(contents);
	assert!(deps.is_empty());
}

#[test]
fn parse_gradle_dependencies_extracts_version_constraints() {
	let contents = r#"dependencies {
    implementation("com.example:lib:1.2.3")
}"#;
	let deps = parse_gradle_dependencies(contents);
	assert_eq!(deps.len(), 1);
	assert_eq!(
		deps.first().unwrap().version_constraint.as_deref(),
		Some("1.2.3")
	);
}

#[test]
fn discover_jvm_projects_skips_already_discovered_maven_dirs() {
	// When a directory is already discovered via Gradle, Maven pom.xml should be skipped
	let root = fixture_path("jvm/gradle-multi");
	let discovery =
		discover_jvm_projects(&root).unwrap_or_else(|error| panic!("discover: {error}"));
	// Should only have Gradle subprojects, not duplicated Maven
	let build_tools: Vec<&str> = discovery
		.packages
		.iter()
		.filter_map(|p| p.metadata.get("build_tool").map(String::as_str))
		.collect();
	assert!(
		build_tools.iter().all(|t| *t == "gradle"),
		"all should be gradle: {build_tools:?}"
	);
}
