use std::time::Duration;

use monochange_core::MonochangeError;
use monochange_telemetry::CommandTelemetry;
use monochange_telemetry::StepTelemetry;
use monochange_telemetry::TelemetryOutcome;
use monochange_telemetry::TelemetrySink;
use serde_json::Value;

#[test]
fn public_api_writes_sanitized_command_and_step_jsonl_events() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let telemetry_file = tempdir.path().join("telemetry.jsonl");
	let telemetry_file_value = telemetry_file.to_string_lossy();
	let config_error = MonochangeError::Config("do not include raw path /private/repo".to_string());

	temp_env::with_vars(
		[
			("MC_TELEMETRY", None::<&str>),
			("MC_TELEMETRY_FILE", Some(telemetry_file_value.as_ref())),
		],
		|| {
			let sink = TelemetrySink::from_env();
			sink.capture_step(StepTelemetry {
				command_name: "validate",
				step_index: 0,
				step_kind: "Validate",
				skipped: false,
				duration: Duration::from_millis(3),
				outcome: TelemetryOutcome::Error,
				error: Some(&config_error),
			});
			sink.capture_command(CommandTelemetry {
				command_name: "validate",
				dry_run: true,
				show_diff: false,
				progress_format: "auto",
				step_count: 1,
				duration: Duration::from_millis(5),
				outcome: TelemetryOutcome::Error,
				error: Some(&config_error),
			});
		},
	);

	let events = read_jsonl_events(&telemetry_file);
	assert_eq!(events.len(), 2);
	let step_event = events
		.first()
		.unwrap_or_else(|| panic!("missing step telemetry event"));
	let command_event = events
		.get(1)
		.unwrap_or_else(|| panic!("missing command telemetry event"));
	assert_eq!(
		step_event
			.pointer("/body/string_value")
			.and_then(Value::as_str),
		Some("command_step")
	);
	assert_eq!(
		step_event
			.pointer("/attributes/error_kind")
			.and_then(Value::as_str),
		Some("config_error")
	);
	assert_eq!(
		command_event
			.pointer("/body/string_value")
			.and_then(Value::as_str),
		Some("command_run")
	);
	assert_eq!(
		command_event
			.pointer("/attributes/dry_run")
			.and_then(Value::as_bool),
		Some(true)
	);
	assert_eq!(
		command_event
			.pointer("/attributes/progress_format")
			.and_then(Value::as_str),
		Some("auto")
	);

	let rendered =
		serde_json::to_string(&events).unwrap_or_else(|error| panic!("render json: {error}"));
	assert!(!rendered.contains("/private/repo"));
}

fn read_jsonl_events(path: &std::path::Path) -> Vec<Value> {
	std::fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("read telemetry file {}: {error}", path.display()))
		.lines()
		.map(|line| {
			serde_json::from_str(line)
				.unwrap_or_else(|error| panic!("parse telemetry json line {line:?}: {error}"))
		})
		.collect()
}
