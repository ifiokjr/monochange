use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_core::Ecosystem;
use tempfile::tempdir;

use crate::add_change_file;
use crate::build_command_for_root;
use crate::discover_workspace;
use crate::evaluate_changeset_policy;
use crate::plan_release;
use crate::run_with_args;
use crate::run_with_args_in_dir;

fn run_cli<I>(root: &Path, args: I) -> monochange_core::MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	run_with_args_in_dir("mc", args, root)
}

#[test]
fn cli_parses_discover_command() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let matches = build_command_for_root("mc", &fixture_root)
		.try_get_matches_from([OsString::from("mc"), OsString::from("discover")])
		.unwrap_or_else(|error| panic!("matches: {error}"));

	assert_eq!(matches.subcommand_name(), Some("discover"));
}

#[test]
fn cli_help_returns_success_output() {
	let output = run_with_args("mc", [OsString::from("mc"), OsString::from("--help")])
		.unwrap_or_else(|error| panic!("help output: {error}"));

	assert!(output.contains("Usage: mc <COMMAND>"));
	assert!(output.contains("change"));
}

#[test]
fn init_writes_detected_packages_groups_and_default_cli_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	);
	write_file(
		tempdir.path().join("packages/web/package.json"),
		"{\"name\":\"web\",\"version\":\"1.0.0\"}",
	);

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.unwrap_or_else(|error| panic!("init output: {error}"));
	let config = fs::read_to_string(tempdir.path().join("monochange.toml"))
		.unwrap_or_else(|error| panic!("config: {error}"));

	assert!(output.contains("wrote"));
	assert!(config.contains("[package.core]"));
	assert!(config.contains("[package.web]"));
	assert!(config.contains("[group.main]"));
	assert!(config.contains("[cli.validate]"));
	assert!(config.contains("[cli.discover]"));
	assert!(config.contains("[cli.change]"));
	assert!(config.contains("[cli.release]"));
	assert!(config.contains("type = \"Discover\""));
	assert!(config.contains("type = \"CreateChangeFile\""));
}

#[test]
fn init_requires_force_to_overwrite_existing_configuration() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("monochange.toml"),
		"[defaults]\nparent_bump = \"patch\"\n",
	);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("init")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected init failure"));
	assert!(error.to_string().contains("--force"));
}

#[test]
fn validate_command_validates_workspace_configuration_and_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	);
	write_file(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"

[group.sdk]
packages = ["core"]
"#,
	);
	write_file(
		tempdir.path().join(".changeset/feature.md"),
		r"---
core: minor
---

#### validate workspace
",
	);

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(output.contains("workspace validation passed"));
}

#[test]
fn validate_command_reports_invalid_changeset_targets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	);
	write_file(
		tempdir.path().join("monochange.toml"),
		r#"
[package.core]
path = "crates/core"
type = "cargo"
"#,
	);
	write_file(
		tempdir.path().join(".changeset/feature.md"),
		r"---
missing: minor
---

#### invalid target
",
	);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	assert!(error
		.to_string()
		.contains("unknown package or group `missing`"));
}

#[test]
fn discover_workspace_aggregates_packages_from_multiple_ecosystems() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let discovery =
		discover_workspace(&fixture_root).unwrap_or_else(|error| panic!("discovery: {error}"));

	assert_eq!(discovery.packages.len(), 4);
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.ecosystem == Ecosystem::Cargo));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.ecosystem == Ecosystem::Npm));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.ecosystem == Ecosystem::Deno));
	assert!(discovery
		.packages
		.iter()
		.any(|package| package.ecosystem == Ecosystem::Dart));
	assert_eq!(discovery.dependencies.len(), 3);
	assert_eq!(discovery.version_groups.len(), 1);
	let version_group = discovery
		.version_groups
		.first()
		.unwrap_or_else(|| panic!("expected a version group"));
	assert_eq!(version_group.members.len(), 2);
}

