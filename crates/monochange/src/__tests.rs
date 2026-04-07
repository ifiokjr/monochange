use std::ffi::OsString;
use std::fs;
use std::path::Path;

use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::Ecosystem;
use tempfile::tempdir;

use crate::add_change_file;
use crate::add_interactive_change_file;
use crate::affected_packages;
use crate::build_command_for_root;
use crate::discover_workspace;
use crate::interactive::InteractiveChangeResult;
use crate::interactive::InteractiveTarget;
use crate::parse_change_bump;
use crate::plan_release;
use crate::render_change_target_markdown;
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
	assert!(output.contains("assist"));
	assert!(output.contains("mcp"));
	assert!(output.contains("change"));
	assert!(output.contains("diagnostics"));
}

#[test]
fn assist_command_prints_install_and_mcp_guidance() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("assist"),
			OsString::from("pi"),
		],
	)
	.unwrap_or_else(|error| panic!("assist output: {error}"));

	assert!(output.contains("@monochange/cli"));
	assert!(output.contains("@monochange/skill"));
	assert!(output.contains("monochange mcp"));
	assert!(output.contains("mc validate"));
}

#[test]
fn assist_command_supports_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("assist"),
			OsString::from("generic"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("assist json output: {error}"));

	assert!(output.contains("\"mcp_config\""));
	assert!(output.contains("\"install\""));
	assert!(output.contains("@monochange/skill"));
}

#[test]
fn init_writes_detected_packages_groups_and_default_cli_commands() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/init-scan", tempdir.path());

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
	copy_fixture("monochange/init-existing-config", tempdir.path());

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
	copy_fixture("monochange/validate-workspace", tempdir.path());
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
	copy_fixture("monochange/validate-invalid-changeset", tempdir.path());
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
	assert!(content.contains("# feature foundation"));
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

	assert!(!content.contains("type:"));
	assert!(content.contains("sdk-core: security"));
	assert!(content.contains("# rotate signing keys"));
	assert!(content.contains("Roll the signing key before the release window closes."));
}

#[test]
fn changes_add_canonicalizes_package_references_to_package_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/canonicalize-package-ref", tempdir.path());
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
	let content = fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("monochange: patch"));
	assert!(!content.contains("crates/monochange: patch"));
}

#[test]
fn add_change_file_creates_default_path_under_changeset_directory() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let output_path = add_change_file(
		&fixture_root,
		&["sdk-core".to_string()],
		BumpSeverity::Patch,
		None,
		"default output",
		None,
		None,
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
fn collect_cli_command_inputs_omits_default_bump_for_type_only_changes() {
	let root = fixture_path("changeset-target-metadata/cli-type-only-change");
	let command = build_command_for_root("mc", &root);
	let matches = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("docs"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected subcommand"));
	let cli_command = monochange_core::default_cli_commands()
		.into_iter()
		.find(|command| command.name == "change")
		.unwrap_or_else(|| panic!("expected change command"));
	let inputs = crate::collect_cli_command_inputs(&cli_command, subcommand_matches);
	assert!(inputs.get("bump").is_some_and(Vec::is_empty));
	assert_eq!(
		inputs
			.get("type")
			.and_then(|values| values.first())
			.map(String::as_str),
		Some("docs")
	);
}

#[test]
fn parse_change_bump_supports_none() {
	assert_eq!(parse_change_bump("none").unwrap(), crate::ChangeBump::None);
	assert_eq!(
		BumpSeverity::from(crate::ChangeBump::None),
		BumpSeverity::None
	);
}

#[test]
fn changes_add_requires_package_or_interactive_mode() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture(
		"changeset-target-metadata/cli-type-only-change",
		tempdir.path(),
	);

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--reason"),
			OsString::from("missing package"),
		],
	)
	.expect_err("change without package should fail");
	assert!(error
		.to_string()
		.contains("requires at least one `--package` value or `--interactive` mode"));
}

#[test]
fn changes_add_defaults_bump_to_none_when_type_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture(
		"changeset-target-metadata/cli-type-only-change",
		tempdir.path(),
	);
	let output_path = tempdir.path().join("docs.md");

	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("docs"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
			OsString::from("--output"),
			output_path.clone().into_os_string(),
		],
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core: docs"));
	assert!(!content.contains("bump:"));
}

