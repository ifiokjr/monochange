use insta::assert_json_snapshot;
use serde_json::Value;

mod test_support;
use test_support::{run_json_command, setup_scenario_workspace, snapshot_settings};

#[test]
fn grouped_member_changes_patch_dependents_outside_the_group() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix("grouped_member_changes_patch_dependents_outside_the_group");
	let _guard = settings.bind_to_scope();

	let tempdir = setup_scenario_workspace("integration/grouped-dependent-propagation");
	let json: Value = run_json_command(tempdir.path(), "release", Some("2026-04-06"));
	assert_json_snapshot!(json);

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