#[test]
fn workspace_discover_json_output_contains_contract_fields() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let output = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("discover"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("json: {error}"));

	assert_eq!(parsed["workspaceRoot"].as_str(), Some("."));
	assert_eq!(parsed["packages"].as_array().map(Vec::len), Some(4));
	assert_eq!(parsed["versionGroups"].as_array().map(Vec::len), Some(1));
	assert_eq!(parsed["dependencies"].as_array().map(Vec::len), Some(3));
	assert!(parsed["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages array"))
		.iter()
		.all(|package| package.get("manifestPath").is_some()));
	assert!(parsed["packages"]
		.as_array()
		.unwrap_or_else(|| panic!("packages array"))
		.iter()
		.all(|package| package["id"]
			.as_str()
			.is_some_and(|id| !id.contains(fixture_root.to_string_lossy().as_ref()))));
}

#[test]
fn plan_release_aggregates_transitive_dependents_and_version_groups() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let plan = plan_release(&fixture_root, &fixture_root.join("changes-minor.md"))
		.unwrap_or_else(|error| panic!("release plan: {error}"));

	let sdk_core = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("cargo/sdk-core/Cargo.toml"))
		.unwrap_or_else(|| panic!("expected cargo core decision"));
	let web_sdk = plan
		.decisions
		.iter()
		.find(|decision| {
			decision
				.package_id
				.contains("packages/web-sdk/package.json")
		})
		.unwrap_or_else(|| panic!("expected web sdk decision"));
	let deno_tool = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("deno/tool/deno.json"))
		.unwrap_or_else(|| panic!("expected deno tool decision"));
	let mobile_sdk = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains("dart/mobile_sdk/pubspec.yaml"))
		.unwrap_or_else(|| panic!("expected mobile sdk decision"));
	let version_group = plan
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected version group plan"));

	assert_eq!(sdk_core.recommended_bump.to_string(), "minor");
	assert_eq!(sdk_core.trigger_type, "direct-change");
	assert_eq!(web_sdk.recommended_bump.to_string(), "minor");
	assert_eq!(web_sdk.trigger_type, "version-group-synchronization");
	assert_eq!(deno_tool.recommended_bump.to_string(), "patch");
	assert_eq!(mobile_sdk.recommended_bump.to_string(), "patch");
	assert_eq!(
		version_group
			.planned_version
			.as_ref()
			.map(ToString::to_string)
			.as_deref(),
		Some("1.1.0")
	);
}

#[test]
fn changes_add_writes_a_change_file_via_the_cli() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("feature.md");

	let output = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("sdk-core"),
			OsString::from("--package"),
			OsString::from("web-sdk"),
			OsString::from("--bump"),
			OsString::from("minor"),
			OsString::from("--reason"),
			OsString::from("feature foundation"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(output.contains("wrote change file"));
	assert!(content.contains("sdk-core: minor"));
	assert!(content.contains("web-sdk: minor"));
	assert!(content.contains("#### feature foundation"));
}

#[test]
fn changes_add_supports_release_note_type_and_details() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("security.md");

	run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("sdk-core"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("rotate signing keys"),
			OsString::from("--type"),
			OsString::from("security"),
			OsString::from("--details"),
			OsString::from("Roll the signing key before the release window closes."),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(content.contains("type:"));
	assert!(content.contains("sdk-core: security"));
	assert!(content.contains("#### rotate signing keys"));
	assert!(content.contains("Roll the signing key before the release window closes."));
}

