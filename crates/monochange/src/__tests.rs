use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::MockServer;
use monochange_config::load_workspace_configuration;
use monochange_core::BumpSeverity;
use monochange_core::ChangesetTargetKind;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::Ecosystem;
use monochange_core::GroupChangelogInclude;
use monochange_core::PreparedChangesetTarget;
use monochange_core::VersionFormat;
use semver::Version;
use tempfile::tempdir;

#[allow(dead_code)]
#[path = "../../../testing/test_support/fs.rs"]
mod shared_fs_test_support;
use shared_fs_test_support::copy_directory;
use shared_fs_test_support::current_test_name;
use shared_fs_test_support::fixture_path;
use shared_fs_test_support::setup_fixture;
use shared_fs_test_support::setup_scenario_workspace;

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
use crate::CliContext;

fn run_cli<I>(root: &Path, args: I) -> monochange_core::MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	run_with_args_in_dir("mc", args, root)
}

#[test]
fn shared_fs_test_support_current_test_name_covers_plain_and_case_prefixes() {
	assert_eq!(
		current_test_name(),
		"shared_fs_test_support_current_test_name_covers_plain_and_case_prefixes"
	);
	let named = std::thread::Builder::new()
		.name("case_1_current_test_name_thread".to_string())
		.spawn(current_test_name)
		.unwrap_or_else(|error| panic!("spawn thread: {error}"))
		.join()
		.unwrap_or_else(|error| panic!("join thread: {error:?}"));
	assert_eq!(named, "current_test_name_thread");
}

#[test]
fn shared_fs_test_support_setup_fixture_copies_fixture_files() {
	let tempdir = setup_fixture("test-support/setup-fixture");
	assert_eq!(
		fs::read_to_string(tempdir.path().join("nested/child.txt"))
			.unwrap_or_else(|error| panic!("read nested fixture: {error}")),
		"nested child\n"
	);
}

#[test]
fn shared_fs_test_support_setup_scenario_workspace_prefers_workspace_and_skips_expected() {
	let tempdir = setup_scenario_workspace("test-support/scenario-workspace");
	assert_eq!(
		fs::read_to_string(tempdir.path().join("workspace-only.txt"))
			.unwrap_or_else(|error| panic!("read workspace fixture: {error}")),
		"workspace marker\n"
	);
	assert!(!tempdir.path().join("scenario-root-only.txt").exists());
	assert!(!tempdir.path().join("expected").exists());
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
	assert!(output.contains("commit-release"));
	assert!(output.contains("diagnostics"));
	assert!(output.contains("repair-release"));
	assert!(output.contains("release-record"));
}

#[test]
fn commit_release_help_describes_local_commit_workflow() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("commit-release"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("commit-release help: {error}"));

	assert!(output.contains("Prepare a release and create a local release commit"));
	assert!(output.contains("mc commit-release --dry-run --format json"));
	assert!(output.contains("Embeds a durable release record block in the commit body"));
}

#[test]
fn repair_release_help_describes_retargeting_workflow() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("repair-release"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("repair-release help: {error}"));

	assert!(output.contains("Repair a recent release by moving its release tags to a later commit"));
	assert!(output.contains("mc repair-release --from v1.2.3 --dry-run"));
	assert!(output.contains("--sync-provider=false"));
}

#[test]
fn boolean_cli_inputs_support_explicit_false_values() {
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	let matches = build_command_for_root("mc", &fixture_root)
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("repair-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
			OsString::from("--sync-provider=false"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected repair-release subcommand"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("sync_provider")
			.map(String::as_str),
		Some("false")
	);
}

#[test]
fn release_record_help_describes_first_parent_discovery() {
	let output = run_with_args(
		"mc",
		[
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--help"),
		],
	)
	.unwrap_or_else(|error| panic!("release-record help: {error}"));

	assert!(
		output.contains("Inspect the monochange release record associated with a tag or commit")
	);
	assert!(output.contains("mc release-record --from v1.2.3"));
	assert!(
		output.contains("Walks first-parent ancestry until it finds a monochange release record")
	);
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
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");
	copy_directory(&fixture_root, tempdir.path());
	let output_path = add_change_file(
		tempdir.path(),
		crate::AddChangeFileRequest::builder()
			.package_refs(&["sdk-core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("default output")
			.build(),
	)
	.unwrap_or_else(|error| panic!("default change file: {error}"));
	let content = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read change file: {error}"));

	assert!(output_path.starts_with(tempdir.path().join(".changeset")));
	assert!(content.contains("sdk-core: patch"));
	fs::remove_file(output_path).unwrap_or_else(|error| panic!("cleanup change file: {error}"));
}

#[test]
fn change_command_sources_type_choices_from_workspace_configuration() {
	let root = fixture_path("changeset-target-metadata/cli-type-only-change");
	let error = build_command_for_root("mc", &root)
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("change"),
			OsString::from("--package"),
			OsString::from("core"),
			OsString::from("--type"),
			OsString::from("security"),
			OsString::from("--reason"),
			OsString::from("clarify migration guide"),
		])
		.expect_err("unknown configured type should be rejected by clap choices");
	let rendered = error.to_string();
	assert!(rendered.contains("invalid value 'security'"));
	assert!(rendered.contains("[possible values: docs, test]"));
}

#[test]
fn build_command_for_root_falls_back_to_default_cli_when_config_load_fails() {
	let root = fixture_path("config/rejects-unknown-template-vars");
	let matches = build_command_for_root("mc", &root)
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("repair-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	assert_eq!(matches.subcommand_name(), Some("repair-release"));
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
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::None)
			.reason("pin a secure release")
			.version(Some("2.0.0"))
			.change_type(Some("security"))
			.output(Some(&output_path))
			.build(),
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
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Major)
			.reason("break the api")
			.change_type(Some("security"))
			.output(Some(&output_path))
			.build(),
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
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::None)
			.reason("type required")
			.build(),
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
		crate::AddChangeFileRequest::builder()
			.package_refs(&["core".to_string()])
			.bump(BumpSeverity::Patch)
			.reason("unknown type")
			.change_type(Some("docs"))
			.build(),
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
	assert!(content.contains("# broaden integration coverage"));
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
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration =
		load_workspace_configuration(&root).unwrap_or_else(|error| panic!("config: {error}"));
	let result = InteractiveChangeResult {
		targets: vec![InteractiveTarget {
			id: "core".to_string(),
			bump: BumpSeverity::Minor,
			version: None,
			change_type: None,
		}],
		reason: "interactive heading".to_string(),
		details: Some("Details body".to_string()),
	};
	let rendered = crate::render_interactive_changeset_markdown(&configuration, &result)
		.unwrap_or_else(|error| panic!("render interactive markdown: {error}"));
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
		"`RetargetRelease`",
		"`Command`",
	] {
		assert!(
			content.contains(expected),
			"configuration guide missing `{expected}`"
		);
	}
}

#[test]
fn repairable_releases_guide_distinguishes_manifest_and_release_record() {
	let guide = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../docs/src/guide/12-repairable-releases.md");
	let content = fs::read_to_string(guide)
		.unwrap_or_else(|error| panic!("repairable releases guide: {error}"));

	for expected in [
		"manifest = \"what monochange is preparing right now\"",
		"release record = \"what this release commit historically declared\"",
		"`ReleaseRecord` does **not** replace `RenderReleaseManifest`",
		"mc release-record --from v1.2.3",
		"mc repair-release --from v1.2.3 --target HEAD --dry-run",
		"Prefer publishing a new patch release",
	] {
		assert!(
			content.contains(expected),
			"repairable releases guide missing `{expected}`"
		);
	}
}

