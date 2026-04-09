use std::fs;

use insta::assert_snapshot;

mod test_support;
use test_support::{monochange_command, setup_scenario_workspace, snapshot_settings};

#[test]
fn release_preserves_cargo_manifest_formatting_while_updating_versions() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix("preserve_cargo");
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("manifest-formatting/preserve-cargo");
	let output = monochange_command(Some("2026-04-09"))
		.current_dir(tempdir.path())
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let root_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("root manifest: {error}"));
	let core_manifest = fs::read_to_string(tempdir.path().join("crates/core/Cargo.toml"))
		.unwrap_or_else(|error| panic!("core manifest: {error}"));
	let app_manifest = fs::read_to_string(tempdir.path().join("crates/app/Cargo.toml"))
		.unwrap_or_else(|error| panic!("app manifest: {error}"));
	let group_file = fs::read_to_string(tempdir.path().join("group.toml"))
		.unwrap_or_else(|error| panic!("group file: {error}"));
	let extra_file = fs::read_to_string(tempdir.path().join("crates/core/extra.toml"))
		.unwrap_or_else(|error| panic!("extra file: {error}"));

	assert_snapshot!("root_manifest", root_manifest);
	assert_snapshot!("core_manifest", core_manifest);
	assert_snapshot!("app_manifest", app_manifest);
	assert_snapshot!("group_file", group_file);
	assert_snapshot!("extra_file", extra_file);
}