#[test]
fn changes_add_canonicalizes_package_references_to_package_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("crates/monochange/Cargo.toml"),
		"[package]\nname = \"monochange\"\nversion = \"1.0.0\"\n",
	);
	let output_path = tempdir.path().join("repo-change.md");

	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("crates/monochange"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("canonical package names"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(content.contains("monochange: patch"));
	assert!(!content.contains("crates/monochange: patch"));
}

#[test]
fn add_change_file_creates_default_path_under_changeset_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let output_path = add_change_file(
		&fixture_root,
		&["sdk-core".to_string()],
		monochange_core::BumpSeverity::Patch,
		"default output",
		None,
		None,
		&[],
		None,
	)
	.unwrap_or_else(|error| panic!("default change file: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(output_path.starts_with(fixture_root.join(".changeset")));
	assert!(content.contains("sdk-core: patch"));
	fs::remove_file(output_path).unwrap_or_else(|error| panic!("cleanup change file: {error}"));
}

#[test]
fn changes_add_supports_evidence_and_round_trips_into_plan_release() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("major.md");

	run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("sdk-core"),
			OsString::from("--bump"),
			OsString::from("patch"),
			OsString::from("--reason"),
			OsString::from("breaking change"),
			OsString::from("--evidence"),
			OsString::from("rust-semver:major:public API break detected"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));
	let plan = plan_release(&fixture_root, &output_path)
		.unwrap_or_else(|error| panic!("release plan: {error}"));

	assert!(content.contains("evidence:"));
	assert!(content.contains("rust-semver:major:public API break detected"));
	assert!(plan
		.compatibility_evidence
		.iter()
		.any(|assessment| assessment.severity.to_string() == "major"));
}

#[test]
fn changes_add_rejects_unknown_package_references() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let error = run_cli(
		&fixture_root,
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("missing-package"),
			OsString::from("--reason"),
			OsString::from("should fail"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected failure"));

	assert!(error
		.to_string()
		.contains("did not match any discovered package"));
}

#[test]
fn release_dry_run_json_output_contains_compatibility_evidence() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	write_file(
		tempdir.path().join(".changeset/feature.md"),
		r"---
core: patch
origin:
  core: direct-change
evidence:
  core:
    - rust-semver:major:public API break detected
---

#### breaking api change
",
	);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("release output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("json: {error}"));

	assert_eq!(
		parsed["plan"]["compatibilityEvidence"]
			.as_array()
			.map(Vec::len),
		Some(1)
	);
	assert_eq!(
		parsed["plan"]["compatibilityEvidence"][0]["severity"].as_str(),
		Some("major")
	);
	assert!(parsed["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"))
		.iter()
		.any(|decision| decision["bump"].as_str() == Some("major")));
}

#[test]
fn plan_release_expands_group_targeted_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	write_file(
		tempdir.path().join("group-change.md"),
		r"---
sdk: minor
---

#### grouped release
",
	);
	let plan = plan_release(tempdir.path(), &tempdir.path().join("group-change.md"))
		.unwrap_or_else(|error| panic!("group release plan: {error}"));
	let direct_count = plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.to_string() == "minor")
		.count();
	assert_eq!(direct_count, 2);
}

#[test]
fn command_release_dry_run_json_includes_deployment_intents() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	write_file(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = "{path}/changelog.md"

[package.core]
path = "crates/core"
versioned_files = ["crates/core/extra.toml"]

[package.app]
path = "crates/app"
changelog = "crates/app/changelog.md"

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
versioned_files = ["group.toml"]
tag = true
release = true
version_format = "primary"

[[deployments]]
name = "production"
trigger = "workflow"
workflow = "deploy-production"
environment = "production"
release_targets = ["sdk"]
requires = ["main"]

[deployments.metadata]
channel = "stable"

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"

[[cli.release.steps]]
type = "Deploy"
"#,
	);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let json = serde_json::from_str::<serde_json::Value>(&output)
		.unwrap_or_else(|error| panic!("parse command json: {error}"));

	assert_eq!(json["deployments"][0]["name"], "production");
	assert_eq!(json["deployments"][0]["trigger"], "workflow");
	assert_eq!(json["deployments"][0]["workflow"], "deploy-production");
	assert_eq!(json["deployments"][0]["environment"], "production");
	assert_eq!(json["deployments"][0]["releaseTargets"][0], "sdk");
	assert_eq!(json["deployments"][0]["requires"][0], "main");
	assert_eq!(json["deployments"][0]["metadata"]["channel"], "stable");
}

