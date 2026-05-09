use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use monochange_core::ShellConfig;

use super::*;

fn progress_reporter(enabled: bool, color: bool) -> CliProgressReporter {
	CliProgressReporter {
		enabled,
		color,
		animate: false,
		command_name: "release".to_string(),
		dry_run: false,
		total_steps: 3,
		writer_lock: Arc::new(Mutex::new(())),
		active_spinner: None,
		command_started: false,
		render_mode: ProgressRenderMode::Human,
		symbols: UNICODE_SYMBOLS,
		event_sequence: 0,
	}
}

fn named_command_step(name: &str) -> CliStepDefinition {
	CliStepDefinition::Command {
		show_progress: None,
		name: Some(name.to_string()),
		when: None,
		always_run: false,
		command: "echo hi".to_string(),
		dry_run_command: None,
		shell: ShellConfig::Default,
		id: None,
		variables: None,
		inputs: BTreeMap::new(),
	}
}

fn command_with_step(step: CliStepDefinition) -> CliCommandDefinition {
	CliCommandDefinition {
		name: "release".to_string(),
		help_text: Some("release".to_string()),
		inputs: Vec::new(),
		steps: vec![step],
	}
}

#[test]
fn format_duration_and_paint_text_cover_terminal_styles() {
	assert_eq!(paint_text("plain", Style::Detail, false), "plain");
	assert_eq!(
		paint_text("accent", Style::Accent, true),
		"\u{1b}[36;1maccent\u{1b}[0m"
	);
	assert_eq!(
		paint_text("success", Style::Success, true),
		"\u{1b}[32;1msuccess\u{1b}[0m"
	);
	assert_eq!(
		paint_text("warn", Style::Warning, true),
		"\u{1b}[33;1mwarn\u{1b}[0m"
	);
	assert_eq!(
		paint_text("error", Style::Error, true),
		"\u{1b}[31;1merror\u{1b}[0m"
	);
	assert_eq!(
		paint_text("detail", Style::Detail, true),
		"\u{1b}[35mdetail\u{1b}[0m"
	);
	assert_eq!(
		paint_text("header", Style::Header, true),
		"\u{1b}[37;1mheader\u{1b}[0m"
	);
	assert_eq!(
		paint_text("muted", Style::Muted, true),
		"\u{1b}[2mmuted\u{1b}[0m"
	);
	assert_eq!(format_duration(Duration::from_secs(61)), "61.0s");
	assert_eq!(format_duration(Duration::from_millis(1500)), "1.50s");
	assert_eq!(format_duration(Duration::from_micros(12)), "12µs");
}

#[test]
fn progress_format_parsing_and_renderer_selection_cover_all_variants() {
	let command = command_with_step(named_command_step("announce release"));
	assert_eq!(ProgressFormat::parse("auto"), Some(ProgressFormat::Auto));
	assert_eq!(
		ProgressFormat::parse("unicode"),
		Some(ProgressFormat::Unicode)
	);
	assert_eq!(ProgressFormat::parse("ascii"), Some(ProgressFormat::Ascii));
	assert_eq!(ProgressFormat::parse("json"), Some(ProgressFormat::Json));
	assert_eq!(ProgressFormat::parse("wat"), None);

	let unicode = CliProgressReporter::new(&command, false, false, ProgressFormat::Unicode);
	assert!(unicode.enabled);
	assert_eq!(unicode.render_mode, ProgressRenderMode::Human);
	assert_eq!(
		unicode.symbols.command_success,
		UNICODE_SYMBOLS.command_success
	);

	let ascii = CliProgressReporter::new(&command, false, false, ProgressFormat::Ascii);
	assert!(ascii.enabled);
	assert_eq!(ascii.render_mode, ProgressRenderMode::Human);
	assert_eq!(ascii.symbols.command_success, ASCII_SYMBOLS.command_success);

	let json = CliProgressReporter::new(&command, false, false, ProgressFormat::Json);
	assert!(json.enabled);
	assert_eq!(json.render_mode, ProgressRenderMode::Json);
	assert_eq!(json.symbols.command_success, ASCII_SYMBOLS.command_success);
}

#[test]
fn progress_reporter_renders_skips_failures_and_stderr_output_when_enabled() {
	let mut reporter = progress_reporter(true, false);
	let step = named_command_step("announce release");

	reporter.step_skipped(0, &step, None);
	reporter.step_skipped(0, &step, Some("{{ false }}"));
	reporter.log_command_output(0, &step, CommandStream::Stderr, "warn line\n");
	reporter.step_failed(1, &step, Duration::from_millis(25), "boom\nagain");
}

#[test]
fn progress_reporter_emits_json_skip_and_failure_events() {
	let mut reporter = progress_reporter(true, false);
	reporter.render_mode = ProgressRenderMode::Json;
	let step = named_command_step("announce release");

	reporter.step_skipped(0, &step, Some("{{ false }}"));
	reporter.step_failed(1, &step, Duration::from_millis(25), "boom");
}

#[test]
fn progress_reporter_updates_step_status_in_human_json_and_animated_modes() {
	let step = named_command_step("retarget release");
	let mut disabled = progress_reporter(false, false);
	disabled.step_status(0, &step, "locating release record");

	let mut human = progress_reporter(true, false);
	human.step_status(0, &step, "planning retarget");

	let mut json = progress_reporter(true, false);
	json.render_mode = ProgressRenderMode::Json;
	json.step_status(0, &step, "applying git ref and provider updates");
	assert_eq!(json.event_sequence, 1);

	let mut animated = progress_reporter(true, true);
	animated.animate = true;
	animated.step_status(0, &step, "syncing provider metadata");
	assert!(animated.active_spinner.is_some());
	animated.stop_spinner();
}

#[test]
fn progress_reporter_animates_named_steps_and_stops_cleanly() {
	let mut reporter = progress_reporter(true, true);
	reporter.animate = true;
	let step = named_command_step("announce release");

	reporter.command_started();
	reporter.step_started(0, &step);
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(20));
	reporter.step_finished(
		0,
		&step,
		Duration::from_millis(12),
		&[StepPhaseTiming {
			label: "build release plan".to_string(),
			duration: Duration::from_millis(8),
		}],
	);
	reporter.command_finished(Duration::from_millis(25));
}