#[test]
fn github_automation_guide_mentions_release_repair_and_dry_run() {
	let guide =
		Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/src/guide/08-github-automation.md");
	let content = fs::read_to_string(guide)
		.unwrap_or_else(|error| panic!("github automation guide: {error}"));

	for expected in [
		"mc release-record --from v1.2.3",
		"mc repair-release --from v1.2.3 --target HEAD --dry-run",
		"Use `--dry-run` first for `repair-release`",
	] {
		assert!(
			content.contains(expected),
			"github automation guide missing `{expected}`"
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

#[test]
fn build_release_commit_message_includes_release_record_body() {
	let source = sample_source_configuration_for_release_commit();
	let manifest = sample_release_manifest_for_commit_message(true, true);

	let commit_message = crate::build_release_commit_message(Some(&source), &manifest);
	assert_eq!(commit_message.subject, "chore(release): prepare release");
	let body = commit_message
		.body
		.unwrap_or_else(|| panic!("expected commit body"));
	assert!(body.contains("Prepare release."));
	assert!(body.contains("- release targets: sdk (1.2.3)"));
	assert!(body.contains("- released packages: monochange, monochange_core"));
	assert!(body.contains("- updated changelogs: crates/monochange/CHANGELOG.md"));
	assert!(body.contains("- deleted changesets: .changeset/feature.md"));
	assert!(body.contains("## monochange Release Record"));
	assert!(body.contains("\"command\": \"release-pr\""));
}

#[test]
fn render_release_commit_body_omits_release_targets_when_empty() {
	let source = sample_source_configuration_for_release_commit();
	let mut manifest = sample_release_manifest_for_commit_message(false, false);
	manifest.release_targets.clear();

	let body = crate::render_release_commit_body(Some(&source), &manifest);
	assert!(!body.contains("- release targets:"));
	assert!(body.contains("- released packages: monochange, monochange_core"));
}

#[test]
fn build_release_commit_message_uses_default_title_without_source() {
	let manifest = sample_release_manifest_for_commit_message(false, false);

	let commit_message = crate::build_release_commit_message(None, &manifest);
	assert_eq!(commit_message.subject, "chore(release): prepare release");
	assert!(commit_message
		.body
		.as_deref()
		.is_some_and(|body| body.contains("## monochange Release Record")));
}

#[test]
fn render_release_commit_body_omits_empty_optional_sections() {
	let source = sample_source_configuration_for_release_commit();
	let manifest = sample_release_manifest_for_commit_message(false, false);

	let body = crate::render_release_commit_body(Some(&source), &manifest);
	assert!(body.contains("Prepare release."));
	assert!(body.contains("- release targets: sdk (1.2.3)"));
	assert!(body.contains("- released packages: monochange, monochange_core"));
	assert!(!body.contains("- updated changelogs:"));
	assert!(!body.contains("- deleted changesets:"));
}

#[test]
fn build_release_record_captures_provider_and_release_targets() {
	let source = sample_source_configuration_for_release_commit();
	let manifest = sample_release_manifest_for_commit_message(true, true);

	let record = crate::build_release_record(Some(&source), &manifest);
	assert_eq!(record.command, "release-pr");
	assert_eq!(record.version.as_deref(), Some("1.2.3"));
	assert_eq!(record.group_version.as_deref(), Some("1.2.3"));
	assert_eq!(record.release_targets.len(), 1);
	assert_eq!(record.release_targets[0].tag_name, "v1.2.3");
	assert_eq!(record.updated_changelogs.len(), 1);
	assert_eq!(record.deleted_changesets.len(), 1);
	let provider = record
		.provider
		.unwrap_or_else(|| panic!("expected provider block"));
	assert_eq!(provider.kind, monochange_core::SourceProvider::GitHub);
	assert_eq!(provider.owner, "ifiokjr");
	assert_eq!(provider.repo, "monochange");
}

fn sample_source_configuration_for_release_commit() -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings {
			title: "chore(release): prepare release".to_string(),
			..monochange_core::ChangeRequestSettings::default()
		},
		bot: monochange_core::BotSettings::default(),
	}
}

fn sample_release_manifest_for_commit_message(
	include_changelog: bool,
	include_deleted_changeset: bool,
) -> monochange_core::ReleaseManifest {
	monochange_core::ReleaseManifest {
		command: "release-pr".to_string(),
		dry_run: false,
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string(), "monochange_core".to_string()],
			rendered_title: "monochange 1.2.3".to_string(),
			rendered_changelog_title: "1.2.3".to_string(),
		}],
		released_packages: vec!["monochange".to_string(), "monochange_core".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		changelogs: if include_changelog {
			vec![monochange_core::ReleaseManifestChangelog {
				owner_id: "sdk".to_string(),
				owner_kind: monochange_core::ReleaseOwnerKind::Group,
				path: Path::new("crates/monochange/CHANGELOG.md").to_path_buf(),
				format: monochange_core::ChangelogFormat::Monochange,
				notes: monochange_core::ReleaseNotesDocument {
					title: "1.2.3".to_string(),
					summary: vec!["- prepare release".to_string()],
					sections: Vec::new(),
				},
				rendered: "## 1.2.3".to_string(),
			}]
		} else {
			Vec::new()
		},
		changesets: Vec::new(),
		deleted_changesets: if include_deleted_changeset {
			vec![Path::new(".changeset/feature.md").to_path_buf()]
		} else {
			Vec::new()
		},
		plan: monochange_core::ReleaseManifestPlan {
			workspace_root: Path::new(".").to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}

#[test]
fn text_release_record_discovery_renders_targets_packages_and_provider() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234567890".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 0,
		record: sample_release_record_for_discovery_text(),
	};

	let rendered = crate::release_record::text_release_record_discovery(&discovery);
	assert!(rendered.contains("input ref: v1.2.3"));
	assert!(rendered.contains("resolved commit: abc1234"));
	assert!(rendered.contains("record commit: abc1234"));
	assert!(rendered.contains("distance: 0"));
	assert!(rendered.contains("version: 1.2.3"));
	assert!(rendered.contains("group version: 1.2.3"));
	assert!(rendered.contains("- group sdk -> 1.2.3 (tag: v1.2.3)"));
	assert!(rendered.contains("- monochange"));
	assert!(rendered.contains("- monochange_core"));
	assert!(rendered.contains("provider: github ifiokjr/monochange"));
}

