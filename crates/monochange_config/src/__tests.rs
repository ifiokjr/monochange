use std::path::Path;
use std::path::PathBuf;

use miette::LabeledSpan;
use monochange_core::BumpSeverity;
use monochange_core::ChangelogDefinition;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::CliCommandDefinition;
use monochange_core::CliInputDefinition;
use monochange_core::CliInputKind;
use monochange_core::CliStepDefinition;
use monochange_core::CliStepInputValue;
use monochange_core::Ecosystem;
use monochange_core::EcosystemType;
use monochange_core::GroupChangelogInclude;
use monochange_core::PackageRecord;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishState;
use monochange_core::RegistryKind;
use monochange_core::ShellConfig;
use monochange_core::SourceProvider;
use monochange_test_helpers::current_test_name;
use semver::Version;
use tempfile::tempdir;

use crate::apply_version_groups;
use crate::frontmatter_span_for_line_column;
use crate::line_and_column_for_offset;
use crate::line_index_for_offset;
use crate::load_change_signals;
use crate::load_changeset_file;
use crate::load_workspace_configuration;
use crate::range_to_span;
use crate::render_diagnostic_notes;
use crate::render_source_diagnostic;
use crate::render_source_snippet;
use crate::render_source_snippets;
use crate::resolve_package_reference;
use crate::sort_labels_by_location;
use crate::validate_workspace;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[test]
fn shared_fs_test_support_helpers_cover_plain_and_case_names_and_fixture_copying() {
	assert_eq!(
		current_test_name(),
		"shared_fs_test_support_helpers_cover_plain_and_case_names_and_fixture_copying"
	);
	let named = std::thread::Builder::new()
		.name("case_1_config_helper_thread".to_string())
		.spawn(current_test_name)
		.unwrap_or_else(|error| panic!("spawn thread: {error}"))
		.join()
		.unwrap_or_else(|error| panic!("join thread: {error:?}"));
	assert_eq!(named, "config_helper_thread");

	let copied_fixture = setup_fixture("test-support/setup-fixture");
	assert_eq!(
		std::fs::read_to_string(copied_fixture.path().join("root.txt"))
			.unwrap_or_else(|error| panic!("read copied fixture: {error}")),
		"root fixture\n"
	);

	let scenario = setup_scenario_workspace("test-support/scenario-root");
	assert_eq!(
		std::fs::read_to_string(scenario.path().join("root-only.txt"))
			.unwrap_or_else(|error| panic!("read copied scenario: {error}")),
		"root scenario\n"
	);
	assert!(!scenario.path().join("expected").exists());
}

#[test]
fn load_workspace_configuration_uses_defaults_when_file_is_missing() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Patch);
	assert!(!configuration.defaults.include_private);
	assert!(configuration.defaults.warn_on_group_mismatch);
	assert!(!configuration.defaults.strict_version_conflicts);
	assert_eq!(configuration.defaults.package_type, None);
	assert_eq!(configuration.defaults.changelog, None);
	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::Monochange
	);
	assert_eq!(configuration.defaults.empty_update_message, None);
	assert!(configuration.packages.is_empty());
	assert!(configuration.groups.is_empty());
	assert_eq!(configuration.cli.len(), 9);
	let cli_command_names = configuration
		.cli
		.iter()
		.map(|cli_command| cli_command.name.as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		cli_command_names,
		vec![
			"validate",
			"discover",
			"change",
			"release",
			"placeholder-publish",
			"publish",
			"affected",
			"diagnostics",
			"repair-release"
		]
	);
	assert_eq!(configuration.cargo.enabled, None);
	assert_eq!(configuration.npm.enabled, None);
	assert_eq!(configuration.deno.enabled, None);
	assert_eq!(configuration.dart.enabled, None);
}

#[test]
fn load_workspace_configuration_supports_diagnostics_cli_command_definition() {
	let root = fixture_path("config/diagnostics-cli");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let diagnostics = configuration
		.cli
		.iter()
		.find(|command| command.name == "diagnostics")
		.unwrap_or_else(|| panic!("expected diagnostics command"));
	assert_eq!(diagnostics.steps.len(), 1);
	match diagnostics.steps.first() {
		Some(CliStepDefinition::DiagnoseChangesets { .. }) => {}
		Some(_) => panic!("expected DiagnoseChangesets step"),
		None => panic!("expected diagnostics step"),
	}
}

#[test]
fn load_workspace_configuration_merges_default_cli_commands_with_overrides_and_custom_commands() {
	let root = fixture_path("config/merge-default-cli-overrides");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let cli_command_names = configuration
		.cli
		.iter()
		.map(|cli_command| cli_command.name.as_str())
		.collect::<Vec<_>>();
	assert_eq!(
		cli_command_names,
		vec![
			"validate",
			"discover",
			"change",
			"release",
			"placeholder-publish",
			"publish",
			"affected",
			"diagnostics",
			"repair-release"
		]
	);

	let discover = configuration
		.cli
		.iter()
		.find(|command| command.name == "discover")
		.unwrap_or_else(|| panic!("expected discover command"));
	assert_eq!(
		discover.help_text.as_deref(),
		Some("Discover packages across supported ecosystems")
	);
	assert_eq!(discover.inputs.len(), 1);

	let release = configuration
		.cli
		.iter()
		.find(|command| command.name == "release")
		.unwrap_or_else(|| panic!("expected release command"));
	assert_eq!(
		release.help_text.as_deref(),
		Some("Prepare a release and refresh the cached release manifest")
	);
	assert!(release.inputs.is_empty());
	assert!(matches!(
		release.steps.first(),
		Some(CliStepDefinition::PrepareRelease { .. })
	));
	assert_eq!(release.steps.len(), 1);
}

#[test]
fn load_workspace_configuration_supports_commit_release_cli_command_definition() {
	let root = fixture_path("config/commit-release-cli");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let commit_release = configuration
		.cli
		.iter()
		.find(|command| command.name == "commit-release")
		.unwrap_or_else(|| panic!("expected commit-release command"));
	assert_eq!(commit_release.steps.len(), 2);
	assert!(matches!(
		commit_release.steps.first(),
		Some(CliStepDefinition::PrepareRelease { .. })
	));
	assert!(matches!(
		commit_release.steps.get(1),
		Some(CliStepDefinition::CommitRelease { .. })
	));
}

#[test]
fn load_workspace_configuration_supports_retarget_release_cli_command_definition() {
	let root = fixture_path("config/repair-release-cli");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let repair_release = configuration
		.cli
		.iter()
		.find(|command| command.name == "repair-release")
		.unwrap_or_else(|| panic!("expected repair-release command"));
	assert_eq!(repair_release.steps.len(), 1);
	assert!(matches!(
		repair_release.steps.first(),
		Some(CliStepDefinition::RetargetRelease { .. })
	));
}

#[test]
fn load_workspace_configuration_rejects_invalid_boolean_input_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.repair-release]

[[cli.repair-release.inputs]]
name = "force"
type = "boolean"
default = "maybe"

[[cli.repair-release.steps]]
type = "RetargetRelease"
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected invalid boolean default error"));
	assert!(
		error
			.to_string()
			.contains("boolean default must be `true` or `false`")
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_when_conditions() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(
		tempdir.path().join("monochange.toml"),
		r#"
[cli.announce]

[[cli.announce.steps]]
type = "Command"
when = ""
command = "echo should not run"
"#,
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	let error = load_workspace_configuration(tempdir.path())
		.err()
		.unwrap_or_else(|| panic!("expected empty-when error"));
	assert!(error.to_string().contains("has an empty `when` condition"));
}

#[test]
fn load_workspace_configuration_supports_boolean_input_default_values() {
	let root = fixture_path("config/accepts-boolean-input-defaults");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let command = configuration
		.cli
		.iter()
		.find(|command| command.name == "pr-check")
		.unwrap_or_else(|| panic!("expected pr-check command"));
	let dry_run = command
		.inputs
		.iter()
		.find(|input| input.name == "dry_run")
		.unwrap_or_else(|| panic!("missing dry_run input"));
	let sync = command
		.inputs
		.iter()
		.find(|input| input.name == "sync")
		.unwrap_or_else(|| panic!("missing sync input"));
	assert_eq!(dry_run.default.as_deref(), Some("true"));
	assert_eq!(sync.default.as_deref(), Some("false"));
}

#[test]
fn load_workspace_configuration_parses_package_group_and_cli_command_declarations() {
	let root = fixture_path("config/package-group-and-cli");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.defaults.parent_bump, BumpSeverity::Minor);
	assert!(configuration.defaults.include_private);
	assert!(!configuration.defaults.strict_version_conflicts);
	assert_eq!(
		configuration.defaults.package_type,
		Some(monochange_core::PackageType::Cargo)
	);
	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::PathPattern(
			"{{ path }}/CHANGELOG.md".to_string()
		))
	);
	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::Monochange
	);
	assert_eq!(configuration.packages.len(), 2);
	assert_eq!(configuration.groups.len(), 1);
	assert_eq!(
		configuration
			.cli
			.iter()
			.find(|command| command.name == "release")
			.unwrap_or_else(|| panic!("expected release CLI command"))
			.steps
			.len(),
		1
	);
	assert_eq!(configuration.defaults.empty_update_message, None);
	assert_eq!(configuration.npm.roots, vec!["packages/*"]);
	let first_package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected first package"));
	assert_eq!(first_package.id, "core");
	assert_eq!(
		first_package.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("crates/core/changelog.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(
		configuration
			.groups
			.first()
			.unwrap_or_else(|| panic!("expected group"))
			.packages,
		vec!["core", "npm:web"]
	);
}

#[test]
fn load_workspace_configuration_parses_github_release_settings() {
	let root = fixture_path("config/github-release-settings");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let source = configuration
		.source
		.unwrap_or_else(|| panic!("expected source config"));
	assert_eq!(source.provider, SourceProvider::GitHub);
	assert_eq!(source.owner, "ifiokjr");
	assert_eq!(source.repo, "monochange");
	assert!(source.releases.enabled);
	assert!(source.releases.draft);
	assert!(source.releases.prerelease);
	assert!(source.releases.generate_notes);
	assert_eq!(
		source.releases.source,
		monochange_core::ProviderReleaseNotesSource::GitHubGenerated
	);
	assert!(source.pull_requests.enabled);
	assert_eq!(source.pull_requests.branch_prefix, "automation/release");
	assert_eq!(source.pull_requests.base, "develop");
	assert_eq!(
		source.pull_requests.title,
		"chore(release): prepare release"
	);
	assert_eq!(
		source.pull_requests.labels,
		vec!["release", "automated", "bot"]
	);
	assert!(source.pull_requests.auto_merge);
}

#[test]
fn load_workspace_configuration_parses_github_changeset_bot_settings() {
	let root = fixture_path("config/github-bot-settings");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let source = configuration
		.source
		.unwrap_or_else(|| panic!("expected source config"));
	assert_eq!(source.provider, SourceProvider::GitHub);
	let bot = &source.bot.changesets;
	assert!(bot.enabled);
	assert!(bot.comment_on_failure);
	assert_eq!(bot.skip_labels, vec!["no-changeset-required"]);
}

#[test]
fn load_workspace_configuration_rejects_missing_package_paths() {
	let root = fixture_path("config/rejects-missing-paths");
	let rendered = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(
		rendered.contains("does not exist")
			|| rendered.contains("missing")
			|| rendered.contains("cannot find"),
		"rendered: {rendered}"
	);
}

#[test]
fn load_workspace_configuration_rejects_duplicate_package_paths() {
	let root = fixture_path("config/rejects-duplicate-paths");
	let rendered = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(
		rendered.contains("duplicate")
			|| rendered.contains("same path")
			|| rendered.contains("already used by"),
		"rendered: {rendered}"
	);
}

