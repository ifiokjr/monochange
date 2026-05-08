use std::fs;

use serde_json::Value;

mod test_support;
use test_support::monochange_command;
use test_support::setup_scenario_workspace;

#[test]
fn local_jsonl_telemetry_records_cli_command_and_step_events() {
	let tempdir = setup_scenario_workspace("monochange/validate-workspace");
	let telemetry_file = tempdir.path().join("telemetry.jsonl");

	let output = monochange_command(None)
		.current_dir(tempdir.path())
		.env("MC_TELEMETRY_FILE", &telemetry_file)
		.env_remove("MC_TELEMETRY")
		.arg("validate")
		.arg("--quiet")
		.output()
		.unwrap_or_else(|error| panic!("run validate with telemetry: {error}"));

	assert!(
		output.status.success(),
		"expected validate to pass\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr),
	);

	let events = read_jsonl_events(&telemetry_file);
	assert_eq!(events.len(), 2);

	let step = &events[0];
	assert_eq!(step["body"]["string_value"], "command_step");
	assert_eq!(step["resource"]["service.name"], "monochange");
	assert_eq!(step["scope"]["name"], "monochange.telemetry");
	assert_eq!(step["attributes"]["command_name"], "validate");
	assert_eq!(step["attributes"]["step_kind"], "Validate");
	assert_eq!(step["attributes"]["outcome"], "success");
	assert!(step["attributes"]["error_kind"].is_null());

	let command = &events[1];
	assert_eq!(command["body"]["string_value"], "command_run");
	assert_eq!(command["attributes"]["command_name"], "validate");
	assert_eq!(command["attributes"]["command_source"], "configured");
	assert_eq!(command["attributes"]["progress_format"], "auto");
	assert_eq!(command["attributes"]["step_count"], 1);
	assert_eq!(command["attributes"]["outcome"], "success");
	assert!(command["attributes"]["error_kind"].is_null());
}

fn read_jsonl_events(path: &std::path::Path) -> Vec<Value> {
	fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("read telemetry file {}: {error}", path.display()))
		.lines()
		.map(|line| {
			serde_json::from_str(line)
				.unwrap_or_else(|error| panic!("parse telemetry json line {line:?}: {error}"))
		})
		.collect()
}