fn sample_release_record_for_discovery_text() -> monochange_core::ReleaseRecord {
	monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string(), "monochange_core".to_string()],
		}],
		released_packages: vec!["monochange".to_string(), "monochange_core".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		updated_changelogs: vec![Path::new("crates/monochange/CHANGELOG.md").to_path_buf()],
		deleted_changesets: vec![Path::new(".changeset/feature.md").to_path_buf()],
		provider: Some(monochange_core::ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

#[test]
fn text_release_record_discovery_omits_empty_sections() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "HEAD".to_string(),
		resolved_commit: "def5678901234".to_string(),
		record_commit: "abc1234567890".to_string(),
		distance: 2,
		record: monochange_core::ReleaseRecord {
			schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
			kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
			created_at: "2026-04-07T08:00:00Z".to_string(),
			command: "release-pr".to_string(),
			version: None,
			group_version: None,
			release_targets: Vec::new(),
			released_packages: Vec::new(),
			changed_files: Vec::new(),
			updated_changelogs: Vec::new(),
			deleted_changesets: Vec::new(),
			provider: None,
		},
	};

	let rendered = crate::release_record::text_release_record_discovery(&discovery);
	assert!(rendered.contains("input ref: HEAD"));
	assert!(!rendered.contains("  targets:"));
	assert!(!rendered.contains("  packages:"));
	assert!(!rendered.contains("  provider:"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn commit_release_command_creates_local_commit_with_release_record() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("monochange/release-base", root);
	copy_fixture("monochange/commit-release", root);
	init_git_repo(root);
	git_in_temp_repo(root, &["add", "."]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);

	let output = run_cli(
		root,
		[OsString::from("mc"), OsString::from("commit-release")],
	)
	.unwrap_or_else(|error| panic!("commit-release output: {error}"));
	let commit_subject = git_output_in_temp_repo(root, &["log", "-1", "--pretty=%s"]);
	let commit_body = git_output_in_temp_repo(root, &["log", "-1", "--pretty=%B"]);
	let status = git_output_in_temp_repo(root, &["status", "--short"]);

	assert!(output.contains("release commit:"));
	assert!(output.contains("status: completed"));
	assert_eq!(commit_subject, "chore(release): prepare release");
	assert!(commit_body.contains("## monochange Release Record"));
	assert!(commit_body.contains("\"command\": \"commit-release\""));
	assert!(
		status.is_empty(),
		"expected clean working tree, got: {status}"
	);
}

#[test]
fn commit_release_command_reports_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	copy_fixture("monochange/release-base", root);
	copy_fixture("monochange/commit-release", root);

	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("commit-release"),
			OsString::from("--format"),
			OsString::from("json"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("commit-release json output: {error}"));
	let value: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("parse json: {error}"));
	assert_eq!(
		value.pointer("/releaseCommit/subject"),
		Some(&serde_json::Value::String(
			"chore(release): prepare release".to_string()
		))
	);
	assert_eq!(
		value.pointer("/releaseCommit/status"),
		Some(&serde_json::Value::String("dry_run".to_string()))
	);
	assert_eq!(
		value.pointer("/manifest/command"),
		Some(&serde_json::Value::String("commit-release".to_string()))
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn repair_release_command_dry_run_reports_text_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);

	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("repair-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
			OsString::from("--target"),
			OsString::from("HEAD"),
			OsString::from("--sync-provider=false"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("repair-release output: {error}"));

	assert!(output.contains("repair release:"));
	assert!(output.contains("from: v1.2.3"));
	assert!(output.contains("tags to move:"));
	assert!(output.contains("v1.2.3"));
	assert!(output.contains("provider sync: disabled"));
	assert!(output.contains("status: dry-run"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn repair_release_command_reports_json_output() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);

	let output = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("repair-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
			OsString::from("--sync-provider=false"),
			OsString::from("--format"),
			OsString::from("json"),
			OsString::from("--dry-run"),
		],
	)
	.unwrap_or_else(|error| panic!("repair-release json output: {error}"));
	let value: serde_json::Value =
		serde_json::from_str(&output).unwrap_or_else(|error| panic!("parse json: {error}"));
	assert_eq!(
		value.get("from"),
		Some(&serde_json::Value::String("v1.2.3".to_string()))
	);
	assert_eq!(
		value.get("target"),
		Some(&serde_json::Value::String("HEAD".to_string()))
	);
	assert_eq!(
		value.get("status"),
		Some(&serde_json::Value::String("dry_run".to_string()))
	);
	assert_eq!(
		value
			.pointer("/gitTagResults/0/tagName")
			.and_then(serde_json::Value::as_str),
		Some("v1.2.3")
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn repair_release_command_rejects_non_descendant_targets_without_force() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write initial file: {error}"));
	git_in_temp_repo(root, &["add", "monochange.toml", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let initial_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	let release_record =
		monochange_core::render_release_record_block(&sample_release_record_for_retarget())
			.unwrap_or_else(|error| panic!("render release record: {error}"));
	git_in_temp_repo(
		root,
		&[
			"commit",
			"-m",
			"chore(release): prepare release",
			"-m",
			release_record.as_str(),
		],
	);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "follow-up\n")
		.unwrap_or_else(|error| panic!("write follow-up file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "fix: follow-up release change"]);
	let main_branch = git_output_in_temp_repo(root, &["branch", "--show-current"]);
	git_in_temp_repo(root, &["checkout", &initial_commit]);
	fs::write(root.join("release.txt"), "branch\n")
		.unwrap_or_else(|error| panic!("write branch release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "branch"]);
	let branch_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["checkout", &main_branch]);

	let error = run_cli(
		root,
		[
			OsString::from("mc"),
			OsString::from("repair-release"),
			OsString::from("--from"),
			OsString::from("v1.2.3"),
			OsString::from("--target"),
			OsString::from(branch_commit),
			OsString::from("--sync-provider=false"),
			OsString::from("--dry-run"),
		],
	)
	.err()
	.unwrap_or_else(|| panic!("expected non-descendant error"));
	assert!(error
		.to_string()
		.contains("is not a descendant of release-record commit"));
}

#[test]
fn template_context_exposes_release_commit_namespace() {
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: Some(crate::CommitReleaseReport {
			subject: "chore(release): prepare release".to_string(),
			body: "Prepare release.".to_string(),
			commit: Some("abc1234567890".to_string()),
			tracked_paths: vec![PathBuf::from("Cargo.toml")],
			dry_run: false,
			status: "completed".to_string(),
		}),
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let template_context = crate::build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("release_commit")
			.and_then(|value| value.pointer("/commit"))
			.and_then(serde_json::Value::as_str),
		Some("abc1234567890")
	);
}

#[test]
fn template_context_exposes_retarget_namespace() {
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: Some(sample_retarget_release_report()),
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let template_context = crate::build_cli_template_context(&context, &BTreeMap::new(), None);
	assert_eq!(
		template_context
			.get("retarget")
			.and_then(|value| value.pointer("/status"))
			.and_then(serde_json::Value::as_str),
		Some("dry_run")
	);
}

#[test]
fn template_context_exposes_manifest_affected_steps_and_custom_variables() {
	let mut step_outputs = BTreeMap::new();
	step_outputs.insert(
		"lint".to_string(),
		crate::CommandStepOutput {
			stdout: "ok".to_string(),
			stderr: String::new(),
		},
	);
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: Some(monochange_core::ChangesetPolicyEvaluation {
			status: monochange_core::ChangesetPolicyStatus::Passed,
			required: true,
			enforce: false,
			summary: "covered".to_string(),
			comment: None,
			labels: Vec::new(),
			matched_skip_labels: Vec::new(),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			matched_paths: vec!["crates/core/src/lib.rs".to_string()],
			ignored_paths: Vec::new(),
			changeset_paths: vec![".changeset/core.md".to_string()],
			affected_package_ids: vec!["core".to_string()],
			covered_package_ids: vec!["core".to_string()],
			uncovered_package_ids: Vec::new(),
			errors: Vec::new(),
		}),
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs,
		command_logs: Vec::new(),
	};
	let inputs = BTreeMap::from([("format".to_string(), vec!["json".to_string()])]);
	let variables = BTreeMap::from([
		(
			"custom_version".to_string(),
			monochange_core::CommandVariable::Version,
		),
		(
			"custom_changesets".to_string(),
			monochange_core::CommandVariable::Changesets,
		),
	]);
	let template_context = crate::build_cli_template_context(&context, &inputs, Some(&variables));
	assert_eq!(
		template_context
			.get("manifest")
			.and_then(|value| value.pointer("/path"))
			.and_then(serde_json::Value::as_str),
		Some("target/release-manifest.json")
	);
	assert_eq!(
		template_context
			.get("affected")
			.and_then(|value| value.pointer("/status"))
			.and_then(serde_json::Value::as_str),
		Some("passed")
	);
	assert_eq!(
		template_context
			.get("steps")
			.and_then(|value| value.pointer("/lint/stdout"))
			.and_then(serde_json::Value::as_str),
		Some("ok")
	);
	assert_eq!(
		template_context
			.get("custom_version")
			.and_then(serde_json::Value::as_str),
		Some("1.2.3")
	);
	assert_eq!(
		template_context
			.get("custom_changesets")
			.and_then(serde_json::Value::as_str),
		Some("")
	);
	assert_eq!(
		template_context
			.get("released_packages_list")
			.and_then(serde_json::Value::as_array)
			.map(Vec::len),
		Some(1)
	);
}

#[test]
fn render_cli_command_result_prefers_retarget_report() {
	let cli_command = monochange_core::CliCommandDefinition {
		name: "repair-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: Some(sample_retarget_release_report()),
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let rendered = crate::render_cli_command_result(&cli_command, &context);
	assert!(rendered.contains("repair release:"));
	assert!(rendered.contains("status: dry-run"));
}

#[test]
fn render_cli_command_result_renders_release_follow_up_sections() {
	let cli_command = monochange_core::CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	};
	let context = CliContext {
		root: PathBuf::from("."),
		dry_run: true,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: Some(sample_prepared_release_for_cli_render()),
		release_manifest_path: Some(PathBuf::from("target/release-manifest.json")),
		release_requests: Vec::new(),
		release_results: vec!["dry-run org/repo v1.2.3 (sdk) via github".to_string()],
		release_request: None,
		release_request_result: Some(
			"dry-run org/repo monochange/release/release -> main via github".to_string(),
		),
		release_commit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: vec!["dry-run org/repo 123".to_string()],
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let rendered = crate::render_cli_command_result(&cli_command, &context);
	assert!(rendered.contains("release manifest: target/release-manifest.json"));
	assert!(rendered.contains("releases:"));
	assert!(rendered.contains("release request:"));
	assert!(rendered.contains("issue comments:"));
	assert!(rendered.contains("changed files:"));
}

#[test]
fn execute_cli_command_retarget_release_requires_from_input() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = monochange_core::CliCommandDefinition {
		name: "repair-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::RetargetRelease {
			inputs: BTreeMap::new(),
		}],
	};
	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		true,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing from input error"));
	assert!(error
		.to_string()
		.contains("`RetargetRelease` requires a `from` input"));
}

#[test]
fn execute_cli_command_release_follow_up_steps_require_prepare_release() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cases = [
		(
			"release-manifest",
			monochange_core::CliStepDefinition::RenderReleaseManifest {
				path: None,
				inputs: BTreeMap::new(),
			},
			"`RenderReleaseManifest` requires a previous `PrepareRelease` step",
		),
		(
			"publish-release",
			monochange_core::CliStepDefinition::PublishRelease {
				inputs: BTreeMap::new(),
			},
			"`PublishRelease` requires a previous `PrepareRelease` step",
		),
		(
			"release-pr",
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				inputs: BTreeMap::new(),
			},
			"`OpenReleaseRequest` requires a previous `PrepareRelease` step",
		),
		(
			"release-comments",
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				inputs: BTreeMap::new(),
			},
			"`CommentReleasedIssues` requires a previous `PrepareRelease` step",
		),
	];
	for (name, step, expected) in cases {
		let cli_command = monochange_core::CliCommandDefinition {
			name: name.to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: vec![step],
		};
		let error = crate::execute_cli_command(
			tempdir.path(),
			&configuration,
			&cli_command,
			true,
			BTreeMap::new(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected missing PrepareRelease error for {name}"));
		assert!(error.to_string().contains(expected), "error: {error}");
	}
}

#[test]
fn execute_cli_command_source_follow_up_steps_require_source_configuration() {
	let root = fixture_path("monochange/release-base");
	let mut configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	configuration.source = Some(monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: Some("https://gitlab.example.com".to_string()),
		api_url: Some("https://gitlab.example.com/api/v4".to_string()),
		owner: "org".to_string(),
		repo: "repo".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	});
	let prepare_and_publish = monochange_core::CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				inputs: BTreeMap::new(),
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				inputs: BTreeMap::new(),
			},
		],
	};
	let error = crate::execute_cli_command(
		&root,
		&configuration,
		&prepare_and_publish,
		true,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected github source requirement error"));
	assert!(error.to_string().contains(
		"`CommentReleasedIssues` requires `[source].provider = \"github\"` configuration"
	));
}

#[test]
fn execute_cli_command_release_follow_up_steps_render_dry_run_outputs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_fixture("monochange/release-base", tempdir.path());
	fs::OpenOptions::new()
		.append(true)
		.open(tempdir.path().join("monochange.toml"))
		.and_then(|mut file| {
			use std::io::Write;
			writeln!(
				file,
				"\n[source]\nprovider = \"github\"\nowner = \"ifiokjr\"\nrepo = \"monochange\"\n"
			)
		})
		.unwrap_or_else(|error| panic!("append source config: {error}"));
	let root = tempdir.path();
	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("configuration: {error}"));

	let manifest_path = root.join("target/release-manifest.json");
	let render_manifest = monochange_core::CliCommandDefinition {
		name: "release-manifest".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				inputs: BTreeMap::new(),
			},
			monochange_core::CliStepDefinition::RenderReleaseManifest {
				path: Some(PathBuf::from("target/release-manifest.json")),
				inputs: BTreeMap::new(),
			},
		],
	};
	let render_output = crate::execute_cli_command(
		root,
		&configuration,
		&render_manifest,
		true,
		BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("render manifest: {error}"));
	assert!(render_output.contains("release manifest: target/release-manifest.json"));
	let manifest_contents =
		fs::read_to_string(&manifest_path).unwrap_or_else(|error| panic!("read manifest: {error}"));
	assert!(manifest_contents.contains("\"releaseTargets\""));

	let publish_release = monochange_core::CliCommandDefinition {
		name: "publish-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				inputs: BTreeMap::new(),
			},
			monochange_core::CliStepDefinition::PublishRelease {
				inputs: BTreeMap::new(),
			},
		],
	};
	let publish_output = crate::execute_cli_command(
		root,
		&configuration,
		&publish_release,
		true,
		BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("publish release: {error}"));
	assert!(publish_output.contains("releases:"));
	assert!(publish_output.contains("dry-run"));

	let release_request = monochange_core::CliCommandDefinition {
		name: "release-pr".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				inputs: BTreeMap::new(),
			},
			monochange_core::CliStepDefinition::OpenReleaseRequest {
				inputs: BTreeMap::new(),
			},
		],
	};
	let request_output = crate::execute_cli_command(
		root,
		&configuration,
		&release_request,
		true,
		BTreeMap::new(),
	)
	.unwrap_or_else(|error| panic!("open release request: {error}"));
	assert!(request_output.contains("release request:"));
	assert!(request_output.contains("dry-run"));

	let issue_comments = monochange_core::CliCommandDefinition {
		name: "release-comments".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![
			monochange_core::CliStepDefinition::PrepareRelease {
				inputs: BTreeMap::new(),
			},
			monochange_core::CliStepDefinition::CommentReleasedIssues {
				inputs: BTreeMap::new(),
			},
		],
	};
	let comments_output =
		crate::execute_cli_command(root, &configuration, &issue_comments, true, BTreeMap::new())
			.unwrap_or_else(|error| panic!("comment released issues: {error}"));
	assert!(!comments_output.is_empty());
}

