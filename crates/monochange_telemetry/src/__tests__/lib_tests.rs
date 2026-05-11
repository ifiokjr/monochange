use std::sync::LazyLock;
use std::sync::Mutex;

static TEST_ENV_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

use std::io;
use std::path::Path;

use super::*;

#[test]
fn telemetry_is_disabled_without_environment_opt_in() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	temp_env::with_vars(
		[
			(TELEMETRY_ENV, None::<&str>),
			(TELEMETRY_FILE_ENV, None::<&str>),
		],
		|| assert!(matches!(TelemetrySink::from_env(), TelemetrySink::Disabled)),
	);
}

#[test]
fn telemetry_file_environment_enables_local_jsonl_sink() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let path = temp.path().join("telemetry.jsonl");
	let path_value = path.to_string_lossy().to_string();
	temp_env::with_vars(
		[
			(TELEMETRY_ENV, None::<&str>),
			(TELEMETRY_FILE_ENV, Some(path_value.as_str())),
		],
		|| {
			let sink = TelemetrySink::from_env();
			sink.capture_command(CommandTelemetry {
				command_name: "validate",
				dry_run: false,
				show_diff: false,
				progress_format: "auto",
				step_count: 1,
				duration: Duration::from_millis(42),
				outcome: TelemetryOutcome::Success,
				error: None,
			});
		},
	);

	let events = read_events(&path);
	assert_eq!(events.len(), 1);
	let event = event_at(&events, 0);
	assert_eq!(json_str(event, "/body/string_value"), "command_run");
	assert_eq!(json_str(event, "/resource/service.name"), "monochange");
	assert_eq!(json_str(event, "/scope/name"), TELEMETRY_SCOPE_NAME);
	assert_eq!(json_str(event, "/scope/version"), TELEMETRY_SCOPE_VERSION);
	assert_eq!(json_str(event, "/severity_text"), "INFO");
	assert!(json_value(event, "/time_unix_nano").as_u64().is_some());
	assert_eq!(json_str(event, "/attributes/command_name"), "validate");
	assert_eq!(json_str(event, "/attributes/command_source"), "configured");
	assert!(!json_bool(event, "/attributes/dry_run"));
	assert!(!json_bool(event, "/attributes/show_diff"));
	assert_eq!(json_str(event, "/attributes/progress_format"), "auto");
	assert_eq!(json_u64(event, "/attributes/step_count"), 1);
	assert_eq!(json_u64(event, "/attributes/duration_ms"), 42);
	assert_eq!(json_str(event, "/attributes/outcome"), "success");
	assert!(json_value(event, "/attributes/error_kind").is_null());
}

#[test]
fn telemetry_mode_environment_uses_default_state_file() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let state_home = temp.path().join("state");
	let state_home_value = state_home.to_string_lossy().to_string();
	temp_env::with_vars(
		[
			(TELEMETRY_ENV, Some("local")),
			(TELEMETRY_FILE_ENV, None::<&str>),
			("XDG_STATE_HOME", Some(state_home_value.as_str())),
			("HOME", None::<&str>),
		],
		|| {
			let sink = TelemetrySink::from_env();
			assert!(matches!(sink, TelemetrySink::LocalJsonl { .. }));
			sink.capture_step(StepTelemetry {
				command_name: "step:validate",
				step_index: 3,
				step_kind: "Validate",
				skipped: true,
				duration: Duration::from_millis(7),
				outcome: TelemetryOutcome::Skipped,
				error: Some(&MonochangeError::Config(
					"secret path /tmp/repo".to_string(),
				)),
			});
		},
	);

	let events = read_events(&state_home.join("monochange").join("telemetry.jsonl"));
	assert_eq!(events.len(), 1);
	let event = event_at(&events, 0);
	assert_eq!(json_str(event, "/body/string_value"), "command_step");
	assert_eq!(json_str(event, "/attributes/command_name"), "step:validate");
	assert_eq!(json_u64(event, "/attributes/step_index"), 3);
	assert_eq!(json_str(event, "/attributes/step_kind"), "Validate");
	assert!(json_bool(event, "/attributes/skipped"));
	assert_eq!(json_u64(event, "/attributes/duration_ms"), 7);
	assert_eq!(json_str(event, "/attributes/outcome"), "skipped");
	assert_eq!(json_str(event, "/attributes/error_kind"), "config_error");
	assert!(!event.to_string().contains("/tmp/repo"));
}

#[test]
fn telemetry_disable_mode_overrides_custom_file() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let path = temp.path().join("telemetry.jsonl");
	let path_value = path.to_string_lossy().to_string();
	temp_env::with_vars(
		[
			(TELEMETRY_ENV, Some("0")),
			(TELEMETRY_FILE_ENV, Some(path_value.as_str())),
		],
		|| {
			let sink = TelemetrySink::from_env();
			assert!(matches!(sink, TelemetrySink::Disabled));
			sink.capture_command(sample_command_telemetry("validate"));
		},
	);

	assert!(!path.exists());
}

#[test]
fn telemetry_helpers_use_stable_labels() {
	assert_eq!(TelemetryOutcome::Success.as_str(), "success");
	assert_eq!(TelemetryOutcome::Skipped.as_str(), "skipped");
	assert_eq!(TelemetryOutcome::Error.as_str(), "error");
	assert_eq!(command_source("validate"), "configured");
	assert_eq!(command_source("step:discover"), "generated_step");
}