#[test]
fn command_release_dry_run_discovers_changesets_without_mutating_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);
	let workspace_manifest = tempdir.path().join("Cargo.toml");
	let core_changelog = tempdir.path().join("crates/core/changelog.md");
	let original_manifest = fs::read_to_string(&workspace_manifest)
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let original_changelog = fs::read_to_string(&core_changelog)
		.unwrap_or_else(|error| panic!("core changelog: {error}"));

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("command `release` completed (dry-run)"));
	assert!(output.contains("version: 1.1.0"));
	assert!(output.contains("workflow-app"));
	assert!(output.contains("workflow-core"));
	assert_eq!(
		fs::read_to_string(&workspace_manifest)
			.unwrap_or_else(|error| panic!("workspace manifest after dry-run: {error}")),
		original_manifest
	);
	assert_eq!(
		fs::read_to_string(&core_changelog)
			.unwrap_or_else(|error| panic!("core changelog after dry-run: {error}")),
		original_changelog
	);
	assert!(tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_release_updates_manifests_changelogs_and_deletes_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(
		tempdir.path(),
		Some("printf '%s' \"$version\" > release-version.txt"),
		false,
	);

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let workspace_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/changelog.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/changelog.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	let group_versioned_file = fs::read_to_string(tempdir.path().join("group.toml"))
		.unwrap_or_else(|error| panic!("group versioned file: {error}"));
	let package_versioned_file = fs::read_to_string(tempdir.path().join("crates/core/extra.toml"))
		.unwrap_or_else(|error| panic!("package versioned file: {error}"));
	let release_version = fs::read_to_string(tempdir.path().join("release-version.txt"))
		.unwrap_or_else(|error| panic!("release version output: {error}"));

	assert!(output.contains("command `release` completed"));
	assert!(output.contains("group sdk -> v1.1.0"));
	assert!(workspace_manifest.contains("version = \"1.1.0\""));
	assert!(core_changelog.contains("## 1.1.0"));
	assert!(core_changelog.contains("- add release command"));
	assert!(app_changelog.contains("## 1.1.0"));
	assert!(app_changelog.contains("No package-specific changes were recorded; `workflow-app` was updated to 1.1.0 as part of group `sdk`."));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("Members: core, app"));
	assert!(group_versioned_file.contains("version = \"1.1.0\""));
	assert!(package_versioned_file.contains("version = \"1.1.0\""));
	assert_eq!(release_version, "1.1.0");
	assert!(!tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_release_uses_empty_update_message_precedence_for_grouped_changelogs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_empty_update_message_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/changelog.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/changelog.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(output.contains("command `release` completed"));
	assert!(core_changelog.contains("Package override for workflow-core -> 1.0.1"));
	assert!(app_changelog.contains("Update triggered by group sdk; version 1.0.1."));
	assert!(group_changelog.contains("Update triggered by group sdk; version 1.0.1."));
}

#[test]
fn command_release_failures_do_not_delete_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, true);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected command failure"));

	assert!(error.to_string().contains("failed to create"));
	assert!(tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_unknown_commands_suggest_available_cli() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(tempdir.path(), None, false);

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("ship-it")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected command suggestion"));

	assert!(error
		.to_string()
		.contains("unrecognized subcommand 'ship-it'"));
}

#[test]
fn cli_command_command_steps_can_run_through_the_shell() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.announce]

[[cli.announce.steps]]
type = "Command"
command = "printf '%s' shell-command > shell-output.txt"
shell = true
"#,
	);

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("announce")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let shell_output = fs::read_to_string(tempdir.path().join("shell-output.txt"))
		.unwrap_or_else(|error| panic!("shell output: {error}"));

	assert!(output.contains("command `announce` completed"));
	assert_eq!(shell_output, "shell-command");
}