#[test]
fn execute_matches_rejects_unknown_cli_command_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_blank_monochange_config(tempdir.path());
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let matches = clap::Command::new("dummy")
		.try_get_matches_from(["dummy"])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let error = crate::execute_matches(tempdir.path(), &configuration, "missing", &matches)
		.err()
		.unwrap_or_else(|| panic!("expected unknown command error"));
	assert!(error.to_string().contains("unknown command `missing`"));
}

#[test]
fn run_git_capture_and_process_report_io_failures_for_missing_worktrees() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = tempdir.path().join("missing");
	let capture_error = crate::run_git_capture(&missing, &["status"], "capture failure")
		.err()
		.unwrap_or_else(|| panic!("expected capture error"));
	assert!(capture_error.to_string().contains("capture failure"));

	let mut command = std::process::Command::new("git");
	command.current_dir(&missing).arg("status");
	let process_error = crate::run_git_process(command, "process failure")
		.err()
		.unwrap_or_else(|| panic!("expected process error"));
	assert!(process_error.to_string().contains("process failure"));
}

#[test]
fn parse_boolean_step_input_rejects_invalid_values() {
	let inputs = BTreeMap::from([("force".to_string(), vec!["maybe".to_string()])]);
	let error = crate::parse_boolean_step_input(&inputs, "force")
		.err()
		.unwrap_or_else(|| panic!("expected invalid boolean error"));
	assert!(error
		.to_string()
		.contains("invalid boolean value `maybe` for `force`"));
}

#[test]
fn inferred_retarget_source_configuration_prefers_configured_source() {
	let configured = sample_github_source_configuration("https://example.com");
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let inferred =
		crate::inferred_retarget_source_configuration(Some(&configured), &discovery, true)
			.unwrap_or_else(|| panic!("expected configured source"));
	assert_eq!(inferred, configured);
}

#[test]
fn inferred_retarget_source_configuration_infers_from_release_record_provider() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let inferred = crate::inferred_retarget_source_configuration(None, &discovery, true)
		.unwrap_or_else(|| panic!("expected inferred source"));
	assert_eq!(inferred.provider, monochange_core::SourceProvider::GitHub);
	assert_eq!(inferred.owner, "ifiokjr");
	assert_eq!(inferred.repo, "monochange");
}

#[test]
fn inferred_retarget_source_configuration_returns_none_when_sync_is_disabled() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	assert!(crate::inferred_retarget_source_configuration(None, &discovery, false).is_none());
}

#[test]
fn build_retarget_release_report_marks_completed_status() {
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: "abc1234".to_string(),
		record_commit: "abc1234".to_string(),
		distance: 2,
		record: sample_release_record_for_retarget(),
	};
	let result = monochange_core::RetargetResult {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		force: true,
		git_tag_results: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "abc1234".to_string(),
			to_commit: "def5678".to_string(),
			operation: monochange_core::RetargetOperation::Moved,
			message: None,
		}],
		provider_results: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitHub,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Synced,
			url: None,
			message: None,
		}],
		sync_provider: true,
		dry_run: false,
	};
	let report = crate::build_retarget_release_report("v1.2.3", "HEAD", &discovery, false, &result);
	assert_eq!(report.status, "completed");
	assert!(!report.is_descendant);
}

#[test]
fn render_retarget_release_report_handles_provider_sync_variants() {
	let mut report = sample_retarget_release_report();
	report.sync_provider = true;
	report.provider_results = vec![monochange_core::RetargetProviderResult {
		provider: monochange_core::SourceProvider::GitHub,
		tag_name: "v1.2.3".to_string(),
		target_commit: "def5678901234".to_string(),
		operation: monochange_core::RetargetProviderOperation::Synced,
		url: None,
		message: None,
	}];
	let rendered = crate::render_retarget_release_report(&report);
	assert!(rendered.contains("provider sync: github"));
	assert!(rendered.contains("[planned]"));

	let mut no_provider_report = sample_retarget_release_report();
	no_provider_report.sync_provider = true;
	no_provider_report.provider_results = Vec::new();
	no_provider_report.git_tag_results = Vec::new();
	let rendered = crate::render_retarget_release_report(&no_provider_report);
	assert!(rendered.contains("provider sync: none"));
}

#[test]
fn retarget_operation_label_covers_all_variants() {
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Planned),
		"planned"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Moved),
		"moved"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::AlreadyUpToDate),
		"already_up_to_date"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Skipped),
		"skipped"
	);
	assert_eq!(
		crate::retarget_operation_label(monochange_core::RetargetOperation::Failed),
		"failed"
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_collects_tag_and_provider_updates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "second"]);

	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD~1"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit: record_commit.clone(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let source = sample_github_source_configuration("https://example.com");

	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, Some(&source))
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));

	assert!(plan.is_descendant);
	assert_eq!(plan.git_tag_updates.len(), 1);
	assert_eq!(plan.git_tag_updates[0].tag_name, "v1.2.3");
	assert_eq!(
		plan.git_tag_updates[0].operation,
		monochange_core::RetargetOperation::Planned
	);
	assert_eq!(plan.provider_updates.len(), 1);
	assert_eq!(
		plan.provider_updates[0].operation,
		monochange_core::RetargetProviderOperation::Planned
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_rejects_non_descendant_without_force() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let main_branch = git_output_in_temp_repo(root, &["branch", "--show-current"]);
	let base_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	fs::write(root.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "second"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["checkout", &base_commit]);
	fs::write(root.join("release.txt"), "branch\n")
		.unwrap_or_else(|error| panic!("write branch release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "branch"]);
	let branch_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["checkout", &main_branch]);
	assert_ne!(record_commit, branch_commit);

	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: record_commit.clone(),
		resolved_commit: record_commit.clone(),
		record_commit: record_commit.clone(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};

	let error =
		crate::plan_release_retarget(root, &discovery, &branch_commit, false, false, true, None)
			.err()
			.unwrap_or_else(|| panic!("expected non-descendant error"));
	assert!(error
		.to_string()
		.contains("is not a descendant of release-record commit"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn execute_release_retarget_moves_tags_and_pushes_origin_refs() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let remote = tempdir.path().join("remote.git");
	git_in_dir(
		tempdir.path(),
		&["init", "--bare", remote.to_str().unwrap_or("remote.git")],
	);

	let repo = tempdir.path().join("repo");
	fs::create_dir_all(&repo).unwrap_or_else(|error| panic!("create repo dir: {error}"));
	init_git_repo(&repo);
	git_in_temp_repo(
		&repo,
		&[
			"remote",
			"add",
			"origin",
			remote.to_str().unwrap_or("remote.git"),
		],
	);
	fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	git_in_temp_repo(&repo, &["tag", "v1.2.3"]);
	git_in_temp_repo(
		&repo,
		&[
			"push",
			"origin",
			"HEAD",
			"refs/tags/v1.2.3:refs/tags/v1.2.3",
		],
	);
	fs::write(repo.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "second"]);

	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let plan = crate::plan_release_retarget(&repo, &discovery, "HEAD", false, false, false, None)
		.unwrap_or_else(|error| panic!("plan retarget: {error}"));

	let result = crate::execute_release_retarget(&repo, None, &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	let head = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	let local_tag = git_output_in_temp_repo(&repo, &["rev-parse", "refs/tags/v1.2.3^{commit}"]);
	let remote_tag = git_output_in_git_dir(&remote, &["rev-parse", "refs/tags/v1.2.3^{commit}"]);

	assert_eq!(
		result.git_tag_results[0].operation,
		monochange_core::RetargetOperation::Moved
	);
	assert_eq!(local_tag, head);
	assert_eq!(remote_tag, head);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn retarget_release_reports_missing_tags() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);

	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: record_commit.clone(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};

	let error = crate::retarget_release(root, &discovery, "HEAD", false, false, true, None)
		.err()
		.unwrap_or_else(|| panic!("expected missing tag error"));
	assert!(error
		.to_string()
		.contains("release tag v1.2.3 could not be found"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_marks_existing_target_tag_as_up_to_date() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit: record_commit.clone(),
		distance: 0,
		record: sample_release_record_for_retarget(),
	};

	let plan =
		crate::plan_release_retarget(root, &discovery, &record_commit, false, false, false, None)
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	assert_eq!(
		plan.git_tag_updates
			.first()
			.unwrap_or_else(|| panic!("expected git tag update"))
			.operation,
		monochange_core::RetargetOperation::AlreadyUpToDate
	);

	let result = crate::execute_release_retarget(root, None, &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(
		result
			.git_tag_results
			.first()
			.unwrap_or_else(|| panic!("expected git tag result"))
			.operation,
		monochange_core::RetargetOperation::AlreadyUpToDate
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_marks_unsupported_provider_sync_in_dry_run() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = Some(monochange_core::ReleaseRecordProvider {
		kind: monochange_core::SourceProvider::GitLab,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
	});
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	};

	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, Some(&source))
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	let provider_update = plan
		.provider_updates
		.first()
		.unwrap_or_else(|| panic!("expected provider update"));
	assert_eq!(
		provider_update.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
	assert!(provider_update
		.message
		.as_deref()
		.unwrap_or("")
		.contains("gitlab"));

	let result = crate::execute_release_retarget(root, Some(&source), &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(
		result
			.provider_results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
}

#[test]
fn execute_release_retarget_returns_empty_provider_results_without_source() {
	let plan = monochange_core::RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: Vec::new(),
		provider_updates: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitHub,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Planned,
			url: None,
			message: None,
		}],
		sync_provider: true,
		dry_run: false,
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let result = crate::execute_release_retarget(tempdir.path(), None, &plan)
		.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert!(result.provider_results.is_empty());
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_rejects_provider_kind_mismatches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	};

	let error =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, false, true, Some(&source))
			.err()
			.unwrap_or_else(|| panic!("expected provider mismatch error"));
	assert!(error
		.to_string()
		.contains("does not match configured source provider"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_rejects_repository_mismatches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = Some(monochange_core::ReleaseRecordProvider {
		kind: monochange_core::SourceProvider::GitHub,
		owner: "other".to_string(),
		repo: "repo".to_string(),
		host: None,
	});
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = sample_github_source_configuration("https://example.com");

	let error =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, false, true, Some(&source))
			.err()
			.unwrap_or_else(|| panic!("expected repository mismatch error"));
	assert!(error
		.to_string()
		.contains("does not match configured source repository"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_skips_provider_updates_when_no_provider_is_available() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = None;
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let plan = crate::plan_release_retarget(root, &discovery, "HEAD", false, true, true, None)
		.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	assert!(plan.provider_updates.is_empty());
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn plan_release_retarget_accepts_missing_record_provider_with_configured_source() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	let mut record = sample_release_record_for_retarget();
	record.provider = None;
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record,
	};
	let source = sample_github_source_configuration("https://example.com");
	let plan =
		crate::plan_release_retarget(root, &discovery, "HEAD", false, false, true, Some(&source))
			.unwrap_or_else(|error| panic!("plan retarget: {error}"));
	assert_eq!(plan.git_tag_updates.len(), 1);
}

#[test]
fn execute_release_retarget_delegates_github_provider_sync() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.3");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"id\":42,\"html_url\":\"https://example.com/releases/42\",\"target_commitish\":\"abc1234\"}");
	});
	let update_release = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/releases/42")
			.json_body_obj(&serde_json::json!({ "target_commitish": "def5678" }));
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/releases/42\"}");
	});
	let source = sample_github_source_configuration(&server.base_url());
	let plan = monochange_core::RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "def5678".to_string(),
			to_commit: "def5678".to_string(),
			operation: monochange_core::RetargetOperation::AlreadyUpToDate,
			message: None,
		}],
		provider_updates: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitHub,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Planned,
			url: None,
			message: None,
		}],
		sync_provider: true,
		dry_run: false,
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let result = temp_env::with_var("GITHUB_TOKEN", Some("token"), || {
		crate::execute_release_retarget(tempdir.path(), Some(&source), &plan)
	})
	.unwrap_or_else(|error| panic!("execute retarget: {error}"));
	assert_eq!(result.provider_results.len(), 1);
	release_lookup.assert();
	assert_eq!(update_release.calls(), 0);
	assert_eq!(
		result
			.provider_results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::AlreadyAligned
	);
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn retarget_release_succeeds_end_to_end_without_provider_sync() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let remote = root.join("remote.git");
	git_in_dir(
		root,
		&["init", "--bare", remote.to_str().unwrap_or("remote.git")],
	);
	let repo = root.join("repo");
	fs::create_dir_all(&repo).unwrap_or_else(|error| panic!("create repo dir: {error}"));
	init_git_repo(&repo);
	git_in_temp_repo(
		&repo,
		&[
			"remote",
			"add",
			"origin",
			remote.to_str().unwrap_or("remote.git"),
		],
	);
	fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "initial"]);
	let record_commit = git_output_in_temp_repo(&repo, &["rev-parse", "HEAD"]);
	git_in_temp_repo(&repo, &["tag", "v1.2.3"]);
	git_in_temp_repo(
		&repo,
		&[
			"push",
			"origin",
			"HEAD",
			"refs/tags/v1.2.3:refs/tags/v1.2.3",
		],
	);
	fs::write(repo.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second release file: {error}"));
	git_in_temp_repo(&repo, &["add", "release.txt"]);
	git_in_temp_repo(&repo, &["commit", "-m", "second"]);
	let discovery = monochange_core::ReleaseRecordDiscovery {
		input_ref: "v1.2.3".to_string(),
		resolved_commit: record_commit.clone(),
		record_commit,
		distance: 0,
		record: sample_release_record_for_retarget(),
	};
	let result = crate::retarget_release(&repo, &discovery, "HEAD", false, false, false, None)
		.unwrap_or_else(|error| panic!("retarget release: {error}"));
	assert_eq!(result.git_tag_results.len(), 1);
}