#[test]
fn load_workspace_configuration_rejects_missing_expected_manifests() {
	let root = fixture_path("config/rejects-missing-manifests");
	let result = load_workspace_configuration(&root);
	let error = result
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	let rendered = error.render();
	// The error message varies: could say "missing expected cargo manifest" or "does not exist"
	assert!(
		rendered.contains("is missing expected cargo manifest")
			|| rendered.contains("does not exist")
			|| rendered.contains("cannot find")
			|| error.to_string().contains("missing")
			|| error.to_string().contains("not found"),
		"rendered: {rendered}\nerror: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_source_owner_and_repo() {
	let root_owner = fixture_path("config/rejects-empty-github-owner");
	assert!(
		load_workspace_configuration(&root_owner)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("[source].owner must not be empty")
	);

	let root_repo = fixture_path("config/rejects-empty-github-repo");
	assert!(
		load_workspace_configuration(&root_repo)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("[source].repo must not be empty")
	);
}

#[test]
fn load_workspace_configuration_rejects_invalid_pull_request_settings() {
	let root = fixture_path("config/rejects-invalid-pr-settings");
	assert!(
		load_workspace_configuration(&root)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("[source.pull_requests].branch_prefix must not be empty")
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_pull_request_base_and_title() {
	let root_base = fixture_path("config/rejects-empty-pr-base");
	assert!(
		load_workspace_configuration(&root_base)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("[source.pull_requests].base must not be empty")
	);

	let root_title = fixture_path("config/rejects-empty-pr-title");
	assert!(
		load_workspace_configuration(&root_title)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("[source.pull_requests].title must not be empty")
	);
}

#[test]
fn load_workspace_configuration_rejects_invalid_github_release_note_source_combinations() {
	let root = fixture_path("config/rejects-invalid-release-source");
	assert!(
		load_workspace_configuration(&root)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("generate_notes cannot be true")
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_pull_request_labels() {
	let root = fixture_path("config/rejects-empty-pr-labels");
	assert!(
		load_workspace_configuration(&root)
			.err()
			.unwrap_or_else(|| panic!("expected config error"))
			.to_string()
			.contains("[source.pull_requests].labels must not include empty values")
	);
}

#[test]
fn load_workspace_configuration_uses_defaults_package_type_when_type_is_omitted() {
	let root = fixture_path("config/defaults-package-type");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));
	assert_eq!(package.package_type, monochange_core::PackageType::Cargo);
}

#[test]
fn load_workspace_configuration_uses_defaults_changelog_pattern_when_package_changelog_is_omitted()
{
	let root = fixture_path("config/defaults-changelog-pattern");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));
	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::PathPattern(
			"{{ path }}/changelog.md".to_string()
		))
	);
	assert_eq!(
		package.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("crates/core/changelog.md"),
			format: ChangelogFormat::Monochange,
		})
	);
}

#[test]
fn load_workspace_configuration_supports_package_changelog_true_false_and_string() {
	let root = fixture_path("config/changelog-variants");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let core = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected core package"));
	let app = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("expected app package"));
	let tool = configuration
		.package_by_id("tool")
		.unwrap_or_else(|| panic!("expected tool package"));

	assert_eq!(
		configuration.defaults.changelog,
		Some(ChangelogDefinition::Disabled)
	);
	assert_eq!(
		core.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(app.changelog, None);
	assert_eq!(
		tool.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("docs/tool-release-notes.md"),
			format: ChangelogFormat::Monochange,
		})
	);
}

#[test]
fn load_workspace_configuration_supports_changelog_format_tables_and_overrides() {
	let root = fixture_path("config/changelog-format-tables");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let core = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected core package"));
	let app = configuration
		.package_by_id("app")
		.unwrap_or_else(|| panic!("expected app package"));
	let group = configuration
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected group"));

	assert_eq!(
		configuration.defaults.changelog_format,
		ChangelogFormat::KeepAChangelog
	);
	assert_eq!(
		core.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("crates/core/CHANGELOG.md"),
			format: ChangelogFormat::KeepAChangelog,
		})
	);
	assert_eq!(
		app.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("docs/app-release-notes.md"),
			format: ChangelogFormat::Monochange,
		})
	);
	assert_eq!(
		group.changelog,
		Some(ChangelogTarget {
			path: PathBuf::from("docs/group-release-notes.md"),
			format: ChangelogFormat::KeepAChangelog,
		})
	);
}

#[test]
fn load_workspace_configuration_supports_group_changelog_include_policies() {
	let root = fixture_path("config/group-changelog-include");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let sdk = configuration
		.group_by_id("sdk")
		.unwrap_or_else(|| panic!("expected sdk group"));
	let main = configuration
		.group_by_id("main")
		.unwrap_or_else(|| panic!("expected main group"));
	let docs = configuration
		.group_by_id("docs")
		.unwrap_or_else(|| panic!("expected docs group"));

	assert_eq!(sdk.changelog_include, GroupChangelogInclude::All);
	assert_eq!(main.changelog_include, GroupChangelogInclude::GroupOnly);
	assert_eq!(
		docs.changelog_include,
		GroupChangelogInclude::Selected(["api".to_string(), "site".to_string()].into())
	);
}

#[test]
fn load_workspace_configuration_rejects_invalid_group_changelog_include_members() {
	let root = fixture_path("config/rejects-group-changelog-include-invalid-member");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains(
		"group `sdk` changelog include entry `missing` must reference a package declared in that group"
	));
	assert!(rendered.contains("group changelog include member"));
}

#[test]
fn load_workspace_configuration_supports_empty_group_changelog_include_lists() {
	let root = fixture_path("config/group-changelog-include-empty-list");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let sdk = configuration
		.group_by_id("sdk")
		.unwrap_or_else(|| panic!("expected sdk group"));

	assert_eq!(sdk.changelog_include, GroupChangelogInclude::GroupOnly);
}

#[test]
fn load_workspace_configuration_rejects_invalid_group_changelog_include_modes() {
	let root = fixture_path("config/rejects-group-changelog-include-invalid-mode");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains(
		"group `sdk` changelog include must be `\"all\"`, `\"group-only\"`, or an array of member package ids"
	));
	assert!(rendered.contains("group changelog include"));
}

#[test]
fn load_workspace_configuration_rejects_empty_group_changelog_include_members() {
	let root = fixture_path("config/rejects-group-changelog-include-empty-member");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains("group `sdk` changelog include entries must not be empty"));
	assert!(rendered.contains("group changelog include member"));
}

#[test]
fn load_workspace_configuration_supports_empty_update_messages() {
	let root = fixture_path("config/empty-update-messages");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let core = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected core package"));
	let group = configuration
		.group_by_id("sdk")
		.unwrap_or_else(|| panic!("expected sdk group"));

	assert_eq!(
		configuration.defaults.empty_update_message.as_deref(),
		Some("No package-specific changes for {{ package }}; version is now {{ version }}.")
	);
	assert_eq!(
		core.empty_update_message.as_deref(),
		Some("Package override for {{ package }}@{{ version }}")
	);
	assert_eq!(
		group.empty_update_message.as_deref(),
		Some("Group fallback for {{ package }} from {{ group }}")
	);
}

#[test]
fn load_workspace_configuration_rejects_group_changelog_tables_without_paths() {
	let root = fixture_path("config/rejects-group-changelog-no-path");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();
	assert!(rendered.contains("group `sdk` changelog must declare a `path`"));
	assert!(rendered.contains("group changelog missing path"));
}

#[test]
fn load_workspace_configuration_requires_package_type_without_default() {
	let root = fixture_path("config/rejects-missing-type");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("must declare `type` or set `[defaults].package_type`"));
	assert!(rendered.contains("single-ecosystem repository"));
}

#[test]
fn load_workspace_configuration_rejects_package_group_namespace_collisions() {
	let root = fixture_path("config/rejects-namespace-collision");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("collides with an existing package or group id"));
	assert!(rendered.contains("package and group ids share one namespace"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_group_members() {
	let root = fixture_path("config/rejects-unknown-group-members");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("references unknown package `missing`"));
	assert!(rendered.contains("declare the package first under [package.<id>]"));
}

#[test]
fn load_workspace_configuration_rejects_multi_group_membership() {
	let root = fixture_path("config/rejects-multi-group-membership");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("error: package `core` belongs to multiple groups"));
	assert!(rendered.contains("--> monochange.toml"));
	assert!(rendered.contains("::: monochange.toml"));
	assert!(
		rendered.contains("= help: move the package into exactly one [group.<id>] declaration")
	);
	assert!(rendered.contains("= note: the first snippet marks the primary failure location"));
}

#[test]
fn load_workspace_configuration_rejects_duplicate_primary_version_format() {
	let root = fixture_path("config/rejects-duplicate-primary-version");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("primary release identity"));
	assert!(
		rendered
			.contains("choose a single package or group as the primary outward release identity")
	);
}

#[test]
fn load_workspace_configuration_rejects_unknown_versioned_file_dependencies() {
	let root = fixture_path("config/rejects-unknown-versioned-dep");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("unknown versioned file name `missing`"));
	assert!(rendered.contains(
		"reference a declared package id from `versioned_files` or remove the name entry"
	));
}

#[test]
fn load_workspace_configuration_infers_package_versioned_file_types_from_string_entries() {
	let root = fixture_path("config/infers-versioned-file-types");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));

	assert_eq!(package.versioned_files.len(), 2);
	assert!(
		package
			.versioned_files
			.iter()
			.all(|definition| definition.ecosystem_type == Some(EcosystemType::Cargo))
	);
}

#[test]
fn load_workspace_configuration_accepts_regex_versioned_files_without_explicit_type() {
	let root = fixture_path("config/regex-versioned-files");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.packages
		.first()
		.unwrap_or_else(|| panic!("expected package"));
	let definition = package
		.versioned_files
		.first()
		.unwrap_or_else(|| panic!("expected versioned file definition"));

	assert_eq!(definition.path, "README.md");
	assert_eq!(
		definition.regex.as_deref(),
		Some(r"https:\/\/example.com\/download\/v(?<version>\d+\.\d+\.\d+)\.tgz")
	);
	assert_eq!(definition.ecosystem_type, None);
	assert_eq!(definition.fields, None);
}

#[test]
fn load_workspace_configuration_rejects_regex_versioned_files_without_version_capture() {
	let root = fixture_path("config/rejects-regex-versioned-file-without-version-capture");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("must include a named `version` capture"));
}

#[test]
fn load_workspace_configuration_rejects_regex_versioned_files_with_type() {
	let root = fixture_path("config/rejects-regex-versioned-file-with-type");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("regex versioned_files cannot also set `type`"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_regex_versioned_file_patterns() {
	let root = fixture_path("config/rejects-invalid-regex-versioned-file");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("pattern `(` is invalid"));
}

#[test]
fn load_workspace_configuration_rejects_regex_versioned_files_with_prefix() {
	let root = fixture_path("config/rejects-regex-versioned-file-with-prefix");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(
		rendered.contains("regex versioned_files cannot also set `prefix`, `fields`, or `name`")
	);
}

#[test]
fn load_workspace_configuration_rejects_group_string_versioned_files_without_explicit_type() {
	let root = fixture_path("config/rejects-group-string-versioned");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("bare-string `versioned_files`"));
	assert!(rendered.contains("use `versioned_files = [{ path = \"...\", type = \"cargo\" }]`"));
}