#[test]
fn add_change_file_renders_type_and_explicit_version_without_bump() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("security-version.md");

	add_change_file(
		tempdir.path(),
		&["core".to_string()],
		BumpSeverity::None,
		Some("2.0.0"),
		"pin a secure release",
		Some("security"),
		None,
		Some(&output_path),
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core:"));
	assert!(!content.contains("  bump:"));
	assert!(content.contains("  type: security"));
	assert!(content.contains("  version: \"2.0.0\""));
}

#[test]
fn add_change_file_renders_object_metadata_when_bump_differs_from_type_default() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("security-major.md");

	add_change_file(
		tempdir.path(),
		&["core".to_string()],
		BumpSeverity::Major,
		None,
		"break the api",
		Some("security"),
		None,
		Some(&output_path),
	)
	.unwrap_or_else(|error| panic!("change file output: {error}"));

	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("core:"));
	assert!(content.contains("  bump: major"));
	assert!(content.contains("  type: security"));
}

#[test]
fn add_change_file_rejects_none_without_type_or_version() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());

	let error = add_change_file(
		tempdir.path(),
		&["core".to_string()],
		BumpSeverity::None,
		None,
		"type required",
		None,
		None,
		None,
	)
	.expect_err("none bump without type/version should fail");
	assert!(error
		.to_string()
		.contains("must not use a `none` bump without also declaring `type` or `version`"));
}

#[test]
fn add_change_file_rejects_unknown_change_type() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());

	let error = add_change_file(
		tempdir.path(),
		&["core".to_string()],
		BumpSeverity::Patch,
		None,
		"unknown type",
		Some("docs"),
		None,
		None,
	)
	.expect_err("unknown type should fail");
	assert!(error
		.to_string()
		.contains("uses unknown change type `docs`"));
}

#[test]
fn render_change_target_markdown_uses_package_defaults() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"core",
		BumpSeverity::Patch,
		None,
		Some("security"),
	)
	.unwrap_or_else(|error| panic!("render target: {error}"));
	assert_eq!(lines, vec!["core: security".to_string()]);
}

#[test]
fn render_change_target_markdown_uses_group_defaults() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let lines = render_change_target_markdown(
		&configuration,
		"sdk",
		BumpSeverity::Minor,
		None,
		Some("test"),
	)
	.unwrap_or_else(|error| panic!("render target: {error}"));
	assert_eq!(lines, vec!["sdk: test".to_string()]);
}

#[test]
fn change_type_default_bump_resolves_package_group_and_unknown_targets() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	assert_eq!(
		crate::change_type_default_bump(&configuration, "core", "security"),
		Some(BumpSeverity::Patch)
	);
	assert_eq!(
		crate::change_type_default_bump(&configuration, "sdk", "test"),
		Some(BumpSeverity::Minor)
	);
	assert_eq!(
		crate::change_type_default_bump(&configuration, "unknown", "test"),
		None
	);
}

#[test]
fn add_interactive_change_file_writes_target_owned_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("changeset-target-metadata/render-workspace", tempdir.path());
	let output_path = tempdir.path().join("interactive.md");
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "sdk".to_string(),
			bump: BumpSeverity::Minor,
			version: None,
			change_type: Some("test".to_string()),
		}],
		reason: "broaden integration coverage".to_string(),
		details: Some("Exercise the group-authored shorthand path.".to_string()),
	};

	add_interactive_change_file(tempdir.path(), &result, Some(&output_path))
		.unwrap_or_else(|error| panic!("interactive change file: {error}"));
	let content = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(content.contains("sdk: test"));
	assert!(content.contains("#### broaden integration coverage"));
}

#[test]
fn changes_add_rejects_legacy_evidence_input() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = tempdir.path().join("major.md");

	let error = run_cli(
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
			output_path.into_os_string(),
		],
	)
	.expect_err("legacy evidence input should be rejected");

	assert!(error
		.to_string()
		.contains("unexpected argument '--evidence'"));
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
fn release_dry_run_rejects_legacy_origin_and_evidence_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	copy_fixture("monochange/release-with-compat-evidence", tempdir.path());
	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.expect_err("legacy metadata should be rejected");

	assert!(error
		.to_string()
		.contains("target `origin` uses unsupported field(s): core"));
}