#[test]
fn error_kind_uses_sanitized_categories() {
	let parse_source: Box<dyn std::error::Error + Send + Sync> = Box::new(io::Error::new(
		io::ErrorKind::InvalidData,
		"raw parse details",
	));
	let errors = [
		(MonochangeError::Io("raw io".to_string()), "io_error"),
		(
			MonochangeError::IoSource {
				path: PathBuf::from("/secret/repo/file"),
				source: io::Error::other("raw source"),
			},
			"io_error",
		),
		(
			MonochangeError::Config("raw config".to_string()),
			"config_error",
		),
		(
			MonochangeError::Discovery("raw discovery".to_string()),
			"discovery_error",
		),
		(
			MonochangeError::Diagnostic("raw diagnostic".to_string()),
			"diagnostic_error",
		),
		(
			MonochangeError::Parse {
				path: PathBuf::from("/secret/repo/config"),
				source: parse_source,
			},
			"parse_error",
		),
		(
			MonochangeError::Interactive {
				message: "raw prompt".to_string(),
			},
			"interactive_error",
		),
		(MonochangeError::Cancelled, "cancelled"),
	];

	for (error, expected) in errors {
		assert_eq!(error_kind(&error), expected);
	}

	#[cfg(feature = "http")]
	{
		let source = reqwest::blocking::get("http://[::1")
			.expect_err("invalid URL should fail before making a request");
		let error = MonochangeError::HttpRequest {
			context: "request".to_string(),
			source,
		};
		assert_eq!(error_kind(&error), "unknown_error");
	}
}

#[test]
fn default_telemetry_file_uses_home_when_state_home_is_absent() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let home = temp.path().join("home");
	let home_value = home.to_string_lossy().to_string();
	temp_env::with_vars(
		[
			("XDG_STATE_HOME", None::<&str>),
			("HOME", Some(home_value.as_str())),
		],
		|| {
			assert_eq!(
				default_telemetry_file(),
				home.join(".local")
					.join("state")
					.join("monochange")
					.join("telemetry.jsonl"),
			);
		},
	);
}

#[test]
fn default_telemetry_file_falls_back_to_workspace_relative_path() {
	let _guard = TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	temp_env::with_vars(
		[("XDG_STATE_HOME", None::<&str>), ("HOME", None::<&str>)],
		|| {
			assert_eq!(
				default_telemetry_file(),
				PathBuf::from(".monochange").join("telemetry.jsonl")
			);
		},
	);
}

#[test]
fn local_telemetry_write_failures_do_not_panic() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let sink = TelemetrySink::local_jsonl(temp.path().to_path_buf());
	sink.capture_command(sample_command_telemetry("validate"));
}

#[test]
fn write_event_supports_paths_without_parent_directory() {
	let temp = tempfile::tempdir().unwrap_or_else(|error| panic!("temporary directory: {error}"));
	let current_dir =
		env::current_dir().unwrap_or_else(|error| panic!("current directory: {error}"));
	env::set_current_dir(temp.path()).unwrap_or_else(|error| panic!("move into temp dir: {error}"));
	let result = write_event(Path::new("telemetry.jsonl"), "command_run", BTreeMap::new());
	env::set_current_dir(current_dir)
		.unwrap_or_else(|error| panic!("restore current dir: {error}"));
	result.unwrap_or_else(|error| panic!("event written: {error}"));
	assert!(temp.path().join("telemetry.jsonl").exists());
}

fn sample_command_telemetry(command_name: &str) -> CommandTelemetry<'_> {
	CommandTelemetry {
		command_name,
		dry_run: false,
		show_diff: false,
		progress_format: "auto",
		step_count: 1,
		duration: Duration::from_millis(42),
		outcome: TelemetryOutcome::Success,
		error: None,
	}
}

fn event_at(events: &[serde_json::Value], index: usize) -> &serde_json::Value {
	events
		.get(index)
		.unwrap_or_else(|| panic!("missing telemetry event {index}"))
}

fn json_value<'event>(
	event: &'event serde_json::Value,
	pointer: &str,
) -> &'event serde_json::Value {
	event
		.pointer(pointer)
		.unwrap_or_else(|| panic!("missing json pointer {pointer} in {event}"))
}

fn json_str<'event>(event: &'event serde_json::Value, pointer: &str) -> &'event str {
	json_value(event, pointer)
		.as_str()
		.unwrap_or_else(|| panic!("json pointer {pointer} is not a string in {event}"))
}

fn json_bool(event: &serde_json::Value, pointer: &str) -> bool {
	json_value(event, pointer)
		.as_bool()
		.unwrap_or_else(|| panic!("json pointer {pointer} is not a bool in {event}"))
}

#[test]
#[should_panic(expected = "is not an unsigned integer")]
fn json_u64_reports_type_mismatches() {
	let event = serde_json::json!({ "value": "not a number" });

	let _ = json_u64(&event, "/value");
}

fn json_u64(event: &serde_json::Value, pointer: &str) -> u64 {
	json_value(event, pointer)
		.as_u64()
		.unwrap_or_else(|| panic!("json pointer {pointer} is not an unsigned integer in {event}"))
}

fn read_events(path: &Path) -> Vec<serde_json::Value> {
	fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("telemetry file should be written: {error}"))
		.lines()
		.map(|line| {
			serde_json::from_str(line).unwrap_or_else(|error| panic!("valid json event: {error}"))
		})
		.collect()
}