#[test]
fn load_workspace_configuration_inherits_ecosystem_versioned_files_unless_package_opt_outs() {
	let root = fixture_path("config/ecosystem-versioned-files");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let app = configuration
		.packages
		.iter()
		.find(|package| package.id == "app")
		.unwrap_or_else(|| panic!("expected app package"));
	let web = configuration
		.packages
		.iter()
		.find(|package| package.id == "web")
		.unwrap_or_else(|| panic!("expected web package"));

	assert_eq!(app.versioned_files.len(), 1);
	assert_eq!(
		app.versioned_files
			.first()
			.map(|definition| definition.path.as_str()),
		Some("**/package.json")
	);
	assert!(web.ignore_ecosystem_versioned_files);
	assert_eq!(web.versioned_files.len(), 1);
	assert_eq!(
		web.versioned_files
			.first()
			.map(|definition| definition.path.as_str()),
		Some("package.json")
	);
}

#[test]
fn load_workspace_configuration_parses_ecosystem_lockfile_commands() {
	let root = fixture_path("config/lockfile-commands");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	assert_eq!(configuration.npm.lockfile_commands.len(), 3);
	let first_command = configuration
		.npm
		.lockfile_commands
		.first()
		.unwrap_or_else(|| panic!("expected first lockfile command"));
	assert_eq!(first_command.command, "npm install --package-lock-only");
	assert_eq!(
		first_command.cwd.as_deref(),
		Some(Path::new("packages/app"))
	);
	let second_command = configuration
		.npm
		.lockfile_commands
		.get(1)
		.unwrap_or_else(|| panic!("expected second lockfile command"));
	assert_eq!(
		second_command.shell,
		ShellConfig::Custom("bash".to_string())
	);
	let third_command = configuration
		.npm
		.lockfile_commands
		.get(2)
		.unwrap_or_else(|| panic!("expected third lockfile command"));
	assert!(third_command.cwd.is_none());
}

#[test]
fn load_workspace_configuration_rejects_empty_lockfile_commands() {
	let root = fixture_path("config/rejects-empty-lockfile-command");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(
		error
			.render()
			.contains("lockfile_commands must provide a non-empty command")
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_lockfile_command_cwds() {
	let root = fixture_path("config/rejects-empty-lockfile-command-cwd");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(
		error
			.render()
			.contains("lockfile_commands must provide a non-empty cwd when set")
	);
}

#[test]
fn load_workspace_configuration_rejects_lockfile_command_cwds_outside_the_workspace() {
	let root = fixture_path("config/rejects-lockfile-command-outside-workspace");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(
		error
			.render()
			.contains("lockfile_commands cwd `/tmp` must stay within the workspace root")
	);
}

#[test]
fn load_workspace_configuration_rejects_missing_lockfile_command_cwds() {
	let root = fixture_path("config/rejects-missing-lockfile-command-cwd");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(
		error.render().contains(
			"lockfile_commands cwd `packages/missing` does not exist or is not a directory"
		)
	);
}

#[test]
fn load_workspace_configuration_rejects_globs_that_match_unsupported_files_for_an_ecosystem() {
	let root = fixture_path("config/rejects-bad-glob");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("matched unsupported file"));
	assert!(rendered.contains("narrow the glob"));
}

#[test]
fn apply_version_groups_assigns_group_ids_and_detects_mismatched_versions() {
	let root = fixture_path("config/version-groups");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			Some(Version::new(2, 0, 0)),
			PublishState::Public,
		),
	];

	let (groups, warnings) = apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));
	let group = groups
		.first()
		.unwrap_or_else(|| panic!("expected one version group"));

	assert_eq!(group.group_id, "sdk");
	assert_eq!(group.members.len(), 2);
	assert!(group.mismatch_detected);
	let first_package = packages
		.first()
		.unwrap_or_else(|| panic!("expected first package"));
	let second_package = packages
		.get(1)
		.unwrap_or_else(|| panic!("expected second package"));
	assert_eq!(first_package.version_group_id.as_deref(), Some("sdk"));
	assert_eq!(second_package.version_group_id.as_deref(), Some("sdk"));
	assert_eq!(
		first_package.metadata.get("config_id").map(String::as_str),
		Some("core")
	);
	assert_eq!(
		second_package.metadata.get("config_id").map(String::as_str),
		Some("web")
	);
	assert_eq!(warnings.len(), 1);
}

#[test]
fn load_change_signals_resolves_configured_package_ids() {
	let root = fixture_path("config/change-signals-basic");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let mut packages = packages;
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	let signal = signals
		.first()
		.unwrap_or_else(|| panic!("expected one change signal"));

	let package = packages
		.first()
		.unwrap_or_else(|| panic!("expected discovered package"));
	assert_eq!(signal.package_id, package.id);
	assert_eq!(signal.requested_bump, Some(BumpSeverity::Minor));
	assert_eq!(signal.explicit_version, None);
	assert_eq!(signal.notes.as_deref(), Some("public API addition"));
	assert_eq!(signal.source_path, root.join("change.md"));
	assert!(signal.evidence_refs.is_empty());
}

#[test]
fn load_change_signals_parses_explicit_versions_and_infers_bumps() {
	let root = fixture_path("config/change-signals-explicit-version");
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let changeset = load_changeset_file(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("changeset file: {error}"));
	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected target"));
	let signal = changeset
		.signals
		.first()
		.unwrap_or_else(|| panic!("expected signal"));

	assert_eq!(target.bump, Some(BumpSeverity::Minor));
	assert_eq!(target.explicit_version, Some(Version::new(1, 2, 0)));
	assert_eq!(signal.requested_bump, Some(BumpSeverity::Minor));
	assert_eq!(signal.explicit_version, Some(Version::new(1, 2, 0)));
}

#[test]
fn markdown_heading_level_rejects_missing_separator_after_hashes() {
	assert_eq!(crate::markdown_heading_level("#Not a heading"), None);
}

#[test]
fn markdown_change_text_normalizes_relative_heading_levels() {
	let (summary, details) =
		crate::markdown_change_text("# Summary\n\n## Details\n\n### Sub-details\n\n- bullet");
	assert_eq!(summary.as_deref(), Some("Summary"));
	assert_eq!(
		details.as_deref(),
		Some("##### Details\n\n###### Sub-details\n\n- bullet")
	);
}

#[test]
fn markdown_change_text_normalizes_headings_after_plain_text_summary() {
	let (summary, details) = crate::markdown_change_text("Summary\n\n# Details\n\n## Sub-details");
	assert_eq!(summary.as_deref(), Some("Summary"));
	assert_eq!(
		details.as_deref(),
		Some("##### Details\n\n###### Sub-details")
	);
}

#[test]
fn markdown_change_text_clamps_deep_headings_and_preserves_code_fences() {
	let (summary, details) = crate::markdown_change_text(
		"# Summary\n\n###### Deep detail\n\n```md\n# leave code fences alone\n```",
	);
	assert_eq!(summary.as_deref(), Some("Summary"));
	assert_eq!(
		details.as_deref(),
		Some("###### Deep detail\n\n```md\n# leave code fences alone\n```")
	);
}

#[test]
fn load_change_signals_parses_markdown_change_types_and_details() {
	let root = fixture_path("config/change-signals-types-and-details");
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	let signal = signals.first().unwrap_or_else(|| panic!("expected signal"));

	assert_eq!(signal.change_type.as_deref(), Some("security"));
	assert_eq!(signal.source_path, root.join("change.md"));
	assert_eq!(
		signal.details.as_deref(),
		Some("Roll the signing key before the release window closes.")
	);
}

#[test]
fn load_change_signals_accept_group_scalar_type_shorthand_with_default_bump() {
	let root = fixture_path("config/change-signals-group-type-shorthand");
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"cargo-core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"cargo-app",
			root.join("crates/app/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let changeset = load_changeset_file(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("changeset file: {error}"));
	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected target"));
	assert_eq!(target.id, "sdk");
	assert_eq!(target.bump, Some(BumpSeverity::Minor));
	assert_eq!(target.change_type.as_deref(), Some("test"));
	assert!(
		changeset
			.signals
			.iter()
			.all(|signal| signal.requested_bump == Some(BumpSeverity::Minor))
	);
}

#[test]
fn load_change_signals_accept_type_only_scalar_shorthand_without_default_bump() {
	let root = fixture_path("config/change-signals-type-only-no-default-bump");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"cargo-core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));
	let signal = signals.first().unwrap_or_else(|| panic!("expected signal"));
	assert_eq!(signal.requested_bump, Some(BumpSeverity::None));
	assert_eq!(signal.change_type.as_deref(), Some("docs"));
}

#[test]
fn load_change_signals_reject_unknown_scalar_type_with_valid_types_help() {
	let root = fixture_path("config/rejects-change-unknown-type-configured");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	let rendered = error.to_string();
	assert!(rendered.contains("invalid scalar value `nope`"));
	assert!(rendered.contains("configured types: docs, test"));
}

#[test]
fn load_change_signals_reject_unknown_object_type_with_valid_types_help() {
	let root = fixture_path("config/rejects-change-unknown-object-type");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	let rendered = error.to_string();
	assert!(rendered.contains("invalid type `nope`"));
	assert!(rendered.contains("valid types: security"));
}

#[test]
fn load_change_signals_reject_object_type_when_target_has_no_configured_sections() {
	let root = fixture_path("config/rejects-change-object-type-without-configured-sections");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(
		error
			.to_string()
			.contains("no configured types are available for this target")
	);
}

#[test]
fn load_change_signals_reject_unknown_group_object_type_with_valid_types_help() {
	let root = fixture_path("config/rejects-group-change-unknown-object-type");
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"cargo-core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"cargo-app",
			root.join("crates/app/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	let rendered = error.to_string();
	assert!(rendered.contains("target `sdk` has invalid type `docs`"));
	assert!(rendered.contains("valid types: test"));
}

#[test]
fn load_change_signals_reject_none_bump_without_type_or_version() {
	let root = fixture_path("config/rejects-change-none-without-type-or-version");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(
		error
			.to_string()
			.contains("must not use `bump = \"none\"` without also declaring `type` or `version`")
	);
}

#[test]
fn load_change_signals_reject_invalid_object_bumps() {
	let root = fixture_path("config/rejects-change-invalid-object-bump");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(
		error
			.to_string()
			.contains("has invalid bump `nope`; expected `none`, `patch`, `minor`, or `major`")
	);
}

#[test]
fn validate_configured_change_type_accepts_known_type() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	crate::validate_configured_change_type(
		&configuration,
		Path::new("change.md"),
		"core",
		"security",
	)
	.unwrap_or_else(|error| panic!("validate type: {error}"));
}

#[test]
fn parse_markdown_change_target_accepts_unconfigured_object_type_literal() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("type: docs")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let parsed = crate::parse_markdown_change_target(
		&value,
		Path::new("change.md"),
		"unknown-target",
		&configuration,
	)
	.unwrap_or_else(|error| panic!("parse target: {error}"));
	assert_eq!(parsed, (None, None, Some("docs".to_string())));
}

#[test]
fn parse_markdown_change_target_accepts_unconfigured_scalar_type_literal() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("docs")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let parsed = crate::parse_markdown_change_target(
		&value,
		Path::new("change.md"),
		"unknown-target",
		&configuration,
	)
	.unwrap_or_else(|error| panic!("parse target: {error}"));
	assert_eq!(parsed, (None, None, Some("docs".to_string())));
}

#[test]
fn parse_markdown_change_target_rejects_non_scalar_non_mapping_values() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("[docs]")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let error =
		crate::parse_markdown_change_target(&value, Path::new("change.md"), "core", &configuration)
			.expect_err("sequence values should fail");
	assert!(
		error
			.to_string()
			.contains("must map to `none`, `patch`, `minor`, `major`, a configured change type")
	);
}

#[test]
fn parse_markdown_change_target_rejects_empty_mapping() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let value = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("{}")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let error =
		crate::parse_markdown_change_target(&value, Path::new("change.md"), "core", &configuration)
			.expect_err("empty mapping should fail");
	assert!(
		error
			.to_string()
			.contains("must declare `bump`, `version`, `type`, or a valid scalar shorthand")
	);
}