#[test]
fn cli_command_command_steps_use_dry_run_overrides_when_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.announce]

[[cli.announce.steps]]
type = "Command"
command = "printf '%s' real-run > command-output.txt"
dry_run_command = "printf '%s' dry-run > dry-run-output.txt"
shell = true
"#,
	);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("announce"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let dry_run_output = fs::read_to_string(tempdir.path().join("dry-run-output.txt"))
		.unwrap_or_else(|error| panic!("dry-run output: {error}"));

	assert!(output.contains("command `announce` completed (dry-run)"));
	assert_eq!(dry_run_output, "dry-run");
	assert!(!tempdir.path().join("command-output.txt").exists());
}

#[test]
fn evaluate_changeset_policy_requires_changesets_for_matching_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	let evaluation = evaluate_changeset_policy(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&Vec::new(),
	)
	.unwrap_or_else(|error| panic!("policy evaluation: {error}"));

	assert_eq!(
		evaluation.status,
		monochange_core::ChangesetPolicyStatus::Failed
	);
	assert!(evaluation.required);
	assert!(evaluation.comment.is_some());
	assert_eq!(evaluation.matched_paths, vec!["crates/core/src/lib.rs"]);
}

#[test]
fn evaluate_changeset_policy_skips_when_allowed_label_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	let evaluation = evaluate_changeset_policy(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&["no-changeset-required".to_string()],
	)
	.unwrap_or_else(|error| panic!("policy evaluation: {error}"));

	assert_eq!(
		evaluation.status,
		monochange_core::ChangesetPolicyStatus::Skipped
	);
	assert!(!evaluation.required);
	assert_eq!(
		evaluation.matched_skip_labels,
		vec!["no-changeset-required"]
	);
}