#[test]
fn git_is_ancestor_reports_git_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = crate::git_support::git_is_ancestor(tempdir.path(), "abc", "def")
		.err()
		.unwrap_or_else(|| panic!("expected git ancestry error"));
	assert!(error.to_string().contains("discovery error"));
}

#[test]
fn git_is_ancestor_reports_missing_worktree_directory_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = tempdir.path().join("missing");
	let error = crate::git_support::git_is_ancestor(&missing, "abc", "def")
		.err()
		.unwrap_or_else(|| panic!("expected git spawn error"));
	assert!(error
		.to_string()
		.contains("failed to compare commit ancestry"));
}

#[test]
fn sync_retargeted_provider_releases_reports_unsupported_provider_in_dry_run() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Gitea,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	};
	let updates = vec![monochange_core::RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: monochange_core::RetargetOperation::Moved,
		message: None,
	}];
	let results = crate::release_record::sync_retargeted_provider_releases(&source, &updates, true)
		.unwrap_or_else(|error| panic!("expected dry-run provider results: {error}"));
	assert_eq!(results.len(), 1);
	assert_eq!(
		results
			.first()
			.unwrap_or_else(|| panic!("expected provider result"))
			.operation,
		monochange_core::RetargetProviderOperation::Unsupported
	);
}

#[test]
fn sync_retargeted_provider_releases_rejects_unsupported_provider_in_real_mode() {
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::Gitea,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	};
	let updates = vec![monochange_core::RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: monochange_core::RetargetOperation::Moved,
		message: None,
	}];
	let error = crate::release_record::sync_retargeted_provider_releases(&source, &updates, false)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported provider error"));
	assert!(error.to_string().contains("gitea"));
}

#[test]
fn execute_release_retarget_rejects_unsupported_provider_sync_in_real_mode() {
	let plan = monochange_core::RetargetPlan {
		record_commit: "abc1234".to_string(),
		target_commit: "def5678".to_string(),
		is_descendant: true,
		force: false,
		git_tag_updates: Vec::new(),
		provider_updates: vec![monochange_core::RetargetProviderResult {
			provider: monochange_core::SourceProvider::GitLab,
			tag_name: "v1.2.3".to_string(),
			target_commit: "def5678".to_string(),
			operation: monochange_core::RetargetProviderOperation::Unsupported,
			url: None,
			message: Some(
				"provider sync is not yet supported for gitlab release retargeting".to_string(),
			),
		}],
		sync_provider: true,
		dry_run: false,
	};
	let source = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	};
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = crate::execute_release_retarget(tempdir.path(), Some(&source), &plan)
		.err()
		.unwrap_or_else(|| panic!("expected unsupported provider error"));
	assert!(error
		.to_string()
		.contains("provider sync is not yet supported for gitlab release retargeting"));
}

#[test]
fn execute_cli_command_commit_release_requires_prepare_release() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command = monochange_core::CliCommandDefinition {
		name: "commit-release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: vec![monochange_core::CliStepDefinition::CommitRelease {
			inputs: BTreeMap::new(),
		}],
	};

	let error = crate::execute_cli_command(
		tempdir.path(),
		&configuration,
		&cli_command,
		false,
		BTreeMap::new(),
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing PrepareRelease error"));
	assert!(error
		.to_string()
		.contains("`CommitRelease` requires a previous `PrepareRelease` step"));
}

#[test]
fn git_stage_paths_reports_git_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = crate::git_stage_paths(tempdir.path(), &[PathBuf::from("release.txt")])
		.err()
		.unwrap_or_else(|| panic!("expected git stage failure"));
	assert!(error
		.to_string()
		.contains("failed to stage release commit files"));
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn first_parent_commits_returns_head_then_ancestors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	git_in_temp_repo(root, &["init"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
	fs::write(root.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write initial file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "initial"]);
	fs::write(root.join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write second file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "second"]);

	let head = git_output_in_temp_repo(root, &["rev-parse", "HEAD"]);
	let commits = crate::git_support::first_parent_commits(root, &head)
		.unwrap_or_else(|error| panic!("first parent commits: {error}"));
	assert_eq!(commits.first().map(String::as_str), Some(head.as_str()));
	assert_eq!(commits.len(), 2);
}

#[test]
fn git_head_commit_and_read_commit_message_roundtrip() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	init_git_repo(root);
	fs::write(root.join("release.txt"), "hello\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "subject line", "-m", "body line"]);

	let head = crate::git_head_commit(root).unwrap_or_else(|error| panic!("head commit: {error}"));
	let message = crate::read_git_commit_message(root, &head)
		.unwrap_or_else(|error| panic!("read commit message: {error}"));
	assert!(message.contains("subject line"));
	assert!(message.contains("body line"));
}

#[test]
fn run_git_status_reports_nonzero_exit_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	init_git_repo(tempdir.path());
	let error = crate::run_git_status(
		tempdir.path(),
		&["definitely-not-a-real-git-command"],
		"git status failure",
	)
	.err()
	.unwrap_or_else(|| panic!("expected git status failure"));
	assert!(error.to_string().contains("git status failure"));
}

fn sample_retarget_release_report() -> crate::RetargetReleaseReport {
	crate::RetargetReleaseReport {
		from: "v1.2.3".to_string(),
		target: "HEAD".to_string(),
		resolved_from_commit: "abc1234567890".to_string(),
		record_commit: "abc1234567890".to_string(),
		target_commit: "def5678901234".to_string(),
		distance: 1,
		is_descendant: true,
		force: false,
		dry_run: true,
		sync_provider: false,
		tags: vec!["v1.2.3".to_string()],
		git_tag_results: vec![monochange_core::RetargetTagResult {
			tag_name: "v1.2.3".to_string(),
			from_commit: "abc1234567890".to_string(),
			to_commit: "def5678901234".to_string(),
			operation: monochange_core::RetargetOperation::Planned,
			message: None,
		}],
		provider_results: Vec::new(),
		status: "dry_run".to_string(),
	}
}

fn write_blank_monochange_config(root: &Path) {
	fs::write(root.join("monochange.toml"), "")
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
}