#[test]
fn configured_change_sections_fall_back_to_empty_for_unknown_targets() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(crate::configured_change_sections(&configuration, "unknown").is_empty());
}

#[test]
fn load_change_signals_rejects_markdown_without_frontmatter() {
	let root = fixture_path("config/rejects-change-no-frontmatter");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(error.to_string().contains("missing markdown frontmatter"));
}

#[test]
fn load_change_signals_rejects_unterminated_markdown_frontmatter() {
	let root = fixture_path("config/rejects-change-unterminated");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(
		error
			.to_string()
			.contains("unterminated markdown frontmatter")
	);
}

#[test]
fn load_change_signals_rejects_invalid_markdown_bumps() {
	let root = fixture_path("config/rejects-change-invalid-bump");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(error.to_string().contains("invalid scalar value `note`"));
}

#[test]
fn load_change_signals_rejects_duplicate_package_entries() {
	let root = fixture_path("config/rejects-change-duplicate-entries");
	let mut packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let rendered = load_change_signals(&root.join("change.toml"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected duplicate entry error"))
		.render();
	assert!(rendered.contains("duplicate change entry"));
}

#[test]
fn load_change_signals_expands_group_targets_into_member_packages() {
	let root = fixture_path("config/change-signals-group-expand");
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));

	assert_eq!(signals.len(), 2);
	assert!(
		signals
			.iter()
			.all(|signal| signal.requested_bump == Some(BumpSeverity::Minor))
	);
}

#[test]
fn load_change_signals_handles_mixed_group_and_member_targets() {
	let root = fixture_path("config/change-signals-group-mixed");
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let signals = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("change signals: {error}"));

	assert_eq!(
		signals.len(),
		2,
		"expected exactly 2 signals (no duplicates)"
	);
	let core_signal = signals
		.iter()
		.find(|signal| signal.package_id.contains("core"))
		.unwrap_or_else(|| panic!("expected core signal"));
	let web_signal = signals
		.iter()
		.find(|signal| signal.package_id.contains("web"))
		.unwrap_or_else(|| panic!("expected web signal"));

	assert_eq!(
		core_signal.requested_bump,
		Some(BumpSeverity::Patch),
		"explicitly-listed member should get its own bump"
	);
	assert_eq!(
		web_signal.requested_bump,
		Some(BumpSeverity::Minor),
		"unlisted member should get the group bump"
	);
}

#[test]
fn load_change_signals_rejects_invalid_explicit_versions() {
	let root = fixture_path("config/rejects-change-invalid-version");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let error = load_change_signals(&root.join("change.md"), &configuration, &packages)
		.err()
		.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(error.to_string().contains("invalid version `nope`"));
}