#[test]
fn planning_behavior_is_consistent_across_ecosystem_fixtures() {
	assert_simple_release_pattern(
		"../../fixtures/cargo/workspace",
		"core",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_simple_release_pattern(
		"../../fixtures/npm/workspace",
		"shared",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/deno/workspace",
		"shared",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/dart/workspace",
		"shared",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

#[test]
fn command_release_dry_run_is_consistent_across_ecosystem_fixtures() {
	assert_cli_release_pattern(
		"../../fixtures/cargo/workspace",
		"core",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_cli_release_pattern(
		"../../fixtures/npm/workspace",
		"shared",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/deno/workspace",
		"shared",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/dart/workspace",
		"shared",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

#[test]
fn quickstart_and_docs_reference_the_same_core_commands() {
	let docs_readme = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/readme.md");
	let quickstart =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../specs/001-first-step-port/quickstart.md");
	let docs_content =
		fs::read_to_string(docs_readme).unwrap_or_else(|error| panic!("docs readme: {error}"));
	let quickstart_content =
		fs::read_to_string(quickstart).unwrap_or_else(|error| panic!("quickstart: {error}"));

	for command in [
		"mc discover --format json",
		"mc change --package",
		"mc release --dry-run",
		"mc release",
		"lint:all",
		"test:all",
		"build:all",
		"build:book",
	] {
		assert!(docs_content.contains(command), "docs missing `{command}`");
		assert!(
			quickstart_content.contains(command),
			"quickstart missing `{command}`"
		);
	}
}

#[test]
fn configuration_guide_calls_out_current_implementation_limits() {
	let configuration_guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/04-configuration.md");
	let content = fs::read_to_string(configuration_guide)
		.unwrap_or_else(|error| panic!("configuration guide: {error}"));

	for expected in [
		"`defaults.include_private`",
		"`version_groups.strategy`",
		"`[ecosystems.*].enabled/roots/exclude`",
		"`package_overrides.changelog`",
		"`PrepareRelease`",
		"`Command`",
	] {
		assert!(
			content.contains(expected),
			"configuration guide missing `{expected}`"
		);
	}
}

#[test]
fn discovery_guide_describes_stable_relative_output_paths() {
	let discovery_guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/03-discovery.md");
	let content = fs::read_to_string(discovery_guide)
		.unwrap_or_else(|error| panic!("discovery guide: {error}"));

	assert!(
		content.contains("rendered relative to the repository root"),
		"discovery guide missing stable relative output note"
	);
}

#[test]
fn release_planning_guide_describes_release_cli_command_requirements() {
	let release_guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/06-release-planning.md");
	let content =
		fs::read_to_string(release_guide).unwrap_or_else(|error| panic!("release guide: {error}"));

	for expected in [
		"`mc release` is a config-defined top-level command.",
		"`[[package_overrides]]`",
		"`.changeset/*.md`",
		"`--dry-run`",
	] {
		assert!(
			content.contains(expected),
			"release planning guide missing `{expected}`"
		);
	}
}

fn assert_simple_release_pattern(
	relative_fixture_root: &str,
	change_reference: &str,
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());
	let changes_path = tempdir.path().join("changes.generated.md");
	fs::write(
		&changes_path,
		format!("---\n{change_reference}: minor\n---\n\n#### feature\n"),
	)
	.unwrap_or_else(|error| panic!("generated changes: {error}"));

	let discovery = discover_workspace(tempdir.path())
		.unwrap_or_else(|error| panic!("fixture discovery: {error}"));
	assert_eq!(discovery.packages.len(), 2);
	assert_eq!(discovery.dependencies.len(), 1);

	let plan = plan_release(tempdir.path(), &changes_path)
		.unwrap_or_else(|error| panic!("fixture release plan: {error}"));
	let direct = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains(direct_manifest_suffix))
		.unwrap_or_else(|| panic!("expected direct decision"));
	let dependent = plan
		.decisions
		.iter()
		.find(|decision| decision.package_id.contains(dependent_manifest_suffix))
		.unwrap_or_else(|| panic!("expected dependent decision"));

	assert_eq!(direct.recommended_bump.to_string(), "minor");
	assert_eq!(dependent.recommended_bump.to_string(), "patch");
}

fn assert_cli_release_pattern(
	relative_fixture_root: &str,
	change_reference: &str,
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());
	write_file(
		tempdir.path().join(".changeset/feature.md"),
		&format!("---\n{change_reference}: minor\n---\n\n#### feature\n"),
	);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("json: {error}"));
	let decisions = parsed["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("plan decisions"));
	let direct = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(direct_manifest_suffix))
		})
		.unwrap_or_else(|| panic!("expected direct release decision"));
	let dependent = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains(dependent_manifest_suffix))
		})
		.unwrap_or_else(|| panic!("expected dependent release decision"));

	assert_eq!(direct["bump"].as_str(), Some("minor"));
	assert_eq!(dependent["bump"].as_str(), Some("patch"));
}

fn copy_directory(source: &Path, destination: &Path) {
	fs::create_dir_all(destination)
		.unwrap_or_else(|error| panic!("create destination {}: {error}", destination.display()));
	for entry in fs::read_dir(source)
		.unwrap_or_else(|error| panic!("read dir {}: {error}", source.display()))
	{
		let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
		let source_path = entry.path();
		let destination_path = destination.join(entry.file_name());
		let file_type = entry
			.file_type()
			.unwrap_or_else(|error| panic!("file type {}: {error}", source_path.display()));
		if file_type.is_dir() {
			copy_directory(&source_path, &destination_path);
		} else if file_type.is_file() {
			if let Some(parent) = destination_path.parent() {
				fs::create_dir_all(parent)
					.unwrap_or_else(|error| panic!("create parent {}: {error}", parent.display()));
			}
			fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
				panic!(
					"copy {} -> {}: {error}",
					source_path.display(),
					destination_path.display()
				)
			});
		}
	}
}

