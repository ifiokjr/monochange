use std::fs;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn release_uses_keep_a_changelog_format_from_defaults() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/defaults-keep-a");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(core_changelog.contains("## [1.1.0]"));
	assert!(core_changelog.contains("### Features"));
	assert!(core_changelog.contains("- add keep a changelog support"));
	assert!(!core_changelog.contains("- **core**: add keep a changelog support"));
	assert!(app_changelog.contains("## [1.1.0]"));
	assert!(app_changelog.contains("### Features"));
	assert!(group_changelog.contains("## [1.1.0]"));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("Changed members: core"));
	assert!(group_changelog.contains("Synchronized members: app"));
	assert!(group_changelog.contains("### Features"));
	assert!(group_changelog.contains("- **core**: add keep a changelog support"));
}

#[test]
fn release_allows_package_and_group_changelog_format_overrides() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/defaults-then-package-override");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let app_changelog = fs::read_to_string(tempdir.path().join("crates/app/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("app changelog: {error}"));
	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));

	assert!(core_changelog.contains("## [1.1.0]"));
	assert!(core_changelog.contains("### Features"));
	assert!(!core_changelog.contains("- **core**: add keep a changelog support"));
	assert!(app_changelog.contains("## 1.1.0"));
	assert!(!app_changelog.contains("## [1.1.0]"));
	assert!(app_changelog.contains("### Features"));
	assert!(app_changelog.contains(
		"No package-specific changes were recorded; `workflow-app` was updated to 1.1.0 as part of group `sdk`."
	));
	assert!(group_changelog.contains("## 1.1.0"));
	assert!(!group_changelog.contains("## [1.1.0]"));
	assert!(group_changelog.contains("Grouped release for `sdk`."));
	assert!(group_changelog.contains("Changed members: core"));
	assert!(group_changelog.contains("Synchronized members: app"));
	assert!(group_changelog.contains("### Features"));
	assert!(group_changelog.contains("- **core**: add keep a changelog support"));
}

#[test]
fn release_uses_alert_syntax_for_group_entries_with_multiline_content() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/alert-multiline");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	assert!(group_changelog.contains("> [!NOTE]"));
	assert!(group_changelog.contains("> *core*"));
	assert!(group_changelog.contains("#### explain grouped changelog formatting"));
	assert!(group_changelog.contains("This release note needs more than one line."));
	assert!(!group_changelog.contains("- **core**: explain grouped changelog formatting"));
}

#[test]
fn release_uses_alert_syntax_for_group_entries_with_multiple_packages_in_one_changeset() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("changelog-formats/alert-multi-packages");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let group_changelog = fs::read_to_string(tempdir.path().join("docs/sdk-CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	assert!(group_changelog.contains("Grouped release for `sdk`"));
	assert!(!group_changelog.contains("Synchronized members:"));
	assert!(group_changelog.contains("> [!NOTE]"));
	assert!(group_changelog.contains("> *core*, *app*"));
	assert!(group_changelog.contains("add shared release note"));
	assert!(!group_changelog.contains("- **core**: add shared release note"));
}