#[test]
fn load_changeset_file_preserves_group_targets_and_source_paths() {
	let root = fixture_path("config/changeset-file-group-targets");
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"web",
			root.join("packages/web/package.json"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let changeset = load_changeset_file(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("changeset file: {error}"));

	assert_eq!(changeset.path, root.join("change.md"));
	assert_eq!(changeset.summary.as_deref(), Some("grouped release"));
	assert_eq!(changeset.targets.len(), 1);
	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected one changeset target"));
	assert_eq!(target.id, "sdk");
	assert_eq!(target.kind.as_str(), "group");
	assert_eq!(target.origin, "direct-change");
	assert_eq!(target.explicit_version, None);
	assert_eq!(changeset.signals.len(), 2);
	assert!(
		changeset
			.signals
			.iter()
			.all(|signal| signal.source_path == root.join("change.md"))
	);
}

#[test]
fn resolve_package_reference_rejects_ambiguous_package_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"shared",
			tempdir.path().join("crates/core/Cargo.toml"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Npm,
			"shared",
			tempdir.path().join("packages/shared/package.json"),
			tempdir.path().to_path_buf(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let error = resolve_package_reference("shared", tempdir.path(), &packages)
		.err()
		.unwrap_or_else(|| panic!("expected ambiguous package error"));
	assert!(error.to_string().contains("matched multiple packages"));
}

#[test]
fn validate_workspace_accepts_changesets_that_mix_group_and_member_references() {
	let root = fixture_path("config/accepts-mixed-changeset");
	validate_workspace(&root)
		.unwrap_or_else(|error| panic!("should accept mixed group+member references: {error}"));
}

#[test]
fn load_workspace_configuration_rejects_publish_release_without_source_config() {
	let root = fixture_path("config/rejects-publish-no-github");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected github CLI command config error"));
	assert!(
		error
			.to_string()
			.contains("uses `PublishRelease` but `[source]` is not configured")
	);
}

#[test]
fn load_workspace_configuration_assigns_default_publish_registries_per_ecosystem() {
	let root = fixture_path("config/publish-default-registries");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let packages = configuration
		.packages
		.iter()
		.map(|package| (package.id.as_str(), &package.publish))
		.collect::<std::collections::BTreeMap<_, _>>();

	assert_eq!(
		packages
			.get("core")
			.and_then(|publish| publish.registry.as_ref()),
		Some(&PublishRegistry::Builtin(RegistryKind::CratesIo))
	);
	assert_eq!(
		packages
			.get("web")
			.and_then(|publish| publish.registry.as_ref()),
		Some(&PublishRegistry::Builtin(RegistryKind::Npm))
	);
	assert_eq!(
		packages
			.get("jsr_pkg")
			.and_then(|publish| publish.registry.as_ref()),
		Some(&PublishRegistry::Builtin(RegistryKind::Jsr))
	);
	assert_eq!(
		packages
			.get("dart_pkg")
			.and_then(|publish| publish.registry.as_ref()),
		Some(&PublishRegistry::Builtin(RegistryKind::PubDev))
	);
	assert!(packages.values().all(|publish| publish.enabled));
	assert!(
		packages
			.values()
			.all(|publish| publish.mode == PublishMode::Builtin)
	);
	assert!(
		packages
			.values()
			.all(|publish| publish.trusted_publishing.enabled)
	);
}

#[test]
fn load_workspace_configuration_allows_package_publish_placeholder_to_override_ecosystem_default() {
	let root = fixture_path("config/publish-placeholder-package-override");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.package_by_id("web")
		.unwrap_or_else(|| panic!("expected web package"));

	assert_eq!(package.publish.placeholder.readme, None);
	assert_eq!(
		package.publish.placeholder.readme_file.as_deref(),
		Some(Path::new("docs/web-placeholder.md"))
	);
}

#[test]
fn load_workspace_configuration_merges_trusted_publishing_details() {
	let root = fixture_path("config/publish-trusted-publishing-overrides");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.package_by_id("web")
		.unwrap_or_else(|| panic!("expected web package"));

	assert!(package.publish.trusted_publishing.enabled);
	assert_eq!(
		package.publish.trusted_publishing.repository.as_deref(),
		Some("ifiokjr/monochange")
	);
	assert_eq!(
		package.publish.trusted_publishing.workflow.as_deref(),
		Some("publish.yml")
	);
	assert_eq!(
		package.publish.trusted_publishing.environment.as_deref(),
		Some("publisher")
	);
}

#[test]
fn load_workspace_configuration_rejects_builtin_publish_registry_override() {
	let root = fixture_path("config/rejects-publish-builtin-registry-override");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected builtin publish registry error"));
	assert!(
		error.to_string().contains(
			"package `core` uses built-in publishing with an unsupported registry override"
		)
	);
	assert!(error.to_string().contains("mode = \"external\""));
}

#[test]
fn load_workspace_configuration_rejects_open_release_pull_request_without_source_config() {
	let root = fixture_path("config/rejects-pr-no-github");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected github CLI command config error"));
	assert!(
		error
			.to_string()
			.contains("uses `OpenReleaseRequest` but `[source]` is not configured")
	);
}

#[test]
fn load_workspace_configuration_rejects_comment_released_issues_for_unsupported_provider() {
	let root_gitlab = fixture_path("config/rejects-comment-unsupported");
	let error = load_workspace_configuration(&root_gitlab)
		.err()
		.unwrap_or_else(|| panic!("expected provider capability error"));
	assert!(error.to_string().contains(
		"uses `CommentReleasedIssues` but `[source].provider = \"gitlab\"` does not support released-issue comments"
	));

	let root_gitea = fixture_path("config/rejects-comment-unsupported-gitea");
	let error = load_workspace_configuration(&root_gitea)
		.err()
		.unwrap_or_else(|| panic!("expected provider capability error"));
	assert!(error.to_string().contains(
		"uses `CommentReleasedIssues` but `[source].provider = \"gitea\"` does not support released-issue comments"
	));
}

#[test]
fn load_workspace_configuration_accepts_comment_released_issues_for_github() {
	let root = fixture_path("config/accepts-comment-github");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(
		configuration
			.cli
			.iter()
			.any(|command| command.name == "comment")
	);
}

#[test]
fn load_workspace_configuration_rejects_enforce_changeset_policy_without_github_bot_config() {
	let root = fixture_path("config/rejects-enforce-no-bot");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected verification CLI command config error"));
	assert!(
		error
			.to_string()
			.contains("uses `AffectedPackages` but `[changesets.verify].enabled` is false")
	);
}

#[test]
fn load_workspace_configuration_rejects_affected_packages_without_path_inputs() {
	let root = fixture_path("config/rejects-affected-no-path");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected verification CLI command config error"));
	assert!(
		error
			.to_string()
			.contains("declares neither a `changed_paths` nor a `since` input")
	);
}

#[test]
fn load_workspace_configuration_accepts_affected_packages_step_input_overrides() {
	let root = fixture_path("config/affected-step-overrides");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(
		configuration
			.cli
			.iter()
			.any(|command| command.name == "pr-check")
	);
}

#[test]
fn load_workspace_configuration_accepts_affected_packages_with_since_in_step_override() {
	let root = fixture_path("config/affected-step-since");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(
		configuration
			.cli
			.iter()
			.any(|command| command.name == "pr-check")
	);
}

#[test]
fn load_workspace_configuration_rejects_affected_packages_when_step_override_provides_no_path_source()
 {
	let root = fixture_path("config/rejects-affected-no-source");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("declares neither a `changed_paths` nor a `since` input")
	);
}

#[test]
fn load_workspace_configuration_rejects_step_override_with_boolean_for_non_boolean_input() {
	let root = fixture_path("config/rejects-bool-for-non-bool");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("override `changed_paths` must use a"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_step_override_with_list_for_boolean_input() {
	let root = fixture_path("config/rejects-list-for-bool");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error.to_string().contains("override `verify` must use a"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_unknown_step_input_override() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-discover");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("unknown input override `nonexistent`"),
		"error was: {error}"
	);
	assert!(
		error.to_string().contains("valid inputs: format"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_unknown_input_on_validate_step() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-validate");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("unknown input override `format`"),
		"error was: {error}"
	);
	assert!(
		error.to_string().contains("this step accepts no inputs"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_allows_any_input_on_command_step() {
	let root = fixture_path("validate-step-inputs/any-input-on-command");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let build_cmd = configuration
		.cli
		.iter()
		.find(|c| c.name == "build")
		.unwrap_or_else(|| panic!("expected build command"));
	assert_eq!(build_cmd.steps.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_wrong_type_format_override_on_discover() {
	let root = fixture_path("validate-step-inputs/wrong-type-format-on-discover");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("override `format` must use a string value"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_list_for_string_input_on_change_step() {
	let root = fixture_path("validate-step-inputs/list-for-string-on-change");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("override `reason` must use a string value"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_accepts_valid_diagnose_changesets_step_inputs() {
	let root = fixture_path("validate-step-inputs/valid-diagnose-inputs");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let diag_cmd = configuration
		.cli
		.iter()
		.find(|c| c.name == "diag")
		.unwrap_or_else(|| panic!("expected diag command"));
	assert_eq!(diag_cmd.steps.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_unknown_input_on_diagnose_changesets() {
	let root = fixture_path("validate-step-inputs/unknown-input-on-diagnose");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.to_string()
			.contains("unknown input override `verbose`"),
		"error was: {error}"
	);
	assert!(
		error
			.to_string()
			.contains("valid inputs: format, changeset"),
		"error was: {error}"
	);
}

#[test]
fn load_workspace_configuration_parses_release_note_customization() {
	let root = fixture_path("config/release-note-customization");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected package"));

	assert_eq!(configuration.release_notes.change_templates.len(), 3);
	assert_eq!(package.extra_changelog_sections.len(), 1);
	let extra_section = package
		.extra_changelog_sections
		.first()
		.unwrap_or_else(|| panic!("expected extra changelog section"));
	assert_eq!(extra_section.name, "Security");
	assert_eq!(extra_section.types, vec!["security"]);
	assert_eq!(extra_section.default_bump, Some(BumpSeverity::Patch));
	assert_eq!(extra_section.description, None);
}

#[test]
fn load_workspace_configuration_parses_extra_changelog_section_with_description() {
	let root = fixture_path("config/release-note-customization-with-description");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected package"));

	assert_eq!(package.extra_changelog_sections.len(), 1);
	let extra_section = package
		.extra_changelog_sections
		.first()
		.unwrap_or_else(|| panic!("expected extra changelog section"));
	assert_eq!(extra_section.name, "Testing");
	assert_eq!(extra_section.types, vec!["test"]);
	assert_eq!(extra_section.default_bump, Some(BumpSeverity::None));
	assert_eq!(
		extra_section.description,
		Some("Changes that only modify tests".to_string())
	);
}

#[test]
fn section_patterns_support_root_sections_without_ids() {
	assert_eq!(
		crate::section_patterns("defaults", ""),
		["[defaults]".to_string(), "[defaults]".to_string()]
	);
}

#[test]
fn load_workspace_configuration_inherits_default_extra_changelog_sections() {
	let root = fixture_path("config/default-extra-changelog-sections");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let package = configuration
		.package_by_id("core")
		.unwrap_or_else(|| panic!("expected package"));

	assert_eq!(configuration.defaults.extra_changelog_sections.len(), 1);
	assert_eq!(package.extra_changelog_sections.len(), 1);
	let extra_section = package
		.extra_changelog_sections
		.first()
		.unwrap_or_else(|| panic!("expected extra changelog section"));
	assert_eq!(extra_section.name, "Security");
	assert_eq!(extra_section.types, vec!["security"]);
}

#[test]
fn load_workspace_configuration_rejects_empty_extra_changelog_section_names() {
	let root = fixture_path("config/rejects-empty-section-names");
	let rendered = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("empty `name`"));
}

#[test]
fn load_workspace_configuration_rejects_empty_extra_changelog_section_types() {
	let root = fixture_path("config/rejects-empty-section-types");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	let rendered = error.render();

	assert!(rendered.contains("extra changelog section `Security` must declare at least one type"));
}

#[test]
fn load_workspace_configuration_rejects_empty_extra_changelog_section_type_values() {
	let root = fixture_path("config/rejects-empty-section-type-values");
	let rendered = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.render();
	assert!(rendered.contains("must not include empty types"));
}

#[test]
fn load_workspace_configuration_rejects_unknown_change_template_variables() {
	let root = fixture_path("config/rejects-unknown-template-vars");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(
		error
			.render()
			.contains("unsupported variables: commit_hash")
	);
}

#[test]
fn load_workspace_configuration_rejects_reserved_cli_command_names() {
	let root = fixture_path("config/rejects-reserved-cli-names");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error.to_string().contains("reserved built-in command"));
}

#[test]
fn load_workspace_configuration_rejects_duplicate_cli_command_tables() {
	let root = fixture_path("config/rejects-duplicate-cli");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error.to_string().contains("failed to parse"));
}

#[test]
fn load_workspace_configuration_rejects_unsupported_workflows_namespace() {
	let root = fixture_path("config/rejects-legacy-workflows");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error.to_string().contains("unknown field `workflows`"));
}

#[test]
fn load_change_signals_rejects_unknown_package_references_with_diagnostic_help() {
	let root = fixture_path("config/rejects-change-unknown-pkg");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let error = load_change_signals(&root.join("change.md"), &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("error: changeset `"));
	assert!(rendered.contains("unknown package or group `missing-package`"));
	assert!(rendered.contains("help: declare the package or group id in monochange.toml"));
}

#[test]
fn load_change_signals_reports_pretty_frontmatter_parse_errors_with_fix_hint() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.\"@monochange/skill\"]\npath = \"crates/core\"\ntype = \"cargo\"\n",
	)
	.unwrap_or_else(|error| panic!("write config: {error}"));
	std::fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir crate: {error}"));
	std::fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2024\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo manifest: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let change_path = tempdir.path().join("change.md");
	std::fs::write(
		&change_path,
		"---\n@monochange/skill: patch\n---\n\n# broken\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));

	let error = load_change_signals(&change_path, &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(
		rendered.contains("error: failed to parse"),
		"rendered: {rendered}"
	);
	assert!(
		rendered.contains(&format!("--> {}:2:1", change_path.display())),
		"rendered: {rendered}"
	);
	assert!(
		rendered.contains("2 | @monochange/skill: patch"),
		"rendered: {rendered}"
	);
	assert!(
		rendered.contains("wrap package or group ids that contain characters like `@`, `/`, `:`, or spaces in double quotes"),
		"rendered: {rendered}"
	);
}

#[test]
fn source_diagnostic_helpers_cover_empty_labels_sorting_and_fallback_spans() {
	let source = "alpha\nbeta\ngamma";
	let empty = render_source_diagnostic("demo.md", source, "plain failure", &[], None);
	assert!(empty.contains("error: plain failure"), "{empty}");
	assert!(empty.contains("  --> demo.md:1:1"), "{empty}");
	assert!(!empty.contains("  = help:"), "{empty}");
	assert!(!empty.contains("  = note:"), "{empty}");

	let labels = vec![
		LabeledSpan::new_with_span(Some("primary".to_string()), range_to_span(6..10)),
		LabeledSpan::new_with_span(Some("later".to_string()), range_to_span(11..16)),
		LabeledSpan::new_with_span(Some("earlier".to_string()), range_to_span(0..0)),
	];
	let sorted = sort_labels_by_location(&labels);
	assert_eq!(sorted.first().and_then(LabeledSpan::label), Some("primary"));
	assert_eq!(sorted.get(1).and_then(LabeledSpan::label), Some("earlier"));
	assert_eq!(sorted.get(2).and_then(LabeledSpan::label), Some("later"));

	let rendered = render_source_diagnostic(
		"demo.md",
		source,
		"annotated failure",
		&labels,
		Some("try fixing it"),
	);
	assert!(rendered.contains("  --> demo.md:2:1"), "{rendered}");
	assert!(rendered.contains("  ::: demo.md:1:1"), "{rendered}");
	assert!(rendered.contains("  ::: demo.md:3:1"), "{rendered}");
	assert!(rendered.contains("^ primary"), "{rendered}");
	assert!(rendered.contains("^ earlier"), "{rendered}");
	assert!(rendered.contains("^ later"), "{rendered}");
	assert!(rendered.contains("  = help: try fixing it"), "{rendered}");
	assert!(
		rendered.contains("  = note: the first snippet marks the primary failure location"),
		"{rendered}"
	);

	let secondary = render_source_snippet(
		"demo.md",
		source,
		&LabeledSpan::new_with_span(None, range_to_span(0..0)),
		false,
	);
	assert_eq!(secondary.first(), Some(&"  ::: demo.md:1:1".to_string()));
	assert!(
		secondary.iter().any(|line| line.contains("^ here")),
		"{secondary:?}"
	);

	assert!(render_source_snippets("demo.md", source, &[]).is_empty());
	assert!(sort_labels_by_location(&[]).is_empty());
	assert!(render_diagnostic_notes(&[]).is_empty());
	let single_label = labels
		.first()
		.cloned()
		.map(|label| vec![label])
		.unwrap_or_default();
	assert!(render_diagnostic_notes(&single_label).is_empty());
	assert_eq!(line_index_for_offset(source, usize::MAX), 2);
	assert_eq!(line_and_column_for_offset(source, usize::MAX), (3, 6));
	assert_eq!(frontmatter_span_for_line_column(source, 2, 99), 10..11);
	assert_eq!(
		frontmatter_span_for_line_column(source, 99, 1),
		source.len()..source.len()
	);
}

#[test]
fn load_workspace_configuration_rejects_unsupported_github_namespace() {
	let root = fixture_path("config/rejects-source-and-github");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error.to_string().contains("unknown field `github`"));
}

#[test]
fn load_change_signals_infers_group_bump_from_member_explicit_version() {
	let root = fixture_path("config/change-signals-group-explicit");
	let mut packages = vec![
		PackageRecord::new(
			Ecosystem::Cargo,
			"core",
			root.join("crates/core/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
		PackageRecord::new(
			Ecosystem::Cargo,
			"app",
			root.join("crates/app/Cargo.toml"),
			root.clone(),
			Some(Version::new(1, 0, 0)),
			PublishState::Public,
		),
	];
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	apply_version_groups(&mut packages, &configuration)
		.unwrap_or_else(|error| panic!("version groups: {error}"));

	let changeset = load_changeset_file(&root.join("change.md"), &configuration, &packages)
		.unwrap_or_else(|error| panic!("changeset file: {error}"));

	let target = changeset
		.targets
		.first()
		.unwrap_or_else(|| panic!("expected target"));
	assert_eq!(target.bump, Some(BumpSeverity::Major));
	assert_eq!(target.explicit_version, Some(Version::new(2, 0, 0)));
}

#[test]
fn load_workspace_configuration_accepts_detailed_and_enabled_true_changelog_in_defaults() {
	let root = fixture_path("config/defaults-changelog-enabled-true");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(!configuration.packages.is_empty());
}

#[test]
fn load_workspace_configuration_accepts_detailed_changelog_disabled_in_defaults() {
	let root = fixture_path("config/defaults-changelog-disabled");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(!configuration.packages.is_empty());
}

#[test]
fn load_workspace_configuration_accepts_detailed_changelog_enabled_with_no_path_in_defaults() {
	let root = fixture_path("config/defaults-changelog-no-path");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert!(!configuration.packages.is_empty());
}

fn package_definition(id: &str, path: &str) -> monochange_core::PackageDefinition {
	monochange_core::PackageDefinition {
		id: id.to_string(),
		path: PathBuf::from(path),
		package_type: monochange_core::PackageType::Cargo,
		changelog: None,
		extra_changelog_sections: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Namespaced,
		publish: monochange_core::PublishSettings::default(),
	}
}

fn cli_input(name: &str, kind: CliInputKind) -> CliInputDefinition {
	CliInputDefinition {
		name: name.to_string(),
		kind,
		help_text: None,
		required: false,
		default: None,
		choices: Vec::new(),
		short: None,
	}
}

fn cli_command(name: &str, steps: Vec<CliStepDefinition>) -> CliCommandDefinition {
	CliCommandDefinition {
		name: name.to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps,
	}
}

fn sample_source_configuration(provider: SourceProvider) -> monochange_core::SourceConfiguration {
	monochange_core::SourceConfiguration {
		provider,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: monochange_core::ProviderReleaseSettings {
			enabled: true,
			..Default::default()
		},
		pull_requests: monochange_core::ProviderMergeRequestSettings {
			enabled: true,
			..Default::default()
		},
		bot: monochange_core::ProviderBotSettings::default(),
	}
}

#[test]
fn load_workspace_configuration_rejects_duplicate_command_step_ids() {
	let root = fixture_path("config/rejects-duplicate-step-ids");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected error for duplicate step ids"));
	assert!(
		error.to_string().contains("duplicate step id"),
		"error: {error}"
	);
}

#[test]
fn load_workspace_configuration_rejects_empty_command_step_id() {
	let root = fixture_path("config/rejects-empty-step-id");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected error for empty step id"));
	assert!(error.to_string().contains("empty id"), "error: {error}");
}

#[test]
fn load_changeset_file_reports_io_and_toml_parse_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let missing = load_changeset_file(&tempdir.path().join("missing.toml"), &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected missing file error"));
	assert!(missing.to_string().contains("failed to read"));

	let invalid = tempdir.path().join("invalid.toml");
	std::fs::write(&invalid, "changes = [")
		.unwrap_or_else(|error| panic!("write invalid: {error}"));
	let parse_error = load_changeset_file(&invalid, &configuration, &[])
		.err()
		.unwrap_or_else(|| panic!("expected parse error"));
	assert!(parse_error.to_string().contains("failed to parse"));
}

#[test]
fn resolve_package_reference_reports_missing_and_ambiguous_matches() {
	let root = PathBuf::from("/workspace");
	let package_a = PackageRecord::new(
		Ecosystem::Cargo,
		"shared",
		root.join("crates/shared/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let package_b = PackageRecord::new(
		Ecosystem::Cargo,
		"shared",
		root.join("packages/shared/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);

	let missing =
		resolve_package_reference("missing", &root, &[package_a.clone(), package_b.clone()])
			.err()
			.unwrap_or_else(|| panic!("expected missing package error"));
	assert!(
		missing
			.to_string()
			.contains("did not match any discovered package")
	);

	let ambiguous = resolve_package_reference("shared", &root, &[package_a, package_b])
		.err()
		.unwrap_or_else(|| panic!("expected ambiguous package error"));
	assert!(ambiguous.to_string().contains("matched multiple packages"));
}

#[test]
fn load_workspace_configuration_rejects_empty_step_name() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::fs::create_dir_all(root.join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir core: {error}"));
	std::fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo: {error}"));
	std::fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"
name = "   "
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange: {error}"));

	let error = load_workspace_configuration(root)
		.err()
		.unwrap_or_else(|| panic!("expected error for empty step name"));
	assert!(error.to_string().contains("empty `name`"), "error: {error}");
}

#[test]
fn load_workspace_configuration_rejects_duplicate_step_names() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	std::fs::create_dir_all(root.join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir core: {error}"));
	std::fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo: {error}"));
	std::fs::write(
		root.join("monochange.toml"),
		r#"[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[cli.release]
[[cli.release.steps]]
type = "PrepareRelease"
name = "plan release"

[[cli.release.steps]]
type = "Command"
name = "plan release"
command = "echo hi"
"#,
	)
	.unwrap_or_else(|error| panic!("write monochange: {error}"));

	let error = load_workspace_configuration(root)
		.err()
		.unwrap_or_else(|| panic!("expected error for duplicate step names"));
	assert!(
		error.to_string().contains("duplicate step name"),
		"error: {error}"
	);
}

#[test]
fn load_workspace_configuration_accepts_command_step_with_shell_string() {
	let root = fixture_path("config/accepts-shell-string");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	let test_cmd = configuration
		.cli
		.iter()
		.find(|c| c.name == "test")
		.unwrap_or_else(|| panic!("expected test command"));
	match test_cmd.steps.first() {
		Some(CliStepDefinition::Command { shell, id, .. }) => {
			assert_eq!(*shell, ShellConfig::Custom("bash".to_string()));
			assert_eq!(id.as_deref(), Some("greet"));
		}
		_ => panic!("expected Command step"),
	}
}

#[test]
fn raw_changelog_config_resolves_package_and_group_paths() {
	let package_path = Path::new("packages/core");
	let legacy_disabled =
		crate::RawChangelogConfig::Legacy(crate::RawChangelogDefinition::Enabled(false));
	assert!(legacy_disabled.is_disabled());
	assert_eq!(
		legacy_disabled.resolve_for_package(package_path, true),
		None
	);
	assert_eq!(legacy_disabled.resolve_for_group(), None);

	let legacy_pattern = crate::RawChangelogConfig::Legacy(crate::RawChangelogDefinition::Path(
		"{{ path }}/notes.md".to_string(),
	));
	assert_eq!(
		legacy_pattern.resolve_for_package(package_path, true),
		Some(PathBuf::from("packages/core/notes.md"))
	);
	assert_eq!(
		legacy_pattern.resolve_for_package(package_path, false),
		Some(PathBuf::from("{{ path }}/notes.md"))
	);
	assert_eq!(
		legacy_pattern.resolve_for_group(),
		Some(PathBuf::from("{{ path }}/notes.md"))
	);

	let detailed_disabled = crate::RawChangelogConfig::Detailed(crate::RawChangelogTable {
		enabled: Some(false),
		path: Some("group/CHANGELOG.md".to_string()),
		format: None,
		include: None,
	});
	assert!(detailed_disabled.is_disabled());
	assert_eq!(
		detailed_disabled.resolve_for_package(package_path, true),
		None
	);
	assert_eq!(detailed_disabled.resolve_for_group(), None);

	let detailed_default = crate::RawChangelogConfig::Detailed(crate::RawChangelogTable {
		enabled: Some(true),
		path: None,
		format: None,
		include: Some(crate::RawGroupChangelogInclude::Mode(
			"group-only".to_string(),
		)),
	});
	assert_eq!(
		detailed_default.resolve_for_package(package_path, true),
		Some(PathBuf::from("packages/core/CHANGELOG.md"))
	);
	assert_eq!(detailed_default.resolve_for_group(), None);
	assert!(matches!(
		detailed_default.include(),
		Some(crate::RawGroupChangelogInclude::Mode(mode)) if mode == "group-only"
	));
}

#[test]
fn infer_bump_helpers_cover_major_minor_patch_and_none() {
	assert_eq!(
		crate::infer_bump_from_versions(&Version::new(1, 2, 3), &Version::new(2, 0, 0)),
		BumpSeverity::Major
	);
	assert_eq!(
		crate::infer_bump_from_versions(&Version::new(1, 2, 3), &Version::new(1, 3, 0)),
		BumpSeverity::Minor
	);
	assert_eq!(
		crate::infer_bump_from_versions(&Version::new(1, 2, 3), &Version::new(1, 2, 4)),
		BumpSeverity::Patch
	);
	assert_eq!(
		crate::infer_bump_from_versions(
			&Version::new(1, 2, 3),
			&Version::parse("1.2.3-beta.1").unwrap_or_else(|error| panic!("version: {error}"))
		),
		BumpSeverity::Patch
	);
	assert_eq!(
		crate::infer_bump_from_versions(&Version::new(1, 2, 3), &Version::new(1, 2, 3)),
		BumpSeverity::None
	);

	let workspace_root = PathBuf::from("/workspace");
	let core = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		workspace_root.join("crates/core/Cargo.toml"),
		workspace_root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let app = PackageRecord::new(
		Ecosystem::Cargo,
		"app",
		workspace_root.join("crates/app/Cargo.toml"),
		workspace_root.clone(),
		Some(Version::new(2, 0, 0)),
		PublishState::Public,
	);
	let group = monochange_core::GroupDefinition {
		id: "sdk".to_string(),
		packages: vec![core.id.clone(), app.id.clone()],
		changelog: None,
		changelog_include: GroupChangelogInclude::All,
		extra_changelog_sections: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Primary,
	};
	assert_eq!(
		crate::infer_group_bump_from_explicit_version(
			&group,
			&workspace_root,
			&[core.clone(), app.clone()],
			Some(&Version::new(2, 1, 0))
		)
		.unwrap_or_else(|error| panic!("expected Ok, got: {error}")),
		Some(BumpSeverity::Minor)
	);

	// A group with an unresolvable member should now produce an error
	// instead of silently dropping the member.
	let group_with_missing = monochange_core::GroupDefinition {
		id: "sdk-missing".to_string(),
		packages: vec![core.id.clone(), "missing".to_string(), app.id.clone()],
		changelog: None,
		changelog_include: GroupChangelogInclude::All,
		extra_changelog_sections: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Primary,
	};
	let error = crate::infer_group_bump_from_explicit_version(
		&group_with_missing,
		&workspace_root,
		&[core.clone(), app.clone()],
		Some(&Version::new(2, 1, 0)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected error for unresolvable group member"));
	assert!(error.to_string().contains("missing"));
	let core_id = core.id.clone();
	assert_eq!(
		crate::infer_package_bump_from_explicit_version(
			&core_id,
			&[core, app],
			Some(&Version::new(1, 0, 1))
		),
		Some(BumpSeverity::Patch)
	);
	assert_eq!(
		crate::infer_group_bump_from_explicit_version(&group, &workspace_root, &[], None)
			.unwrap_or_else(|error| panic!("expected Ok, got: {error}")),
		None
	);
}

#[test]
fn validate_source_and_changeset_settings_reject_empty_values() {
	let source_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: " ".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected source validation error"));
	assert!(
		source_error
			.to_string()
			.contains("[source].owner must not be empty")
	);

	let changeset_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings {
			verify: monochange_core::ChangesetVerificationSettings {
				skip_labels: vec![String::new()],
				..Default::default()
			},
		},
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected changeset validation error"));
	assert!(
		changeset_error
			.to_string()
			.contains("[changesets.verify].skip_labels must not include empty values")
	);

	let source_skip_label_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings {
				changesets: monochange_core::ProviderChangesetBotSettings {
					enabled: true,
					required: true,
					skip_labels: vec![String::new()],
					comment_on_failure: true,
					changed_paths: Vec::new(),
					ignored_paths: Vec::new(),
				},
			},
		}))
		.err()
		.unwrap_or_else(|| panic!("expected source skip label validation error"));
	assert!(
		source_skip_label_error
			.to_string()
			.contains("[source.bot.changesets].skip_labels must not include empty values")
	);

	let source_repo_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: "ifiokjr".to_string(),
			repo: " ".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected source repo validation error"));
	assert!(
		source_repo_error
			.to_string()
			.contains("[source].repo must not be empty")
	);
}

#[test]
fn validate_changesets_configuration_rejects_invalid_additional_path_globs() {
	let error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings::default(),
		&[monochange_core::PackageDefinition {
			additional_paths: vec!["[".to_string()],
			..package_definition("core", "crates/core")
		}],
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid additional path glob"));
	assert!(
		error
			.to_string()
			.contains("[package.core].additional_paths contains invalid glob pattern")
	);

	let empty_value = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings::default(),
		&[monochange_core::PackageDefinition {
			additional_paths: vec![" ".to_string()],
			..package_definition("core", "crates/core")
		}],
	)
	.err()
	.unwrap_or_else(|| panic!("expected empty additional path error"));
	assert!(
		empty_value
			.to_string()
			.contains("[package.core].additional_paths must not include empty values")
	);
}

#[test]
fn validate_cli_rejects_invalid_command_shapes() {
	let duplicate = crate::validate_cli(&[
		cli_command(
			"release",
			vec![CliStepDefinition::Validate {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		),
		cli_command(
			"release",
			vec![CliStepDefinition::Validate {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		),
	])
	.err()
	.unwrap_or_else(|| panic!("expected duplicate command error"));
	assert!(
		duplicate
			.to_string()
			.contains("duplicate CLI command `release`")
	);

	let reserved = crate::validate_cli(&[cli_command(
		"help",
		vec![CliStepDefinition::Validate {
			name: None,
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected reserved name error"));
	assert!(reserved.to_string().contains("reserved built-in command"));

	let no_steps = crate::validate_cli(&[cli_command("release", Vec::new())])
		.err()
		.unwrap_or_else(|| panic!("expected missing steps error"));
	assert!(
		no_steps
			.to_string()
			.contains("must define at least one step")
	);
}

#[test]
fn validate_cli_rejects_invalid_inputs_and_step_metadata() {
	let mut duplicate_inputs = cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	);
	duplicate_inputs.inputs = vec![
		cli_input("token", CliInputKind::String),
		cli_input("token", CliInputKind::String),
	];
	let duplicate_input_error = crate::validate_cli(&[duplicate_inputs])
		.err()
		.unwrap_or_else(|| panic!("expected duplicate input error"));
	assert!(
		duplicate_input_error
			.to_string()
			.contains("defines duplicate input `token`")
	);

	let mut invalid_choice_default = cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	);
	let mut channel = cli_input("channel", CliInputKind::Choice);
	channel.choices = vec!["stable".to_string()];
	channel.default = Some("beta".to_string());
	invalid_choice_default.inputs = vec![channel];
	let choice_error = crate::validate_cli(&[invalid_choice_default])
		.err()
		.unwrap_or_else(|| panic!("expected invalid choice default"));
	assert!(
		choice_error
			.to_string()
			.contains("default `beta` is not one of the configured choices")
	);

	let mut invalid_boolean_default = cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	);
	let mut confirm = cli_input("confirm", CliInputKind::Boolean);
	confirm.default = Some("yes".to_string());
	invalid_boolean_default.inputs = vec![confirm];
	let boolean_error = crate::validate_cli(&[invalid_boolean_default])
		.err()
		.unwrap_or_else(|| panic!("expected invalid boolean default"));
	assert!(
		boolean_error
			.to_string()
			.contains("boolean default must be `true` or `false`")
	);

	let empty_step_name = crate::validate_cli(&[cli_command(
		"release",
		vec![CliStepDefinition::Validate {
			name: Some("   ".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected empty step name error"));
	assert!(empty_step_name.to_string().contains("has an empty `name`"));

	let duplicate_step_names = crate::validate_cli(&[cli_command(
		"release",
		vec![
			CliStepDefinition::Validate {
				name: Some("Shared".to_string()),
				when: None,
				inputs: std::collections::BTreeMap::new(),
			},
			CliStepDefinition::Discover {
				name: Some("Shared".to_string()),
				when: None,
				inputs: std::collections::BTreeMap::new(),
			},
		],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected duplicate step name error"));
	assert!(
		duplicate_step_names
			.to_string()
			.contains("duplicate step name `Shared`")
	);

	let invalid_command = crate::validate_cli(&[cli_command(
		"release",
		vec![CliStepDefinition::Command {
			name: Some("Run command".to_string()),
			when: Some(" ".to_string()),
			show_progress: None,
			command: String::new(),
			dry_run_command: Some(" ".to_string()),
			shell: ShellConfig::Default,
			id: Some(" ".to_string()),
			variables: None,
			inputs: std::collections::BTreeMap::from([(
				String::new(),
				CliStepInputValue::String("value".to_string()),
			)]),
		}],
	)])
	.err()
	.unwrap_or_else(|| panic!("expected invalid command step"));
	assert!(
		invalid_command
			.to_string()
			.contains("has an empty `when` condition")
	);

	let empty_input_name = crate::validate_cli(&[CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![cli_input(" ", CliInputKind::String)],
		steps: vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	}])
	.err()
	.unwrap_or_else(|| panic!("expected empty input name error"));
	assert!(
		empty_input_name
			.to_string()
			.contains("has an input with an empty name")
	);

	let reserved_input_name = crate::validate_cli(&[CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![cli_input("help", CliInputKind::String)],
		steps: vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	}])
	.err()
	.unwrap_or_else(|| panic!("expected reserved input name error"));
	assert!(
		reserved_input_name
			.to_string()
			.contains("collides with an implicit command flag")
	);

	let empty_choices = crate::validate_cli(&[CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: vec![cli_input("channel", CliInputKind::Choice)],
		steps: vec![CliStepDefinition::Validate {
			name: Some("Validate workspace".to_string()),
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	}])
	.err()
	.unwrap_or_else(|| panic!("expected empty choices error"));
	assert!(
		empty_choices
			.to_string()
			.contains("must define at least one choice")
	);
}

#[test]
fn validate_cli_runtime_requirements_enforce_source_features() {
	let publish_without_source = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		None,
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing source error"));
	assert!(
		publish_without_source
			.to_string()
			.contains("PublishRelease")
	);
	assert!(
		publish_without_source
			.to_string()
			.contains("not configured")
	);

	let mut source = sample_source_configuration(SourceProvider::GitHub);
	source.releases.enabled = false;
	let publish_disabled = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::PublishRelease {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&source),
	)
	.err()
	.unwrap_or_else(|| panic!("expected disabled release error"));
	assert!(publish_disabled.to_string().contains("PublishRelease"));
	assert!(publish_disabled.to_string().contains("enabled` is false"));

	let mut pull_requests_disabled = sample_source_configuration(SourceProvider::GitHub);
	pull_requests_disabled.pull_requests.enabled = false;
	let open_request_error = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::OpenReleaseRequest {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&pull_requests_disabled),
	)
	.err()
	.unwrap_or_else(|| panic!("expected disabled pull request error"));
	assert!(
		open_request_error
			.to_string()
			.contains("OpenReleaseRequest")
	);
	assert!(open_request_error.to_string().contains("enabled` is false"));

	let comment_provider_error = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"release",
			vec![CliStepDefinition::CommentReleasedIssues {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings::default(),
		Some(&sample_source_configuration(SourceProvider::GitLab)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported provider error"));
	assert!(
		comment_provider_error
			.to_string()
			.contains("does not support released-issue comments")
	);
}

#[test]
fn validate_cli_runtime_requirements_enforce_affected_package_inputs() {
	let verify_disabled = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"affected",
			vec![CliStepDefinition::AffectedPackages {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings {
			verify: monochange_core::ChangesetVerificationSettings {
				enabled: false,
				..Default::default()
			},
		},
		Some(&sample_source_configuration(SourceProvider::GitHub)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected verify disabled error"));
	assert!(verify_disabled.to_string().contains("AffectedPackages"));
	assert!(verify_disabled.to_string().contains("enabled` is false"));

	let missing_selector = crate::validate_cli_runtime_requirements(
		&[cli_command(
			"affected",
			vec![CliStepDefinition::AffectedPackages {
				name: None,
				when: None,
				inputs: std::collections::BTreeMap::new(),
			}],
		)],
		&monochange_core::ChangesetSettings {
			verify: monochange_core::ChangesetVerificationSettings {
				enabled: true,
				..Default::default()
			},
		},
		Some(&sample_source_configuration(SourceProvider::GitHub)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected selector error"));
	assert!(
		missing_selector
			.to_string()
			.contains("declares neither a `changed_paths` nor a `since` input")
	);

	let mut command = cli_command(
		"affected",
		vec![CliStepDefinition::AffectedPackages {
			name: None,
			when: None,
			inputs: std::collections::BTreeMap::new(),
		}],
	);
	command.inputs = vec![
		cli_input("changed_paths", CliInputKind::StringList),
		cli_input("label", CliInputKind::String),
	];
	let invalid_label = crate::validate_cli_runtime_requirements(
		&[command],
		&monochange_core::ChangesetSettings {
			verify: monochange_core::ChangesetVerificationSettings {
				enabled: true,
				..Default::default()
			},
		},
		Some(&sample_source_configuration(SourceProvider::GitHub)),
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid label kind error"));
	assert!(
		invalid_label
			.to_string()
			.contains("input `label` must use type `string_list`")
	);
}

#[test]
fn validate_package_and_source_settings_cover_duplicate_and_pattern_errors() {
	let root = fixture_path("config/validation-helper-branches");

	let duplicate_path_error = crate::validate_package_and_group_definitions(
		&root,
		"[package.core]\npath = 'crates/core'\n\n[package.util]\npath = 'crates/core'\n",
		&[
			package_definition("core", "crates/core"),
			package_definition("util", "crates/core"),
		],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate path error"));
	assert!(
		duplicate_path_error
			.to_string()
			.contains("package path `crates/core` is already used by `core`")
	);

	let mut primary_core = package_definition("core", "crates/core");
	primary_core.version_format = monochange_core::VersionFormat::Primary;
	let mut primary_util = package_definition("util", "crates/util");
	primary_util.version_format = monochange_core::VersionFormat::Primary;
	let duplicate_primary_error = crate::validate_package_and_group_definitions(
		&root,
		"[package.core]\nversion_format = 'primary'\n\n[package.util]\nversion_format = 'primary'\n",
		&[primary_core, primary_util],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate primary error"));
	assert!(
		duplicate_primary_error
			.to_string()
			.contains("`version_format = \"primary\"` is already used by `core`")
	);

	let source_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			host: None,
			api_url: None,
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings {
				changesets: monochange_core::ProviderChangesetBotSettings {
					enabled: true,
					required: true,
					skip_labels: vec!["skip".to_string()],
					comment_on_failure: true,
					changed_paths: vec!["[".to_string()],
					ignored_paths: Vec::new(),
				},
			},
		}))
		.err()
		.unwrap_or_else(|| panic!("expected invalid source glob error"));
	assert!(
		source_error
			.to_string()
			.contains("[source.bot.changesets].changed_paths contains invalid glob pattern")
	);

	let package_pattern_error = crate::validate_changesets_configuration(
		&monochange_core::ChangesetSettings::default(),
		&[monochange_core::PackageDefinition {
			ignored_paths: vec![String::new()],
			..package_definition("core", "crates/core")
		}],
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid package path pattern error"));
	assert!(
		package_pattern_error
			.to_string()
			.contains("[package.core].ignored_paths must not include empty values")
	);

	let duplicate_id_error = crate::validate_package_and_group_definitions(
		&root,
		"[package.core]\npath = 'crates/core'\n",
		&[
			package_definition("core", "crates/core"),
			package_definition("core", "crates/util"),
		],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected duplicate id error"));
	assert!(
		duplicate_id_error
			.to_string()
			.contains("duplicate package id `core`")
	);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir core dir: {error}"));
	let missing_manifest_error = crate::validate_package_and_group_definitions(
		tempdir.path(),
		"[package.core]\npath = 'crates/core'\ntype = 'cargo'\n",
		&[package_definition("core", "crates/core")],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing manifest error"));
	assert!(
		missing_manifest_error
			.to_string()
			.contains("missing expected cargo manifest")
	);
}

#[test]
fn parse_markdown_change_target_and_validation_helpers_cover_remaining_error_paths() {
	let root = fixture_path("changeset-target-metadata/render-workspace");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));

	let invalid_scalar = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("docs")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let scalar_error = crate::parse_markdown_change_target(
		&invalid_scalar,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid scalar error"));
	assert!(
		scalar_error
			.to_string()
			.contains("invalid scalar value `docs`")
	);
	assert!(scalar_error.to_string().contains("configured types"));

	let unknown_keys = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("extra: true")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let unknown_keys_error = crate::parse_markdown_change_target(
		&unknown_keys,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported field error"));
	assert!(
		unknown_keys_error
			.to_string()
			.contains("uses unsupported field(s): extra")
	);

	let invalid_bump = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("bump: nope")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let invalid_bump_error = crate::parse_markdown_change_target(
		&invalid_bump,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid bump error"));
	assert!(
		invalid_bump_error
			.to_string()
			.contains("has invalid bump `nope`")
	);

	let invalid_version = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("version: nope")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let invalid_version_error = crate::parse_markdown_change_target(
		&invalid_version,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid version error"));
	assert!(
		invalid_version_error
			.to_string()
			.contains("has invalid version `nope`")
	);

	let none_only = serde_yaml_ng::from_str::<serde_yaml_ng::Value>("bump: none")
		.unwrap_or_else(|error| panic!("yaml parse: {error}"));
	let none_only_error = crate::parse_markdown_change_target(
		&none_only,
		Path::new("change.md"),
		"core",
		&configuration,
	)
	.err()
	.unwrap_or_else(|| panic!("expected none-only error"));
	assert!(
		none_only_error
			.to_string()
			.contains("must not use `bump = \"none\"`")
	);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::create_dir_all(tempdir.path().join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir core: {error}"));
	std::fs::write(
		tempdir.path().join("monochange.toml"),
		"[package.core]\npath = \"crates/core\"\ntype = \"cargo\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));
	std::fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));
	let no_types_configuration = load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("workspace configuration: {error}"));
	let no_types_error = crate::validate_configured_change_type(
		&no_types_configuration,
		Path::new("change.md"),
		"core",
		"docs",
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid type error"));
	assert!(
		no_types_error
			.to_string()
			.contains("no configured types are available")
	);
}

#[test]
fn validate_versioned_files_and_release_notes_cover_remaining_validation_paths() {
	let root = fixture_path("config/validation-helper-branches");
	let config_contents = "[package.core]\npath = 'crates/core'\n";
	let declared_packages = std::collections::BTreeSet::from(["core"]);

	let missing_type = crate::validate_versioned_files(
		&root,
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "README.md".to_string(),
			ecosystem_type: None,
			name: None,
			fields: None,
			prefix: None,
			regex: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing type error"));
	assert!(
		missing_type
			.to_string()
			.contains("versioned_files must set `type`")
	);

	let invalid_glob = crate::validate_versioned_files(
		&root,
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "[".to_string(),
			ecosystem_type: Some(EcosystemType::Cargo),
			name: None,
			fields: None,
			prefix: None,
			regex: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected invalid glob error"));
	assert!(
		invalid_glob
			.to_string()
			.contains("invalid glob pattern `[`")
	);

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	std::fs::create_dir_all(tempdir.path().join("packages/web"))
		.unwrap_or_else(|error| panic!("mkdir web package: {error}"));
	std::fs::write(
		tempdir.path().join("packages/web/package.json"),
		"{\"name\":\"web\",\"version\":\"1.0.0\"}\n",
	)
	.unwrap_or_else(|error| panic!("write package.json: {error}"));
	let unsupported_match = crate::validate_versioned_files(
		tempdir.path(),
		config_contents,
		&[monochange_core::VersionedFileDefinition {
			path: "packages/*/package.json".to_string(),
			ecosystem_type: Some(EcosystemType::Cargo),
			name: None,
			fields: None,
			prefix: None,
			regex: None,
		}],
		&declared_packages,
		"package",
		"core",
	)
	.err()
	.unwrap_or_else(|| panic!("expected unsupported match error"));
	assert!(
		unsupported_match
			.to_string()
			.contains("matched unsupported file")
	);
	assert!(
		unsupported_match
			.to_string()
			.contains("for ecosystem `cargo`")
	);

	let empty_template = crate::validate_release_notes_configuration(
		"",
		&crate::RawReleaseNotesSettings {
			change_templates: vec![" ".to_string()],
		},
		&[],
		&[],
		&[],
	)
	.err()
	.unwrap_or_else(|| panic!("expected empty template error"));
	assert!(
		empty_template
			.to_string()
			.contains("must not include empty templates")
	);

	assert_eq!(
		crate::change_template_variables("{{ summary }} {{ details | default('') }} {{"),
		vec!["details".to_string(), "summary".to_string()]
	);
}

#[test]
fn validate_github_source_and_api_configuration_cover_remaining_paths() {
	let github_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings {
				labels: vec![" ".to_string()],
				..Default::default()
			},
			bot: monochange_core::ProviderBotSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected invalid github labels error"));
	assert!(
		github_error
			.to_string()
			.contains("[source.pull_requests].labels must not include empty values")
	);

	let github_skip_label_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: None,
			api_url: None,
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings {
				changesets: monochange_core::ProviderChangesetBotSettings {
					skip_labels: vec![" ".to_string()],
					..Default::default()
				},
			},
		}))
		.err()
		.unwrap_or_else(|| panic!("expected invalid github skip label error"));
	assert!(
		github_skip_label_error
			.to_string()
			.contains("[source.bot.changesets].skip_labels must not include empty values")
	);

	let source_api_error =
		crate::validate_source_configuration(Some(&monochange_core::SourceConfiguration {
			provider: SourceProvider::GitHub,
			host: Some("https://example.invalid".to_string()),
			api_url: Some("http://api.example.invalid".to_string()),
			owner: "ifiokjr".to_string(),
			repo: "monochange".to_string(),
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
			bot: monochange_core::ProviderBotSettings::default(),
		}))
		.err()
		.unwrap_or_else(|| panic!("expected insecure api_url error"));
	assert!(source_api_error.to_string().contains("insecure scheme"));

	crate::validate_api_url_host("https://github.example.com/api/v3", SourceProvider::GitHub)
		.unwrap_or_else(|error| panic!("custom GitHub host should warn but succeed: {error}"));
}

#[test]
fn matching_package_helpers_cover_references_and_definitions() {
	let root = PathBuf::from("/workspace");
	let core = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let web = PackageRecord::new(
		Ecosystem::Npm,
		"web",
		root.join("packages/web/package.json"),
		root.clone(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	let packages = vec![core.clone(), web.clone()];
	assert_eq!(
		crate::find_matching_package_indices(&packages, &root, "core"),
		vec![0]
	);
	assert_eq!(
		crate::find_matching_package_indices(&packages, &root, "packages/web"),
		vec![1]
	);
	let definition = monochange_core::PackageDefinition {
		id: "web".to_string(),
		path: PathBuf::from("packages/web"),
		package_type: monochange_core::PackageType::Npm,
		changelog: None,
		extra_changelog_sections: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		version_format: monochange_core::VersionFormat::Primary,
		publish: monochange_core::PublishSettings::default(),
	};
	assert_eq!(
		crate::find_matching_package_indices_for_definition(&packages, &root, &definition),
		vec![1]
	);
	assert!(crate::package_matches_definition(&web, &root, &definition));
	assert!(!crate::package_matches_definition(
		&core,
		&root,
		&definition
	));
	assert!(crate::ecosystem_matches_package_type(
		Ecosystem::Flutter,
		monochange_core::PackageType::Flutter
	));
}

#[test]
fn load_workspace_configuration_rejects_versioned_file_without_type_or_regex() {
	let root = fixture_path("config/rejects-versioned-file-without-type-or-regex");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("versioned_files must set `type`"));
	assert!(rendered.contains("versioned_files entry is missing `type`"));
}

// -- validate_versioned_files_content tests --

#[test]
fn validate_versioned_files_content_rejects_missing_file() {
	let root = fixture_path("config/versioned-file-missing");
	let error = crate::validate_versioned_files_content(&root)
		.err()
		.unwrap_or_else(|| panic!("expected error for missing versioned file"));
	assert!(error.to_string().contains("does-not-exist.toml"));
	assert!(error.to_string().contains("does not exist"));
}

#[test]
fn validate_versioned_files_content_rejects_regex_without_match() {
	let root = fixture_path("config/versioned-file-regex-no-match");
	let error = crate::validate_versioned_files_content(&root)
		.err()
		.unwrap_or_else(|| panic!("expected error for regex without match"));
	assert!(error.to_string().contains("does not match any content"));
}

#[test]
fn validate_versioned_files_content_rejects_unparseable_version() {
	let root = fixture_path("config/versioned-file-unparseable-version");
	let error = crate::validate_versioned_files_content(&root)
		.err()
		.unwrap_or_else(|| panic!("expected error for missing version field"));
	assert!(
		error
			.to_string()
			.contains("does not contain a readable version field")
	);
}

#[test]
fn validate_versioned_files_content_warns_on_empty_glob() {
	let root = fixture_path("config/versioned-file-empty-glob");
	let warnings = crate::validate_versioned_files_content(&root)
		.unwrap_or_else(|error| panic!("expected Ok with warnings, got error: {error}"));
	assert_eq!(warnings.len(), 1);
	assert!(warnings.first().unwrap().contains("matches no files"));
}

#[test]
fn validate_api_url_host_rejects_insecure_http_scheme() {
	let error = crate::validate_api_url_host("http://attacker.com/api/v3", SourceProvider::GitHub)
		.err()
		.unwrap_or_else(|| panic!("expected error for http://"));
	assert!(error.to_string().contains("insecure scheme"));
}

#[test]
fn validate_api_url_host_accepts_https_scheme() {
	crate::validate_api_url_host("https://api.github.com", SourceProvider::GitHub)
		.unwrap_or_else(|error| panic!("expected Ok for https://: {error}"));
}

#[test]
fn changeset_files_with_crlf_line_endings_parse_correctly() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Write a minimal monochange.toml.
	std::fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n\n[package.core]\npath = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write toml: {error}"));

	// Create the package manifest.
	std::fs::create_dir_all(root.join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir: {error}"));
	std::fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo: {error}"));

	// Write a changeset file with CRLF line endings.
	let crlf_changeset = "---\r\ncore: patch\r\n---\r\n\r\nFix a bug with CRLF endings.\r\n";
	std::fs::create_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("mkdir changeset: {error}"));
	std::fs::write(root.join(".changeset/crlf-test.md"), crlf_changeset)
		.unwrap_or_else(|error| panic!("write changeset: {error}"));

	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("config: {error}"));

	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let changeset = load_changeset_file(
		&root.join(".changeset/crlf-test.md"),
		&configuration,
		&packages,
	)
	.unwrap_or_else(|error| panic!("expected CRLF changeset to parse, got: {error}"));

	assert!(!changeset.targets.is_empty());
	let summary = changeset
		.summary
		.unwrap_or_else(|| panic!("expected summary"));
	assert!(summary.contains("CRLF endings"));
}

#[test]
fn changeset_files_with_bare_cr_line_endings_parse_correctly() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	std::fs::write(
		root.join("monochange.toml"),
		"[defaults]\npackage_type = \"cargo\"\n\n[package.core]\npath = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write toml: {error}"));
	std::fs::create_dir_all(root.join("crates/core"))
		.unwrap_or_else(|error| panic!("mkdir: {error}"));
	std::fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	)
	.unwrap_or_else(|error| panic!("write cargo: {error}"));

	// Bare carriage return (old Mac style) line endings.
	let bare_cr = "---\rcore: patch\r---\r\rFix with bare CR.\r";
	std::fs::create_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("mkdir: {error}"));
	std::fs::write(root.join(".changeset/bare-cr.md"), bare_cr)
		.unwrap_or_else(|error| panic!("write: {error}"));

	let configuration =
		load_workspace_configuration(root).unwrap_or_else(|error| panic!("config: {error}"));
	let packages = vec![PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		root.join("crates/core/Cargo.toml"),
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	)];

	let changeset = load_changeset_file(
		&root.join(".changeset/bare-cr.md"),
		&configuration,
		&packages,
	)
	.unwrap_or_else(|error| panic!("expected bare CR changeset to parse, got: {error}"));

	assert!(!changeset.targets.is_empty());
	let summary = changeset
		.summary
		.unwrap_or_else(|| panic!("expected summary"));
	assert!(summary.contains("bare CR"));
}
