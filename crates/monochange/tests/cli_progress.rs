use std::path::Path;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use regex::Regex;
use serde_json::Value;

mod test_support;
use test_support::TtyAction;
use test_support::current_test_name;
use test_support::monochange_command;
use test_support::run_in_tty;
use test_support::setup_fixture;
use test_support::snapshot_settings;

fn normalize_duration_text(text: &str) -> String {
	let duration_pattern = Regex::new(r"\b\d+(?:\.\d+)?(?:ms|s|µs)\b")
		.unwrap_or_else(|error| panic!("regex: {error}"));
	duration_pattern
		.replace_all(text, "[duration]")
		.into_owned()
}

fn normalized_ascii_progress(stderr: &str) -> String {
	let normalized = normalize_duration_text(&normalize_terminal_transcript(stderr));
	normalized
		.lines()
		.filter(|line| !line.starts_with("  - "))
		.collect::<Vec<_>>()
		.join("\n")
}

fn normalized_progress_events(stderr: &str) -> Vec<Value> {
	let mut events = stderr
		.lines()
		.filter(|line| !line.trim().is_empty())
		.map(|line| {
			serde_json::from_str::<Value>(line)
				.unwrap_or_else(|error| panic!("parse progress event `{line}`: {error}"))
		})
		.collect::<Vec<_>>();
	for event in &mut events {
		let Some(object) = event.as_object_mut() else {
			panic!("progress event should be an object: {event}");
		};
		if let Some(duration) = object.get_mut("durationMs") {
			*duration = Value::String("[duration_ms]".to_string());
		}
		if let Some(phase_timings) = object.get_mut("phaseTimings").and_then(Value::as_array_mut) {
			for phase in phase_timings {
				if let Some(duration) = phase.get_mut("durationMs") {
					*duration = Value::String("[duration_ms]".to_string());
				}
			}
		}
	}
	let mut normalized = Vec::with_capacity(events.len());
	let mut index = 0;
	while index < events.len() {
		if events[index].get("event").and_then(Value::as_str) != Some("command_output") {
			normalized.push(events[index].clone());
			index += 1;
			continue;
		}
		let start = index;
		while index < events.len()
			&& events[index].get("event").and_then(Value::as_str) == Some("command_output")
		{
			index += 1;
		}
		let mut output_events = events[start..index].to_vec();
		output_events.sort_by(|left, right| {
			let left_key = (
				left.get("stepIndex")
					.and_then(Value::as_u64)
					.unwrap_or_default(),
				left.get("stream")
					.and_then(Value::as_str)
					.unwrap_or_default(),
				left.get("text").and_then(Value::as_str).unwrap_or_default(),
			);
			let right_key = (
				right
					.get("stepIndex")
					.and_then(Value::as_u64)
					.unwrap_or_default(),
				right
					.get("stream")
					.and_then(Value::as_str)
					.unwrap_or_default(),
				right
					.get("text")
					.and_then(Value::as_str)
					.unwrap_or_default(),
			);
			left_key.cmp(&right_key)
		});
		normalized.extend(output_events);
	}
	for (sequence, event) in normalized.iter_mut().enumerate() {
		if let Some(object) = event.as_object_mut() {
			object.insert(
				"sequence".to_string(),
				Value::String(format!("[sequence:{sequence}]")),
			);
		}
	}
	normalized
}

fn normalize_terminal_transcript(text: &str) -> String {
	let mut normalized = String::with_capacity(text.len());
	let mut chars = text.chars().peekable();
	while let Some(ch) = chars.next() {
		if ch == '\u{1b}' && chars.peek() == Some(&'[') {
			let _ = chars.next();
			for escape_ch in chars.by_ref() {
				if ('@'..='~').contains(&escape_ch) {
					break;
				}
			}
			continue;
		}
		if ch != '\r' {
			normalized.push(ch);
		}
	}
	normalized
}

#[cfg(unix)]
fn run_tty_command_result(workspace: &Path, command_name: &str) -> (i32, String) {
	let (status, transcript) = run_in_tty(workspace, &[command_name], None, &[]);
	(status, normalize_terminal_transcript(&transcript))
}

#[cfg(unix)]
fn run_tty_command(workspace: &Path, command_name: &str) -> String {
	let (status, transcript) = run_tty_command_result(workspace, command_name);
	assert_eq!(status, 0, "{transcript}");
	transcript
}