fn create_release_record_history(root: &Path) {
	init_git_repo(root);
	fs::write(root.join("release.txt"), "release\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(root, &["add", "monochange.toml", "release.txt"]);
	let record = sample_release_record_for_retarget();
	let release_record = monochange_core::render_release_record_block(&record)
		.unwrap_or_else(|error| panic!("render release record: {error}"));
	git_in_temp_repo(
		root,
		&[
			"commit",
			"-m",
			"chore(release): prepare release",
			"-m",
			release_record.as_str(),
		],
	);
	git_in_temp_repo(root, &["tag", "v1.2.3"]);
	fs::write(root.join("release.txt"), "follow-up\n")
		.unwrap_or_else(|error| panic!("write follow-up file: {error}"));
	git_in_temp_repo(root, &["add", "release.txt"]);
	git_in_temp_repo(root, &["commit", "-m", "fix: follow-up release change"]);
}

fn sample_release_record_for_retarget() -> monochange_core::ReleaseRecord {
	monochange_core::ReleaseRecord {
		schema_version: monochange_core::RELEASE_RECORD_SCHEMA_VERSION,
		kind: monochange_core::RELEASE_RECORD_KIND.to_string(),
		created_at: "2026-04-07T08:00:00Z".to_string(),
		command: "release-pr".to_string(),
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![monochange_core::ReleaseRecordTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			version_format: VersionFormat::Primary,
			tag: true,
			release: true,
			tag_name: "v1.2.3".to_string(),
			members: vec!["monochange".to_string()],
		}],
		released_packages: vec!["monochange".to_string()],
		changed_files: vec![Path::new("Cargo.lock").to_path_buf()],
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		provider: Some(monochange_core::ReleaseRecordProvider {
			kind: monochange_core::SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
		}),
	}
}

#[test]
fn group_changelog_include_allows_expected_member_targets() {
	let core_only = BTreeSet::from(["core".to_string()]);
	let core_and_app = BTreeSet::from(["core".to_string(), "app".to_string()]);

	assert!(crate::group_changelog_include_allows(
		&GroupChangelogInclude::All,
		&core_only,
	));
	assert!(!crate::group_changelog_include_allows(
		&GroupChangelogInclude::GroupOnly,
		&core_only,
	));
	assert!(crate::group_changelog_include_allows(
		&GroupChangelogInclude::Selected(["core".to_string()].into()),
		&core_only,
	));
	assert!(!crate::group_changelog_include_allows(
		&GroupChangelogInclude::Selected(["app".to_string()].into()),
		&core_only,
	));
	assert!(!crate::group_changelog_include_allows(
		&GroupChangelogInclude::Selected(["core".to_string()].into()),
		&core_and_app,
	));
}

#[test]
fn filter_group_release_note_change_handles_missing_context_and_direct_group_targets() {
	let planned_group = sample_planned_group();
	let group = sample_group_definition(GroupChangelogInclude::GroupOnly);

	assert!(crate::filter_group_release_note_change(
		&sample_release_note_change(None),
		Some(&group),
		&planned_group,
		&BTreeMap::new(),
	)
	.is_none());

	assert!(crate::filter_group_release_note_change(
		&sample_release_note_change(Some(".changeset/missing.md")),
		Some(&group),
		&planned_group,
		&BTreeMap::new(),
	)
	.is_none());

	let group_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/group.md"),
		vec![PreparedChangesetTarget {
			id: "sdk".to_string(),
			kind: ChangesetTargetKind::Group,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
		}],
	)]);
	let renamed = crate::filter_group_release_note_change(
		&sample_release_note_change(Some(".changeset/group.md")),
		Some(&group),
		&planned_group,
		&group_changeset,
	)
	.unwrap_or_else(|| panic!("expected direct group note"));
	assert_eq!(renamed.package_name, "sdk");

	let outside_group_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/outside.md"),
		vec![PreparedChangesetTarget {
			id: "docs".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
		}],
	)]);
	assert!(crate::filter_group_release_note_change(
		&sample_release_note_change(Some(".changeset/outside.md")),
		Some(&group),
		&planned_group,
		&outside_group_changeset,
	)
	.is_none());
}

#[test]
fn filter_group_release_note_change_respects_member_allowlists() {
	let planned_group = sample_planned_group();
	let change = sample_release_note_change(Some(".changeset/member.md"));
	let selected_core =
		sample_group_definition(GroupChangelogInclude::Selected(["core".to_string()].into()));
	let member_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/member.md"),
		vec![PreparedChangesetTarget {
			id: "core".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: None,
			origin: "author".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
		}],
	)]);
	assert!(crate::filter_group_release_note_change(
		&change,
		Some(&selected_core),
		&planned_group,
		&member_changeset,
	)
	.is_some());

	let blocked_changeset = BTreeMap::from([(
		PathBuf::from(".changeset/member.md"),
		vec![
			PreparedChangesetTarget {
				id: "core".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: None,
				origin: "author".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
			},
			PreparedChangesetTarget {
				id: "app".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: None,
				origin: "author".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
			},
		],
	)]);
	assert!(crate::filter_group_release_note_change(
		&change,
		Some(&selected_core),
		&planned_group,
		&blocked_changeset,
	)
	.is_none());

	let message = crate::render_group_filtered_update_message("sdk");
	assert!(message.contains("No group-facing notes were recorded for this release."));
	assert!(message.contains("synchronized group `sdk`"));
}

fn sample_release_note_change(source_path: Option<&str>) -> crate::ReleaseNoteChange {
	crate::ReleaseNoteChange {
		package_id: "core".to_string(),
		package_name: "core".to_string(),
		package_labels: Vec::new(),
		source_path: source_path.map(str::to_string),
		summary: "add grouped note".to_string(),
		details: None,
		bump: BumpSeverity::Minor,
		change_type: None,
		context: None,
		changeset_path: None,
		change_owner: None,
		change_owner_link: None,
		review_request: None,
		review_request_link: None,
		introduced_commit: None,
		introduced_commit_link: None,
		last_updated_commit: None,
		last_updated_commit_link: None,
		related_issues: None,
		related_issue_links: None,
		closed_issues: None,
		closed_issue_links: None,
	}
}

fn sample_group_definition(include: GroupChangelogInclude) -> monochange_core::GroupDefinition {
	monochange_core::GroupDefinition {
		id: "sdk".to_string(),
		packages: vec!["core".to_string(), "app".to_string()],
		changelog: None,
		changelog_include: include,
		extra_changelog_sections: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: false,
		release: false,
		version_format: VersionFormat::Namespaced,
	}
}

#[test]
fn assistant_display_name_covers_all_variants() {
	assert_eq!(
		crate::assistant_display_name(crate::Assistant::Generic),
		"Generic MCP client"
	);
	assert_eq!(
		crate::assistant_display_name(crate::Assistant::Claude),
		"Claude"
	);
	assert_eq!(
		crate::assistant_display_name(crate::Assistant::Cursor),
		"Cursor"
	);
	assert_eq!(
		crate::assistant_display_name(crate::Assistant::Copilot),
		"GitHub Copilot"
	);
	assert_eq!(crate::assistant_display_name(crate::Assistant::Pi), "Pi");
}

#[test]
fn assistant_setup_payload_contains_mcp_config_and_guidance() {
	let payload = crate::assistant_setup_payload(crate::Assistant::Pi);
	assert_eq!(payload["assistant"].as_str(), Some("Pi"));
	assert_eq!(
		payload["mcp_config"]["mcpServers"]["monochange"]["command"],
		"monochange"
	);
	assert!(payload["repo_guidance"]
		.as_array()
		.is_some_and(|items| items.len() >= 5));
	assert!(payload["notes"]
		.as_array()
		.is_some_and(|items| items.iter().any(|item| item
			.as_str()
			.is_some_and(|text| text.contains("monochange mcp")))));
}

#[test]
fn assistant_setup_payload_includes_variant_specific_notes() {
	let cases = [
		(crate::Assistant::Generic, "supports stdio MCP servers"),
		(crate::Assistant::Claude, "Claude's MCP configuration"),
		(
			crate::Assistant::Cursor,
			"Configure the MCP server in Cursor",
		),
		(
			crate::Assistant::Copilot,
			"support MCP-compatible server definitions",
		),
	];
	for (assistant, expected_note) in cases {
		let payload = crate::assistant_setup_payload(assistant);
		assert!(payload["notes"]
			.as_array()
			.is_some_and(|items| items.iter().any(|item| {
				item.as_str()
					.is_some_and(|text| text.contains(expected_note))
			})));
	}
}

#[test]
fn build_command_and_configured_change_type_choices_include_runtime_metadata() {
	let command = crate::build_command("monochange");
	assert_eq!(command.get_name(), "monochange");
	assert!(command.clone().find_subcommand("assist").is_some());
	assert!(command.clone().find_subcommand("release-record").is_some());

	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		release_notes: monochange_core::ReleaseNotesSettings::default(),
		packages: vec![monochange_core::PackageDefinition {
			id: "core".to_string(),
			path: PathBuf::from("crates/core"),
			package_type: monochange_core::PackageType::Cargo,
			changelog: None,
			extra_changelog_sections: vec![monochange_core::ExtraChangelogSection {
				name: "Docs".to_string(),
				types: vec![" docs ".to_string(), "test".to_string()],
				default_bump: None,
			}],
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		groups: vec![monochange_core::GroupDefinition {
			id: "sdk".to_string(),
			packages: vec!["core".to_string()],
			changelog: None,
			changelog_include: GroupChangelogInclude::All,
			extra_changelog_sections: vec![monochange_core::ExtraChangelogSection {
				name: "Security".to_string(),
				types: vec!["security".to_string(), "docs".to_string()],
				default_bump: None,
			}],
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
	};
	assert_eq!(
		crate::configured_change_type_choices(&configuration),
		vec![
			"docs".to_string(),
			"security".to_string(),
			"test".to_string()
		]
	);
}

#[test]
fn apply_runtime_change_type_choices_updates_only_unconfigured_change_inputs() {
	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		release_notes: monochange_core::ReleaseNotesSettings::default(),
		packages: vec![monochange_core::PackageDefinition {
			id: "core".to_string(),
			path: PathBuf::from("crates/core"),
			package_type: monochange_core::PackageType::Cargo,
			changelog: None,
			extra_changelog_sections: vec![monochange_core::ExtraChangelogSection {
				name: "Docs".to_string(),
				types: vec!["docs".to_string(), "test".to_string()],
				default_bump: None,
			}],
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
		}],
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
	};
	let mut cli = vec![
		monochange_core::CliCommandDefinition {
			name: "change".to_string(),
			help_text: Some("Create a change".to_string()),
			inputs: vec![CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::String,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			}],
			steps: Vec::new(),
		},
		monochange_core::CliCommandDefinition {
			name: "change-with-existing-choices".to_string(),
			help_text: None,
			inputs: vec![CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::Choice,
				help_text: None,
				required: false,
				default: None,
				choices: vec!["existing".to_string()],
				short: None,
			}],
			steps: Vec::new(),
		},
	];

	crate::apply_runtime_change_type_choices(&mut cli, &configuration);

	assert_eq!(cli[0].inputs[0].kind, CliInputKind::Choice);
	assert_eq!(cli[0].inputs[0].choices, vec!["docs", "test"]);
	assert_eq!(cli[1].inputs[0].choices, vec!["existing"]);
}