#[test]
fn plan_release_expands_group_targeted_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	copy_fixture("monochange/release-with-group-change", tempdir.path());
	let plan = plan_release(tempdir.path(), &tempdir.path().join("group-change.md"))
		.unwrap_or_else(|error| panic!("group release plan: {error}"));
	let direct_count = plan
		.decisions
		.iter()
		.filter(|d| d.recommended_bump.to_string() == "minor")
		.count();
	assert_eq!(direct_count, 2);
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
fn render_interactive_changeset_markdown_uses_natural_summary_heading() {
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "core".to_string(),
			bump: monochange_core::BumpSeverity::Minor,
			version: None,
			change_type: None,
		}],
		reason: "interactive heading".to_string(),
		details: Some("Details body".to_string()),
	};
	let rendered = crate::render_interactive_changeset_markdown(&result);
	assert!(rendered.contains("# interactive heading"));
	assert!(rendered.contains("Details body"));
}

#[test]
fn command_release_normalizes_authored_changeset_heading_levels() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-heading-normalization", tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/changelog.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));

	assert!(core_changelog.contains("#### add release command"));
	assert!(core_changelog.contains("##### Details"));
	assert!(core_changelog.contains("###### API changes"));
	assert!(!core_changelog.contains("\n## Details\n"));
	assert!(!core_changelog.contains("\n### API changes\n"));
}

#[test]
fn command_release_updates_manifests_changelogs_and_deletes_changesets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_fixture(
		tempdir.path(),
		Some("printf '%s' \"{{ version }}\" > release-version.txt"),
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
	assert!(group_changelog.contains("Changed members: core"));
	assert!(group_changelog.contains("Synchronized members: app"));
	assert!(group_versioned_file.contains("version = \"1.1.0\""));
	assert!(package_versioned_file.contains("version = \"1.1.0\""));
	assert_eq!(release_version, "1.1.0");
	assert!(!tempdir.path().join(".changeset/feature.md").exists());
}

#[test]
fn command_release_auto_discovers_and_updates_cargo_lockfiles() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_cargo_lock_release_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let cargo_lock = fs::read_to_string(tempdir.path().join("Cargo.lock"))
		.unwrap_or_else(|error| panic!("cargo lock: {error}"));

	assert!(cargo_lock.contains("version = \"1.1.0\""));
}

#[test]
fn command_release_auto_discovers_and_updates_package_lockfiles() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_npm_lock_release_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let package_lock = fs::read_to_string(tempdir.path().join("packages/app/package-lock.json"))
		.unwrap_or_else(|error| panic!("package lock: {error}"));

	assert!(package_lock.contains("\"version\": \"1.1.0\""));
}

#[test]
fn command_release_auto_discovers_and_updates_bun_lockb_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_bun_lockb_release_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let bun_lock = fs::read(tempdir.path().join("packages/app/bun.lockb"))
		.unwrap_or_else(|error| panic!("bun lockb: {error}"));

	assert!(String::from_utf8_lossy(&bun_lock).contains("1.1.0"));
}

#[test]
fn command_release_auto_discovers_and_updates_deno_lockfiles() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_deno_lock_release_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let deno_lock = fs::read_to_string(tempdir.path().join("packages/app/deno.lock"))
		.unwrap_or_else(|error| panic!("deno lock: {error}"));

	assert!(deno_lock.contains("npm:app@1.1.0"));
}

#[test]
fn command_release_honors_explicit_lockfile_paths_in_versioned_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_explicit_lockfile_override_fixture(tempdir.path());

	run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let shared_lock = fs::read_to_string(tempdir.path().join("lockfiles/shared/package-lock.json"))
		.unwrap_or_else(|error| panic!("shared package lock: {error}"));

	assert!(shared_lock.contains("\"version\": \"1.1.0\""));
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
fn command_diagnostics_reports_requested_changeset_text() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("diagnostics"),
			OsString::from("--changeset"),
			OsString::from(".changeset/feature.md"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("changeset: .changeset/feature.md"));
	assert!(output.contains("summary: Add feature"));
	assert!(output.contains("targets:"));
	assert!(output.contains("core"));
}