#[cfg(unix)]
fn run_tty_interactive_change(workspace: &Path, output_path: &Path) -> (i32, String) {
	let actions = [
		TtyAction::Sleep(std::time::Duration::from_millis(500)),
		TtyAction::Send {
			bytes: b" ",
			pause_after: std::time::Duration::from_millis(100),
		},
		TtyAction::Send {
			bytes: b"\r",
			pause_after: std::time::Duration::from_millis(500),
		},
		TtyAction::Send {
			bytes: b"\x1b[B",
			pause_after: std::time::Duration::from_millis(100),
		},
		TtyAction::Send {
			bytes: b"\r",
			pause_after: std::time::Duration::from_millis(500),
		},
		TtyAction::Send {
			bytes: b"\r",
			pause_after: std::time::Duration::from_millis(500),
		},
		TtyAction::Send {
			bytes: b"\r",
			pause_after: std::time::Duration::from_millis(500),
		},
	];
	let output = output_path.display().to_string();
	let (status, transcript) = run_in_tty(
		workspace,
		&[
			"change",
			"--interactive",
			"--reason",
			"interactive reason",
			"--details",
			"interactive details",
			"--output",
			output.as_str(),
		],
		None,
		&actions,
	);
	(status, normalize_terminal_transcript(&transcript))
}

#[cfg(not(unix))]
fn run_tty_command(_workspace: &Path, _command_name: &str) -> String {
	String::new()
}

#[test]
#[cfg(unix)]
fn release_progress_streams_named_steps_on_tty() {
	let tempdir = setup_fixture("monochange/release-progress");

	let transcript = run_tty_command(tempdir.path(), "progress-release");

	assert!(transcript.contains("[1/2] plan release (PrepareRelease)"));
	assert!(transcript.contains("[2/2] stream summary (Command)"));
	assert!(transcript.contains("stream summary [stdout] streamed line 1"));
	assert!(transcript.contains("stream summary [stdout] streamed line 2"));
	assert!(transcript.contains("`progress-release` finished"));
	assert!(transcript.contains("Summary"));
}

#[test]
#[cfg(unix)]
fn release_progress_renders_skipped_failed_steps_and_stderr_on_tty() {
	let tempdir = setup_fixture("monochange/release-progress-failure");

	let (status, transcript) = run_tty_command_result(tempdir.path(), "progress-failure");

	assert_ne!(status, 0, "expected failure transcript:\n{transcript}");
	assert!(transcript.contains("○ [1/3] skip validate (Validate) — skipped ({{ false }})"));
	assert!(transcript.contains("stderr only [stderr] warn line"));
	assert!(transcript.contains("✖ [3/3] fail loud (Command)"));
	assert!(transcript.contains("fail loud [stderr] bad line"));
	assert!(transcript.contains("└─ command `printf 'bad line\\n' >&2; exit 3` failed: bad line"));
}

#[test]
#[cfg(unix)]
fn interactive_change_cli_hides_progress_output_on_tty() {
	let tempdir = setup_fixture("monochange/release-base");
	let output_path = tempdir.path().join(".changeset/interactive.md");

	let (status, transcript) = run_tty_interactive_change(tempdir.path(), &output_path);

	assert_eq!(
		status, 0,
		"unexpected interactive transcript:\n{transcript}"
	);
	assert!(!transcript.contains("running `change`"), "{transcript}");
	assert!(!transcript.contains("[1/1]"), "{transcript}");
	assert!(!transcript.contains("finished"), "{transcript}");
	assert!(transcript.contains("wrote change file .changeset/interactive.md"));
	assert!(
		output_path.exists(),
		"interactive change file should be created"
	);
}

#[test]
fn ascii_progress_renders_clean_captured_output() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_fixture("monochange/release-progress");

	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("progress-ascii")
		.arg("--progress-format")
		.arg("ascii")
		.output()
		.unwrap_or_else(|error| panic!("run ascii progress command: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stderr = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("ascii stderr utf8: {error}"));
	assert_snapshot!(normalized_ascii_progress(&stderr));
}

#[test]
fn json_progress_emits_structured_events_for_machine_consumers() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = setup_fixture("monochange/release-progress");

	let output = monochange_command(Some("2026-04-06"))
		.current_dir(tempdir.path())
		.arg("progress-json")
		.arg("--progress-format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("run json progress command: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let stderr = String::from_utf8(output.stderr)
		.unwrap_or_else(|error| panic!("json stderr utf8: {error}"));
	assert_json_snapshot!(normalized_progress_events(&stderr));
}

#[test]
#[cfg(not(unix))]
fn release_progress_streams_named_steps_on_tty() {}

#[test]
#[cfg(not(unix))]
fn release_progress_renders_skipped_failed_steps_and_stderr_on_tty() {}

#[test]
#[cfg(not(unix))]
fn interactive_change_cli_hides_progress_output_on_tty() {}