fn seed_changeset_policy_fixture(root: &Path, with_changeset: bool) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(root.join("docs/readme.md"), "# docs\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.bot.changesets]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["crates/**"]
ignored_paths = ["docs/**", "*.md"]

[cli.changeset-check]
help_text = "Validate pull-request changeset policy"

[[cli.changeset-check.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.changeset-check.inputs]]
name = "changed_path"
type = "string_list"
required = true

[[cli.changeset-check.inputs]]
name = "label"
type = "string_list"

[[cli.changeset-check.steps]]
type = "EnforceChangesetPolicy"
"#,
	);
	if with_changeset {
		write_file(
			root.join(".changeset/feature.md"),
			r"---
core: patch
---

#### add feature
",
		);
	}
}

fn seed_release_fixture(root: &Path, command_step: Option<&str>, failing_changelog: bool) {
	let command_step = command_step.map_or_else(String::new, |command| {
		let escaped_command = command.replace('\\', "\\\\").replace('"', "\\\"");
		format!("\n[[cli.release.steps]]\ntype = \"Command\"\ncommand = \"{escaped_command}\"\nshell = true\n")
	});
	let app_changelog = if failing_changelog {
		write_file(root.join("blocked"), "not a directory");
		"blocked/changelog.md".to_string()
	} else {
		"crates/app/changelog.md".to_string()
	};
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
workflow-core = { path = "./crates/core", version = "1.0.0" }
workflow-app = { path = "./crates/app", version = "1.0.0" }
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		r#"
[package]
name = "workflow-core"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(
		root.join("crates/app/Cargo.toml"),
		r#"
[package]
name = "workflow-app"
version = { workspace = true }
edition = "2021"

[dependencies]
workflow-core = { workspace = true }
"#,
	);
	write_file(root.join("crates/core/changelog.md"), "# Changelog\n");
	if !failing_changelog {
		write_file(root.join("crates/app/changelog.md"), "# Changelog\n");
	}
	write_file(root.join("changelog.md"), "# Changelog\n");
	write_file(
		root.join("group.toml"),
		"[workspace.package]\nversion = \"1.0.0\"\n[workspace.dependencies]\nworkflow-core = { version = \"1.0.0\" }\n",
	);
	write_file(
		root.join("crates/core/extra.toml"),
		"[package]\nname = \"workflow-core\"\nversion = \"1.0.0\"\n",
	);
	write_file(
		root.join("monochange.toml"),
		&format!(
			r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = "{{path}}/changelog.md"

[package.core]
path = "crates/core"
versioned_files = ["crates/core/extra.toml"]

[package.app]
path = "crates/app"
changelog = "{app_changelog}"

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
versioned_files = ["group.toml"]
tag = true
release = true
version_format = "primary"

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
{command_step}
"#,
		),
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add release command
",
	);
}

fn seed_group_empty_update_message_fixture(root: &Path) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		r#"
[package]
name = "workflow-core"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(
		root.join("crates/app/Cargo.toml"),
		r#"
[package]
name = "workflow-app"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(root.join("crates/core/changelog.md"), "# Changelog\n");
	write_file(root.join("crates/app/changelog.md"), "# Changelog\n");
	write_file(root.join("changelog.md"), "# Changelog\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"
changelog = "{path}/changelog.md"
empty_update_message = "Default update for {package} -> {version}."

[package.core]
path = "crates/core"
empty_update_message = "Package override for {package} -> {version}"

[package.app]
path = "crates/app"

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
empty_update_message = "Update triggered by group {group}; version {version}."
tag = true
release = true
version_format = "primary"

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.steps]]
type = "PrepareRelease"
"#,
	);
	write_file(
		root.join(".changeset/group-release.md"),
		"---\nsdk: patch\n---\n",
	);
}

fn write_file(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}
	fs::write(path, content)
		.unwrap_or_else(|error| panic!("write file {}: {error}", path.display()));
}