#[test]
fn command_diagnostics_reports_multiple_changesets_in_json() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), true);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("diagnostics"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("diagnostics json: {error}"));

	let requested = parsed["requestedChangesets"]
		.as_array()
		.unwrap_or_else(|| panic!("requested"));
	assert_eq!(requested.len(), 2);
	assert_eq!(requested[0].as_str(), Some(".changeset/feature.md"));
	assert_eq!(requested[1].as_str(), Some(".changeset/performance.md"));
	let changesets = parsed["changesets"]
		.as_array()
		.unwrap_or_else(|| panic!("changesets"));
	assert_eq!(changesets.len(), 2);
	assert_eq!(changesets[0]["targets"][0]["id"], "core");
}

#[test]
fn command_diagnostics_deduplicates_duplicate_requested_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("diagnostics"),
			OsString::from("--changeset"),
			OsString::from(".changeset/feature.md"),
			OsString::from("--changeset"),
			OsString::from(".changeset/feature.md"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let parsed: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("diagnostics json: {error}"));
	let requested = parsed["requestedChangesets"]
		.as_array()
		.unwrap_or_else(|| panic!("requested"));

	assert_eq!(requested.len(), 1);
	assert_eq!(requested[0].as_str(), Some(".changeset/feature.md"));
}

#[test]
fn command_diagnostics_reports_unknown_changeset_path() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let error = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("diagnostics"),
			OsString::from("--changeset"),
			OsString::from(".changeset/does-not-exist.md"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing changeset failure"));

	assert!(error.to_string().contains("does not exist"));
}

#[test]
fn command_diagnostics_resolves_changeset_fallback_for_short_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("diagnostics"),
			OsString::from("--changeset"),
			OsString::from("feature.md"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("changeset: .changeset/feature.md"));
}

#[test]
fn command_diagnostics_supports_absolute_changeset_path() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);
	let absolute = tempdir.path().join(".changeset/feature.md");

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("diagnostics"),
			OsString::from("--format"),
			OsString::from("text"),
			OsString::from("--changeset"),
			OsString::from(absolute.to_string_lossy().into_owned()),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(output.contains("changeset: .changeset/feature.md"));
}

#[test]
fn resolve_changeset_path_accepts_short_names_with_fallback_to_changeset_dir() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let absolute = tempdir.path().join(".changeset/feature.md");
	let from_short = crate::resolve_changeset_path(tempdir.path(), "feature.md")
		.unwrap_or_else(|error| panic!("resolve short path: {error}"));
	let from_absolute =
		crate::resolve_changeset_path(tempdir.path(), absolute.to_string_lossy().as_ref())
			.unwrap_or_else(|error| panic!("resolve absolute path: {error}"));

	assert_eq!(from_short, absolute);
	assert_eq!(from_absolute, absolute);
}

#[test]
fn resolve_changeset_path_rejects_invalid_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_diagnostics_fixture(tempdir.path(), false);

	let missing = crate::resolve_changeset_path(tempdir.path(), ".changeset/does-not-exist.md")
		.err()
		.unwrap_or_else(|| panic!("expected missing path failure"));
	assert!(missing.to_string().contains("does not exist"));

	let empty = crate::resolve_changeset_path(tempdir.path(), "")
		.err()
		.unwrap_or_else(|| panic!("expected empty path failure"));
	assert!(empty.to_string().contains("cannot be empty"));
}

#[test]
fn render_changeset_diagnostics_reports_empty_set() {
	let report = crate::ChangesetDiagnosticsReport {
		requested_changesets: Vec::new(),
		changesets: Vec::new(),
	};
	let rendered = crate::render_changeset_diagnostics(&report);

	assert_eq!(rendered, "no matching changesets found");
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
	copy_fixture("monochange/shell-command", tempdir.path());
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
	copy_fixture("monochange/dry-run-override", tempdir.path());
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
fn cli_command_command_steps_expose_namespaced_inputs_and_step_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/namespaced-inputs", tempdir.path());
	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("announce"),
			OsString::from("--message"),
			OsString::from("hello-world"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	let command_output = fs::read_to_string(tempdir.path().join("command-output.txt"))
		.unwrap_or_else(|error| panic!("command output file: {error}"));
	assert_eq!(command_output, "hello-world");
}

#[test]
fn affected_packages_requires_attached_coverage_for_changed_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	let evaluation = affected_packages(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&Vec::new(),
	)
	.unwrap_or_else(|error| panic!("verification: {error}"));

	assert_eq!(
		evaluation.status,
		monochange_core::ChangesetPolicyStatus::Failed
	);
	assert!(evaluation.required);
	assert!(evaluation.comment.is_some());
	assert_eq!(evaluation.matched_paths, vec!["crates/core/src/lib.rs"]);
	assert_eq!(evaluation.uncovered_package_ids, vec!["core"]);
}

