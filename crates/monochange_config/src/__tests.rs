use std::path::{Path, PathBuf};

use monochange_core::BumpSeverity;
use monochange_core::ChangelogDefinition;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogTarget;
use monochange_core::CliStepDefinition;
use monochange_core::Ecosystem;
use monochange_core::EcosystemType;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::ShellConfig;
use semver::Version;
use tempfile::tempdir;

use crate::apply_version_groups;
use crate::load_change_signals;
use crate::load_changeset_file;
use crate::load_workspace_configuration;
use crate::resolve_package_reference;
use crate::validate_workspace;

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
	assert_eq!(configuration.cli.len(), 6);
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
			"affected",
			"diagnostics"
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
	assert_eq!(configuration.cli.len(), 1);
	assert_eq!(
		configuration
			.cli
			.first()
			.unwrap_or_else(|| panic!("expected CLI command"))
			.steps
			.len(),
		2
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
	assert_eq!(source.provider, monochange_core::SourceProvider::GitHub);
	assert_eq!(source.owner, "ifiokjr");
	assert_eq!(source.repo, "monochange");
	assert!(source.releases.enabled);
	assert!(source.releases.draft);
	assert!(source.releases.prerelease);
	assert!(source.releases.generate_notes);
	assert_eq!(
		source.releases.source,
		monochange_core::GitHubReleaseNotesSource::GitHubGenerated
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
	assert_eq!(source.provider, monochange_core::SourceProvider::GitHub);
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
fn load_workspace_configuration_rejects_empty_github_owner_and_repo() {
	let root_owner = fixture_path("config/rejects-empty-github-owner");
	assert!(load_workspace_configuration(&root_owner)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github].owner must not be empty"));

	let root_repo = fixture_path("config/rejects-empty-github-repo");
	assert!(load_workspace_configuration(&root_repo)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github].repo must not be empty"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_pull_request_settings() {
	let root = fixture_path("config/rejects-invalid-pr-settings");
	assert!(load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github.pull_requests].branch_prefix must not be empty"));
}

#[test]
fn load_workspace_configuration_rejects_empty_pull_request_base_and_title() {
	let root_base = fixture_path("config/rejects-empty-pr-base");
	assert!(load_workspace_configuration(&root_base)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github.pull_requests].base must not be empty"));

	let root_title = fixture_path("config/rejects-empty-pr-title");
	assert!(load_workspace_configuration(&root_title)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("[github.pull_requests].title must not be empty"));
}

#[test]
fn load_workspace_configuration_rejects_invalid_github_release_note_source_combinations() {
	let root = fixture_path("config/rejects-invalid-release-source");
	assert!(load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("generate_notes cannot be true"));
}

#[test]
fn load_workspace_configuration_rejects_empty_pull_request_labels() {
	let root = fixture_path("config/rejects-empty-pr-labels");
	assert!(load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"))
		.to_string()
		.contains("labels must not include empty values"));
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
fn migration_guide_new_style_example_loads_successfully() {
	let root = fixture_path("config/migration-guide-example");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.packages.len(), 2);
	assert_eq!(configuration.groups.len(), 1);
	let group = configuration
		.groups
		.first()
		.unwrap_or_else(|| panic!("expected migration group"));
	assert_eq!(group.id, "main");
	assert_eq!(group.packages, vec!["monochange", "monochange_core"]);
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
	assert!(rendered.contains("labels:"));
	assert!(rendered.contains("move the package into exactly one [group.<id>] declaration"));
}

#[test]
fn load_workspace_configuration_rejects_duplicate_primary_version_format() {
	let root = fixture_path("config/rejects-duplicate-primary-version");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));
	let rendered = error.render();

	assert!(rendered.contains("primary release identity"));
	assert!(rendered
		.contains("choose a single package or group as the primary outward release identity"));
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
	assert!(package
		.versioned_files
		.iter()
		.all(|definition| definition.ecosystem_type == EcosystemType::Cargo));
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
	assert!(changeset
		.signals
		.iter()
		.all(|signal| signal.requested_bump == Some(BumpSeverity::Minor)));
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
	assert!(error
		.to_string()
		.contains("no configured types are available for this target"));
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
	assert!(error
		.to_string()
		.contains("must not use `bump = \"none\"` without also declaring `type` or `version`"));
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
	assert!(error
		.to_string()
		.contains("has invalid bump `nope`; expected `none`, `patch`, `minor`, or `major`"));
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
	assert!(error
		.to_string()
		.contains("must map to `none`, `patch`, `minor`, `major`, a configured change type"));
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
	assert!(error
		.to_string()
		.contains("must declare `bump`, `version`, `type`, or a valid scalar shorthand"));
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
	assert!(error
		.to_string()
		.contains("unterminated markdown frontmatter"));
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
	assert!(signals
		.iter()
		.all(|signal| signal.requested_bump == Some(BumpSeverity::Minor)));
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
	assert!(changeset
		.signals
		.iter()
		.all(|signal| signal.source_path == root.join("change.md")));
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
fn validate_workspace_rejects_changesets_that_mix_group_and_member_references() {
	let root = fixture_path("config/rejects-mixed-changeset");
	let error = validate_workspace(&root)
		.err()
		.unwrap_or_else(|| panic!("expected changeset validation error"));
	let rendered = error.render();

	assert!(rendered.contains("references both group `sdk` and member package `core`"));
	assert!(rendered.contains("reference either the group or one of its member packages"));
}

#[test]
fn load_workspace_configuration_rejects_publish_github_release_without_github_config() {
	let root = fixture_path("config/rejects-publish-no-github");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected github CLI command config error"));
	assert!(error
		.to_string()
		.contains("uses `PublishRelease` but `[source]` is not configured"));
}

#[test]
fn load_workspace_configuration_rejects_open_release_pull_request_without_github_config() {
	let root = fixture_path("config/rejects-pr-no-github");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected github CLI command config error"));
	assert!(error
		.to_string()
		.contains("uses `OpenReleaseRequest` but `[source]` is not configured"));
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
	assert_eq!(configuration.cli.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_enforce_changeset_policy_without_github_bot_config() {
	let root = fixture_path("config/rejects-enforce-no-bot");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected verification CLI command config error"));
	assert!(error
		.to_string()
		.contains("uses `AffectedPackages` but `[changesets.verify].enabled` is false"));
}

#[test]
fn load_workspace_configuration_rejects_affected_packages_without_path_inputs() {
	let root = fixture_path("config/rejects-affected-no-path");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected verification CLI command config error"));
	assert!(error
		.to_string()
		.contains("declares neither a `changed_paths` nor a `since` input"));
}

#[test]
fn load_workspace_configuration_accepts_affected_packages_step_input_overrides() {
	let root = fixture_path("config/affected-step-overrides");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.cli.len(), 1);
}

#[test]
fn load_workspace_configuration_accepts_affected_packages_with_since_in_step_override() {
	let root = fixture_path("config/affected-step-since");
	let configuration = load_workspace_configuration(&root)
		.unwrap_or_else(|error| panic!("configuration: {error}"));
	assert_eq!(configuration.cli.len(), 1);
}

#[test]
fn load_workspace_configuration_rejects_affected_packages_when_step_override_provides_no_path_source(
) {
	let root = fixture_path("config/rejects-affected-no-source");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error
		.to_string()
		.contains("declares neither a `changed_paths` nor a `since` input"));
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
	assert!(error
		.render()
		.contains("unsupported variables: commit_hash"));
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
fn load_workspace_configuration_rejects_legacy_workflows_namespace() {
	let root = fixture_path("config/rejects-legacy-workflows");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected configuration error"));

	assert!(error
		.to_string()
		.contains("legacy `[[workflows]]` configuration is no longer supported"));
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
fn load_workspace_configuration_rejects_both_source_and_legacy_github_config() {
	let root = fixture_path("config/rejects-source-and-github");
	let error = load_workspace_configuration(&root)
		.err()
		.unwrap_or_else(|| panic!("expected config error"));
	assert!(error
		.to_string()
		.contains("configure either `[source]` or legacy `[github]`"));
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

fn fixture_path(relative: &str) -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
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