#[test]
fn apply_runtime_change_type_choices_preserves_existing_choice_inputs_and_empty_configs() {
	let configuration = monochange_core::WorkspaceConfiguration {
		root_path: PathBuf::from("."),
		defaults: monochange_core::WorkspaceDefaults::default(),
		release_notes: monochange_core::ReleaseNotesSettings::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
	};
	let mut cli = vec![monochange_core::CliCommandDefinition {
		name: "change".to_string(),
		help_text: None,
		inputs: vec![CliInputDefinition {
			name: "type".to_string(),
			kind: CliInputKind::Choice,
			help_text: None,
			required: false,
			default: None,
			choices: vec!["existing".to_string()],
			short: None,
		}],
		steps: Vec::new(),
	}];
	crate::apply_runtime_change_type_choices(&mut cli, &configuration);
	assert_eq!(cli[0].inputs[0].choices, vec!["existing"]);
	assert_eq!(cli[0].inputs[0].kind, CliInputKind::Choice);
}

#[test]
fn cli_commands_for_root_uses_workspace_cli_when_configuration_load_succeeds() {
	let cli = crate::cli_commands_for_root(&fixture_path("config/package-group-and-cli"));
	assert_eq!(
		cli.iter()
			.map(|command| command.name.as_str())
			.collect::<Vec<_>>(),
		vec!["release"]
	);
}

#[test]
fn build_assist_subcommand_parses_valid_inputs_and_rejects_unknown_assistants() {
	let command = clap::Command::new("mc").subcommand(crate::build_assist_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("assist"),
			OsString::from("pi"),
		])
		.unwrap_or_else(|error| panic!("assist matches: {error}"));
	let (_, assist_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected assist subcommand"));
	assert_eq!(
		assist_matches
			.get_one::<String>("assistant")
			.map(String::as_str),
		Some("pi")
	);
	assert_eq!(
		assist_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("text")
	);

	let error = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("assist"),
			OsString::from("unknown"),
		])
		.err()
		.unwrap_or_else(|| panic!("expected invalid assistant error"));
	assert_eq!(error.kind(), clap::error::ErrorKind::InvalidValue);
}

#[test]
fn build_release_record_subcommand_requires_from_and_supports_json_output() {
	let command = clap::Command::new("mc").subcommand(crate::build_release_record_subcommand());
	let matches = command
		.clone()
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("release-record"),
			OsString::from("--from"),
			OsString::from("HEAD"),
			OsString::from("--format"),
			OsString::from("json"),
		])
		.unwrap_or_else(|error| panic!("release-record matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected release-record subcommand"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("from")
			.map(String::as_str),
		Some("HEAD")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("format")
			.map(String::as_str),
		Some("json")
	);
	let error = command
		.try_get_matches_from([OsString::from("mc"), OsString::from("release-record")])
		.err()
		.unwrap_or_else(|| panic!("expected missing from error"));
	assert_eq!(
		error.kind(),
		clap::error::ErrorKind::MissingRequiredArgument
	);
}

#[test]
fn build_command_with_cli_registers_custom_subcommands_and_default_help_text() {
	let cli = vec![monochange_core::CliCommandDefinition {
		name: "custom".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
	}];
	let command = crate::build_command_with_cli("monochange", &cli);
	assert!(command.clone().find_subcommand("custom").is_some());
	let error = command
		.try_get_matches_from([
			OsString::from("monochange"),
			OsString::from("custom"),
			OsString::from("--help"),
		])
		.err()
		.unwrap_or_else(|| panic!("expected help output"));
	assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
	assert!(error.to_string().contains("Run the `custom` command"));
}

#[test]
fn cli_command_after_help_covers_supported_commands_and_custom_commands() {
	let cases = [
		("change", "Prefer configured package ids"),
		("release", "Direct package changes propagate to dependents"),
		("commit-release", "Embeds a durable release record block"),
		("affected", "Group-owned changesets cover all members"),
		("diagnostics", "linked review request"),
		("repair-release", "Defaults to descendant-only retargets"),
	];
	for (name, expected) in cases {
		let after_help = crate::cli_command_after_help(&monochange_core::CliCommandDefinition {
			name: name.to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		})
		.unwrap_or_else(|| panic!("expected after_help for {name}"));
		assert!(after_help.contains(expected));
	}
	assert!(
		crate::cli_command_after_help(&monochange_core::CliCommandDefinition {
			name: "custom".to_string(),
			help_text: None,
			inputs: Vec::new(),
			steps: Vec::new(),
		})
		.is_none()
	);
}

#[test]
fn build_cli_command_subcommand_parses_supported_input_kinds() {
	let cli_command = monochange_core::CliCommandDefinition {
		name: "custom".to_string(),
		help_text: Some("Run a custom command".to_string()),
		inputs: vec![
			CliInputDefinition {
				name: "package".to_string(),
				kind: CliInputKind::String,
				help_text: Some("Target package".to_string()),
				required: true,
				default: None,
				choices: Vec::new(),
				short: Some('p'),
			},
			CliInputDefinition {
				name: "changed_paths".to_string(),
				kind: CliInputKind::StringList,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "output".to_string(),
				kind: CliInputKind::Path,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "enabled".to_string(),
				kind: CliInputKind::Boolean,
				help_text: None,
				required: false,
				default: None,
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "sync_provider".to_string(),
				kind: CliInputKind::Boolean,
				help_text: None,
				required: false,
				default: Some("true".to_string()),
				choices: Vec::new(),
				short: None,
			},
			CliInputDefinition {
				name: "type".to_string(),
				kind: CliInputKind::Choice,
				help_text: None,
				required: false,
				default: Some("docs".to_string()),
				choices: vec!["docs".to_string(), "test".to_string()],
				short: None,
			},
		],
		steps: Vec::new(),
	};

	let command =
		clap::Command::new("mc").subcommand(crate::build_cli_command_subcommand(&cli_command));
	let matches = command
		.try_get_matches_from([
			OsString::from("mc"),
			OsString::from("custom"),
			OsString::from("-p"),
			OsString::from("core"),
			OsString::from("--changed-paths"),
			OsString::from("crates/core/src/lib.rs"),
			OsString::from("--changed-paths"),
			OsString::from("Cargo.toml"),
			OsString::from("--output"),
			OsString::from("out.json"),
			OsString::from("--enabled"),
			OsString::from("--sync-provider=false"),
		])
		.unwrap_or_else(|error| panic!("matches: {error}"));
	let (_, subcommand_matches) = matches
		.subcommand()
		.unwrap_or_else(|| panic!("expected subcommand matches"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("package")
			.map(String::as_str),
		Some("core")
	);
	assert_eq!(
		subcommand_matches
			.get_many::<String>("changed_paths")
			.unwrap_or_else(|| panic!("expected changed_paths"))
			.map(String::as_str)
			.collect::<Vec<_>>(),
		vec!["crates/core/src/lib.rs", "Cargo.toml"]
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("output")
			.map(String::as_str),
		Some("out.json")
	);
	assert!(subcommand_matches.get_flag("enabled"));
	assert_eq!(
		subcommand_matches
			.get_one::<String>("sync_provider")
			.map(String::as_str),
		Some("false")
	);
	assert_eq!(
		subcommand_matches
			.get_one::<String>("type")
			.map(String::as_str),
		Some("docs")
	);
}

#[test]
fn run_assist_renders_json_and_text_outputs() {
	let json_output = crate::run_assist(crate::Assistant::Cursor, crate::AssistOutputFormat::Json)
		.unwrap_or_else(|error| panic!("assist json: {error}"));
	assert!(json_output.contains("\"assistant\": \"Cursor\""));
	assert!(json_output.contains("\"mcp_config\""));

	let text_output = crate::run_assist(crate::Assistant::Copilot, crate::AssistOutputFormat::Text)
		.unwrap_or_else(|error| panic!("assist text: {error}"));
	assert!(text_output.contains("monochange assist"));
	assert!(text_output.contains("Notes for GitHub Copilot:"));
	assert!(text_output.contains("Suggested repo-local guidance:"));
}

#[test]
fn render_release_record_discovery_supports_text_and_json_formats() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	write_blank_monochange_config(root);
	create_release_record_history(root);
	let text = crate::render_release_record_discovery(root, "HEAD", crate::OutputFormat::Text)
		.unwrap_or_else(|error| panic!("release-record text: {error}"));
	assert!(text.contains("release record:"));
	assert!(text.contains("input ref: HEAD"));
	let json = crate::render_release_record_discovery(root, "HEAD", crate::OutputFormat::Json)
		.unwrap_or_else(|error| panic!("release-record json: {error}"));
	assert!(json.contains("\"record\""));
	assert!(json.contains("\"resolvedCommit\""));
	assert!(json.contains("\"inputRef\": "));
}