#[test]
fn affected_packages_skips_when_allowed_label_is_present() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	let evaluation = affected_packages(
		tempdir.path(),
		&["crates/core/src/lib.rs".to_string()],
		&["no-changeset-required".to_string()],
	)
	.unwrap_or_else(|error| panic!("verification: {error}"));

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
fn affected_packages_step_can_override_built_in_inputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/affected-step-override", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("pr-check"),
			OsString::from("--paths"),
			OsString::from("crates/core/src/lib.rs"),
			OsString::from("--labels"),
			OsString::from("no-changeset-required"),
			OsString::from("--enforce"),
		],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(output.contains("changeset policy: skipped"));
	assert!(output.contains("matched skip labels: no-changeset-required"));
}

#[test]
fn planning_behavior_is_consistent_across_ecosystem_fixtures() {
	assert_simple_release_pattern(
		"../../fixtures/cargo/workspace",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_simple_release_pattern(
		"../../fixtures/npm/workspace",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/deno/workspace",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_simple_release_pattern(
		"../../fixtures/dart/workspace",
		"packages/shared/pubspec.yaml",
		"packages/app/pubspec.yaml",
	);
}

#[test]
fn source_github_release_comments_command_supports_provider_neutral_source_config() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/source/github");
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("release-comments"),
			OsString::from("--dry-run"),
			OsString::from("--format"),
			OsString::from("json"),
		],
	)
	.unwrap_or_else(|error| panic!("release comments output: {error}"));

	assert!(output.contains("released_packages") || output.contains("releaseTargets"));
}

#[test]
fn command_release_dry_run_is_consistent_across_ecosystem_fixtures() {
	assert_cli_release_pattern(
		"../../fixtures/cargo/workspace",
		"crates/core/Cargo.toml",
		"crates/app/Cargo.toml",
	);
	assert_cli_release_pattern(
		"../../fixtures/npm/workspace",
		"packages/shared/package.json",
		"packages/web/package.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/deno/workspace",
		"packages/shared/deno.json",
		"packages/tool/deno.json",
	);
	assert_cli_release_pattern(
		"../../fixtures/dart/workspace",
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
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());
	let changes_path = tempdir.path().join(".changeset/feature.md");

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
	direct_manifest_suffix: &str,
	dependent_manifest_suffix: &str,
) {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative_fixture_root);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

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

fn seed_diagnostics_fixture(root: &Path, with_second_changeset: bool) {
	if with_second_changeset {
		copy_fixture("monochange/diagnostics-two-changesets", root);
	} else {
		copy_fixture("monochange/diagnostics-base", root);
	}
}

fn seed_changeset_policy_fixture(root: &Path, _with_changeset: bool) {
	copy_fixture("monochange/changeset-policy-base", root);
}

fn seed_release_fixture(root: &Path, command_step: Option<&str>, failing_changelog: bool) {
	if failing_changelog {
		copy_fixture("monochange/release-failing-changelog", root);
	} else if command_step.is_some() {
		copy_fixture("monochange/release-with-command-step", root);
	} else {
		copy_fixture("monochange/release-base", root);
	}
	let _ = command_step;
}

fn seed_cargo_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/cargo-lock-release", root);
}

fn seed_npm_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/npm-lock-release", root);
}

fn seed_bun_lockb_release_fixture(root: &Path) {
	copy_fixture("monochange/bun-lock-release", root);
}

fn seed_deno_lock_release_fixture(root: &Path) {
	copy_fixture("monochange/deno-lock-release", root);
}

