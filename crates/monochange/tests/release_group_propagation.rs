use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path};

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env("MONOCHANGE_RELEASE_DATE", "2026-04-06");
	command
}

#[test]
fn grouped_member_changes_patch_dependents_outside_the_group() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_root = fixture_path("integration/grouped-dependent-propagation");
	copy_directory(&fixture_root, tempdir.path());

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));
	let decisions = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"));

	let web_sdk = decisions
		.iter()
		.find(|decision| {
			decision["package"]
				.as_str()
				.is_some_and(|package| package.contains("packages/web-sdk/package.json"))
		})
		.unwrap_or_else(|| panic!("expected web-sdk decision"));
	assert_eq!(web_sdk["bump"], "minor");
	assert_eq!(web_sdk["trigger"], "version-group-synchronization");

	for (label, needle) in [
		("deno-tool", "deno/tool/deno.json"),
		("mobile-sdk", "dart/mobile_sdk/pubspec.yaml"),
	] {
		let decision = decisions
			.iter()
			.find(|candidate| {
				candidate["package"]
					.as_str()
					.is_some_and(|package| package.contains(needle))
			})
			.unwrap_or_else(|| panic!("expected {label} decision"));
		assert_eq!(decision["bump"], "patch", "unexpected bump for {label}");
		assert_eq!(
			decision["trigger"], "transitive-dependency",
			"unexpected trigger for {label}"
		);
	}
}