#[test]
fn render_tag_name_and_provider_urls_follow_provider_conventions() {
	let github = sample_github_source_configuration("https://api.github.com");
	assert_eq!(
		crate::render_tag_name("core", "1.2.3", VersionFormat::Primary),
		"v1.2.3"
	);
	assert_eq!(
		crate::render_tag_name("core", "1.2.3", VersionFormat::Namespaced),
		"core/v1.2.3"
	);
	assert!(crate::tag_url_for_provider(&github, "v1.2.3").contains("/releases/tag/v1.2.3"));
	assert!(crate::compare_url_for_provider(&github, "v1.2.2", "v1.2.3")
		.contains("compare/v1.2.2...v1.2.3"));

	let gitlab = monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		host: Some("gitlab.example.com".to_string()),
		api_url: None,
		owner: "group".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	};
	assert!(crate::tag_url_for_provider(&gitlab, "v1.2.3").contains("gitlab.example.com"));
	assert!(crate::compare_url_for_provider(&gitlab, "v1.2.2", "v1.2.3").contains("/-/compare/"));
}

#[test]
fn parse_tag_prefix_and_version_parses_primary_and_namespaced_tags() {
	let primary = crate::parse_tag_prefix_and_version("v1.2.3")
		.unwrap_or_else(|| panic!("expected primary tag"));
	assert_eq!(primary.0, "v");
	assert_eq!(primary.1, Version::new(1, 2, 3));

	let namespaced = crate::parse_tag_prefix_and_version("core/v2.0.0")
		.unwrap_or_else(|| panic!("expected namespaced tag"));
	assert_eq!(namespaced.0, "core/v");
	assert_eq!(namespaced.1, Version::new(2, 0, 0));
	assert_eq!(crate::parse_tag_prefix_and_version("not-a-tag"), None);
}

#[test]
fn resolve_release_datetime_honors_env_overrides() {
	temp_env::with_var("MONOCHANGE_RELEASE_DATE", Some("2026-04-07"), || {
		assert_eq!(
			crate::resolve_release_datetime(),
			chrono::NaiveDate::from_ymd_opt(2026, 4, 7)
				.unwrap_or_else(|| panic!("valid date"))
				.and_hms_opt(0, 0, 0)
				.unwrap_or_else(|| panic!("valid time"))
		);
	});
	temp_env::with_var(
		"MONOCHANGE_RELEASE_DATE",
		Some("2026-04-07T12:34:56"),
		|| {
			assert_eq!(
				crate::resolve_release_datetime(),
				chrono::NaiveDate::from_ymd_opt(2026, 4, 7)
					.unwrap_or_else(|| panic!("valid date"))
					.and_hms_opt(12, 34, 56)
					.unwrap_or_else(|| panic!("valid time"))
			);
		},
	);
}

#[test]
fn default_change_path_sluggifies_first_package_reference() {
	let path = crate::default_change_path(
		Path::new("/workspace"),
		&["cargo:crates/core/Cargo.toml".to_string()],
	);
	assert!(path.starts_with("/workspace/.changeset"));
	assert!(path
		.to_string_lossy()
		.ends_with("-cargo-crates-core-cargo-toml.md"));
}

#[test]
fn format_publish_state_and_source_operation_labels_are_stable() {
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Public),
		"public"
	);
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Private),
		"private"
	);
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Unpublished),
		"unpublished"
	);
	assert_eq!(
		crate::format_publish_state(monochange_core::PublishState::Excluded),
		"excluded"
	);
	assert_eq!(
		crate::format_source_operation(&monochange_core::SourceReleaseOperation::Created),
		"created"
	);
	assert_eq!(
		crate::format_source_operation(&monochange_core::SourceReleaseOperation::Updated),
		"updated"
	);
	assert_eq!(
		crate::format_change_request_operation(
			&monochange_core::SourceChangeRequestOperation::Created
		),
		"created"
	);
	assert_eq!(
		crate::format_change_request_operation(
			&monochange_core::SourceChangeRequestOperation::Updated
		),
		"updated"
	);
}

#[test]
fn parse_output_format_accepts_text_and_json_and_rejects_invalid_values() {
	assert_eq!(
		crate::parse_output_format("text").unwrap(),
		crate::OutputFormat::Text
	);
	assert_eq!(
		crate::parse_output_format("json").unwrap(),
		crate::OutputFormat::Json
	);
	let error = crate::parse_output_format("yaml")
		.err()
		.unwrap_or_else(|| panic!("expected output format error"));
	assert!(error.to_string().contains("unsupported output format"));
}

#[test]
fn effective_title_template_prefers_specific_then_defaults_then_builtin() {
	assert_eq!(
		crate::effective_title_template(Some("specific"), Some("default"), "builtin"),
		"specific"
	);
	assert_eq!(
		crate::effective_title_template(None, Some("default"), "builtin"),
		"default"
	);
	assert_eq!(
		crate::effective_title_template(None, None, "builtin"),
		"builtin"
	);
}

#[test]
fn default_title_templates_follow_version_format_defaults() {
	assert_eq!(
		crate::default_release_title_for_format(VersionFormat::Primary),
		monochange_core::DEFAULT_RELEASE_TITLE_PRIMARY
	);
	assert_eq!(
		crate::default_release_title_for_format(VersionFormat::Namespaced),
		monochange_core::DEFAULT_RELEASE_TITLE_NAMESPACED
	);
	assert_eq!(
		crate::default_changelog_version_title_for_format(VersionFormat::Primary),
		monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_PRIMARY
	);
	assert_eq!(
		crate::default_changelog_version_title_for_format(VersionFormat::Namespaced),
		monochange_core::DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED
	);
}

#[test]
fn current_dir_or_dot_returns_current_directory() {
	let current = std::env::current_dir().unwrap_or_else(|error| panic!("current dir: {error}"));
	assert_eq!(crate::current_dir_or_dot(), current);
}

#[test]
fn render_discovery_report_supports_json_and_text_formats() {
	let report = monochange_core::DiscoveryReport {
		workspace_root: PathBuf::from("."),
		packages: vec![monochange_core::PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			PathBuf::from("crates/core/Cargo.toml"),
			PathBuf::from("."),
			Some(Version::new(1, 0, 0)),
			monochange_core::PublishState::Public,
		)],
		dependencies: Vec::new(),
		version_groups: vec![monochange_core::VersionGroup {
			group_id: "sdk".to_string(),
			display_name: "sdk".to_string(),
			members: vec!["cargo:crates/core/Cargo.toml".to_string()],
			mismatch_detected: false,
		}],
		warnings: vec!["warning text".to_string()],
	};
	let json = crate::render_discovery_report(&report, crate::OutputFormat::Json)
		.unwrap_or_else(|error| panic!("json discovery: {error}"));
	assert!(json.contains("\"workspaceRoot\""));
	assert!(json.contains("\"warning text\""));

	let text = crate::render_discovery_report(&report, crate::OutputFormat::Text)
		.unwrap_or_else(|error| panic!("text discovery: {error}"));
	assert!(text.contains("Workspace discovery for ."));
	assert!(text.contains("Packages: 1"));
	assert!(text.contains("Warnings:"));
}

#[test]
fn find_previous_tag_returns_previous_matching_prefix() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	init_git_repo(tempdir.path());
	fs::write(tempdir.path().join("release.txt"), "first\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(tempdir.path(), &["add", "release.txt"]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "first release"]);
	git_in_temp_repo(tempdir.path(), &["tag", "core/v1.0.0"]);
	fs::write(tempdir.path().join("release.txt"), "second\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git_in_temp_repo(tempdir.path(), &["add", "release.txt"]);
	git_in_temp_repo(tempdir.path(), &["commit", "-m", "second release"]);
	git_in_temp_repo(tempdir.path(), &["tag", "core/v1.2.0"]);
	git_in_temp_repo(tempdir.path(), &["tag", "app/v9.9.9"]);
	assert_eq!(
		crate::find_previous_tag(tempdir.path(), "core/v1.2.0"),
		Some("core/v1.0.0".to_string())
	);
	assert_eq!(
		crate::find_previous_tag(tempdir.path(), "core/v1.0.0"),
		None
	);
}

fn sample_planned_group() -> monochange_core::PlannedVersionGroup {
	monochange_core::PlannedVersionGroup {
		group_id: "sdk".to_string(),
		display_name: "sdk".to_string(),
		members: vec![
			"cargo:crates/core".to_string(),
			"cargo:crates/app".to_string(),
		],
		mismatch_detected: false,
		planned_version: Some(
			Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}")),
		),
		recommended_bump: BumpSeverity::Minor,
	}
}

fn sample_prepared_release_for_cli_render() -> crate::PreparedRelease {
	crate::PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![monochange_core::ReleaseDecision {
				package_id: "core".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(
					Version::parse("1.2.3").unwrap_or_else(|error| panic!("version: {error}")),
				),
				group_id: Some("sdk".to_string()),
				reasons: vec!["release core".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			}],
			groups: vec![sample_planned_group()],
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: Some("1.2.3".to_string()),
		release_targets: vec![crate::ReleaseTarget {
			id: "sdk".to_string(),
			kind: monochange_core::ReleaseOwnerKind::Group,
			version: "1.2.3".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.3".to_string(),
			members: vec!["core".to_string(), "app".to_string()],
			rendered_title: "sdk 1.2.3".to_string(),
			rendered_changelog_title: "sdk changelog".to_string(),
		}],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
	}
}

fn sample_github_source_configuration(api_url: &str) -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		host: None,
		api_url: Some(api_url.to_string()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ReleaseProviderSettings::default(),
		pull_requests: monochange_core::ChangeRequestSettings::default(),
		bot: monochange_core::BotSettings::default(),
	}
}

fn init_git_repo(root: &Path) {
	git_in_temp_repo(root, &["init"]);
	git_in_temp_repo(root, &["config", "user.name", "monochange Tests"]);
	git_in_temp_repo(root, &["config", "user.email", "monochange@example.com"]);
	git_in_temp_repo(root, &["config", "commit.gpgsign", "false"]);
}

fn git_in_dir(root: &Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_git_dir(git_dir: &Path, args: &[&str]) -> String {
	let output = std::process::Command::new("git")
		.arg("--git-dir")
		.arg(git_dir)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git --git-dir {args:?}: {error}"));
	assert!(output.status.success(), "git --git-dir {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}

fn git_in_temp_repo(root: &Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output_in_temp_repo(root: &Path, args: &[&str]) -> String {
	let output = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout)
		.unwrap_or_else(|error| panic!("git output utf8: {error}"))
		.trim()
		.to_string()
}