fn seed_explicit_lockfile_override_fixture(root: &Path) {
	copy_fixture("monochange/explicit-lockfile-override", root);
}

fn seed_group_empty_update_message_fixture(root: &Path) {
	copy_fixture("monochange/group-empty-update-message", root);
}

#[test]
fn validate_rejects_workspace_versioned_packages_in_different_groups() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-different-groups");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	let rendered = error.to_string();

	assert!(rendered.contains("version.workspace = true"));
	assert!(rendered.contains("same version group"));
}

#[test]
fn validate_rejects_workspace_versioned_packages_not_in_any_group() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-ungrouped");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let error = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.err()
	.unwrap_or_else(|| panic!("expected validation failure"));
	let rendered = error.to_string();

	assert!(rendered.contains("version.workspace = true"));
	assert!(rendered.contains("same version group"));
	assert!(rendered.contains("not in any group"));
}

#[test]
fn validate_accepts_workspace_versioned_packages_in_same_group() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-same-group");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(output.contains("workspace validation passed"));
}

#[test]
fn validate_accepts_single_workspace_versioned_package_without_group() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-single");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("validate")],
	)
	.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(output.contains("workspace validation passed"));
}

#[test]
fn command_step_with_id_captures_stdout_for_later_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("echo-test")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("got:hello-world"),
		"expected stdout interpolation, got: {output}"
	);
}

#[test]
fn command_step_with_shell_string_uses_custom_shell() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("shell-bash")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("result:bash-works"),
		"expected bash shell output, got: {output}"
	);
}

#[test]
fn release_step_exposes_updated_changelogs_to_command_steps() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_step_outputs_fixture(tempdir.path());

	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("release")],
	)
	.unwrap_or_else(|error| panic!("command output: {error}"));

	assert!(
		output.contains("crates/core/CHANGELOG.md"),
		"expected changelog path in output, got: {output}"
	);
}

fn seed_step_outputs_fixture(root: &Path) {
	let fixture_dir =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/tests/step-outputs/base");
	let files = [
		"Cargo.toml",
		"monochange.toml",
		"crates/core/Cargo.toml",
		"crates/core/src/lib.rs",
		"crates/core/CHANGELOG.md",
		".changeset/feature.md",
	];
	for file in &files {
		let source = fixture_dir.join(file);
		let target = root.join(file);
		if let Some(parent) = target.parent() {
			fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
		}
		fs::copy(&source, &target)
			.unwrap_or_else(|error| panic!("copy {}: {error}", source.display()));
	}
}

fn fixture_path(relative: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn copy_fixture(fixture_relative: &str, dest: &Path) {
	copy_directory(&fixture_path(fixture_relative), dest);
}

#[test]
fn step_override_with_literal_list_uses_list_as_changed_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/step-override-literal-list", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("pr-check")],
	)
	.unwrap_or_else(|error| panic!("pr-check output: {error}"));
	let json: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("json: {error}; output={output}"));
	assert_eq!(json["affectedPackageIds"][0], "core");
}

#[test]
fn step_override_with_non_direct_jinja_template_renders_correctly() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/step-override-jinja", tempdir.path());
	run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("announce"),
			OsString::from("--prefix"),
			OsString::from("goodbye"),
		],
	)
	.unwrap_or_else(|error| panic!("announce output: {error}"));
	let output = fs::read_to_string(tempdir.path().join("output.txt"))
		.unwrap_or_else(|error| panic!("output file: {error}"));
	assert_eq!(output, "goodbye-world");
}

#[test]
fn step_override_forwards_multi_value_list_reference_as_list() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/step-override-multi-list", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[
			OsString::from("mc"),
			OsString::from("pr-check"),
			OsString::from("--paths"),
			OsString::from("crates/core/src/lib.rs"),
			OsString::from("--paths"),
			OsString::from("docs/readme.md"),
		],
	)
	.unwrap_or_else(|error| panic!("pr-check output: {error}"));
	let json: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("json: {error}; output={output}"));
	assert!(json["matchedPaths"]
		.as_array()
		.is_some_and(|p| p.iter().any(|v| v == "crates/core/src/lib.rs")));
}

