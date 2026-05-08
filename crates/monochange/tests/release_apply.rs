use std::fs;

use serde_json::Value;

mod test_support;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;

#[test]
fn release_apply_with_github_source_updates_workspace_and_deletes_changesets() {
	let tempdir = setup_scenario_workspace("monochange/release-apply-github");
	let root = tempdir.path();

	let output = monochange_command(Some("2026-04-06"))
		.current_dir(root)
		.arg("release")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release command: {error}"));
	assert!(
		output.status.success(),
		"release failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let parsed: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("release json: {error}"));
	assert_eq!(parsed["version"].as_str(), Some("1.1.0"));
	assert_eq!(parsed["dryRun"].as_bool(), Some(false));
	assert!(
		!root.join(".changeset/feature.md").exists(),
		"expected release apply to delete processed changeset"
	);
	assert!(
		fs::read_to_string(root.join("Cargo.toml"))
			.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"))
			.contains("version = \"1.1.0\""),
		"expected release apply to update the workspace manifest"
	);
	assert!(
		fs::read_to_string(root.join("crates/core/extra.toml"))
			.unwrap_or_else(|error| panic!("read crates/core/extra.toml: {error}"))
			.contains("version = \"1.1.0\""),
		"expected release apply to update configured package versioned files"
	);
	assert!(
		fs::read_to_string(root.join("group.toml"))
			.unwrap_or_else(|error| panic!("read group.toml: {error}"))
			.contains("version = \"1.1.0\""),
		"expected release apply to update configured versioned files"
	);
}