#[test]
fn step_override_missing_template_reference_produces_empty_changed_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/changeset-policy-base", tempdir.path());
	copy_fixture("monochange/step-override-missing-ref", tempdir.path());
	let output = run_cli(
		tempdir.path(),
		[OsString::from("mc"), OsString::from("pr-check")],
	)
	.unwrap_or_else(|error| panic!("pr-check output: {error}"));
	let json: serde_json::Value = serde_json::from_str(&output)
		.unwrap_or_else(|error| panic!("json: {error}; output={output}"));
	assert_eq!(json["affectedPackageIds"].as_array().map(Vec::len), Some(0));
}

// Unit tests for private helper functions in the input-override pipeline.
// These tests cover branches not reached by integration paths (e.g.
// Null/Number/Object JSON types, invalid template reference forms).

#[test]
fn parse_direct_template_reference_returns_inner_path_for_valid_refs() {
	use super::parse_direct_template_reference;
	assert_eq!(
		parse_direct_template_reference("{{ inputs.message }}"),
		Some("inputs.message")
	);
	assert_eq!(
		parse_direct_template_reference("  {{ foo.bar_baz }}  "),
		Some("foo.bar_baz")
	);
}

#[test]
fn parse_direct_template_reference_returns_none_for_empty_inner() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("{{  }}"), None);
	assert_eq!(parse_direct_template_reference("{{}}"), None);
}

#[test]
fn parse_direct_template_reference_returns_none_for_invalid_chars() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("{{ foo-bar }}"), None);
	assert_eq!(parse_direct_template_reference("{{ foo bar }}"), None);
}

#[test]
fn parse_direct_template_reference_returns_none_when_not_a_bare_ref() {
	use super::parse_direct_template_reference;
	assert_eq!(parse_direct_template_reference("prefix-{{ foo }}"), None);
	assert_eq!(parse_direct_template_reference("hello world"), None);
}

#[test]
fn lookup_template_value_traverses_nested_objects() {
	use super::lookup_template_value;
	use serde_json::json;
	let v = json!({"inputs": {"message": "hello"}});
	assert_eq!(
		lookup_template_value(&v, "inputs.message"),
		Some(&json!("hello"))
	);
}

#[test]
fn lookup_template_value_traverses_array_by_index() {
	use super::lookup_template_value;
	use serde_json::json;
	let v = json!({"items": ["a", "b", "c"]});
	assert_eq!(lookup_template_value(&v, "items.1"), Some(&json!("b")));
}

#[test]
fn lookup_template_value_returns_none_for_missing_key() {
	use super::lookup_template_value;
	use serde_json::json;
	let v = json!({"inputs": {}});
	assert_eq!(lookup_template_value(&v, "inputs.missing"), None);
}

#[test]
fn lookup_template_value_returns_none_for_primitive_descent() {
	use super::lookup_template_value;
	use serde_json::json;
	let v = json!({"foo": "string_value"});
	assert_eq!(lookup_template_value(&v, "foo.nested"), None);
}

#[test]
fn template_value_to_input_values_null_returns_empty() {
	use super::template_value_to_input_values;
	assert_eq!(
		template_value_to_input_values(&serde_json::Value::Null),
		Vec::<String>::new()
	);
}

#[test]
fn template_value_to_input_values_number_returns_string() {
	use super::template_value_to_input_values;
	use serde_json::json;
	assert_eq!(
		template_value_to_input_values(&json!(42)),
		vec!["42".to_string()]
	);
	assert_eq!(
		template_value_to_input_values(&json!(1.5)),
		vec!["1.5".to_string()]
	);
}

#[test]
fn template_value_to_input_values_array_flattens_elements() {
	use super::template_value_to_input_values;
	use serde_json::json;
	assert_eq!(
		template_value_to_input_values(&json!(["a", "b", "c"])),
		vec!["a", "b", "c"]
	);
	assert_eq!(
		template_value_to_input_values(&json!([true, false])),
		vec!["true", "false"]
	);
}

#[test]
fn template_value_to_input_values_object_returns_json_serialization() {
	use super::template_value_to_input_values;
	use serde_json::json;
	let obj = json!({"k": "v"});
	let result = template_value_to_input_values(&obj);
	assert_eq!(result.len(), 1);
	assert!(result[0].contains('"'));
}
