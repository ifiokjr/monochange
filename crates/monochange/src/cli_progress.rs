use std::cmp::Reverse;
use std::env;
use std::fmt::Write as _;
use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use monochange_core::CliCommandDefinition;
use monochange_core::CliStepDefinition;
use serde::Serialize;

use crate::StepPhaseTiming;

const UNICODE_SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ASCII_SPINNER_FRAMES: [&str; 4] = ["-", "\\", "|", "/"];
const SPINNER_TICK: Duration = Duration::from_millis(90);
const SPINNER_DELAY: Duration = Duration::from_millis(120);
const PHASE_TIMING_DETAIL_LIMIT: usize = 5;
const PHASE_TIMING_MINIMUM: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
pub(crate) enum CommandStream {
	Stdout,
	Stderr,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ProgressFormat {
	Auto,
	Unicode,
	Ascii,
	Json,
}

impl ProgressFormat {
	pub(crate) fn parse(value: &str) -> Option<Self> {
		match value {
			"auto" => Some(Self::Auto),
			"unicode" => Some(Self::Unicode),
			"ascii" => Some(Self::Ascii),
			"json" => Some(Self::Json),
			_ => None,
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ProgressRenderMode {
	Human,
	Json,
}

#[derive(Clone, Copy)]
struct ProgressSymbols {
	command_success: &'static str,
	step_start: &'static str,
	step_skip: &'static str,
	step_success: &'static str,
	step_failure: &'static str,
	error_branch: &'static str,
	bullet: &'static str,
	log_pipe: &'static str,
	spinner_frames: &'static [&'static str],
}

const UNICODE_SYMBOLS: ProgressSymbols = ProgressSymbols {
	command_success: "✓",
	step_start: "▶",
	step_skip: "○",
	step_success: "✔",
	step_failure: "✖",
	error_branch: "└─",
	bullet: "·",
	log_pipe: "│",
	spinner_frames: &UNICODE_SPINNER_FRAMES,
};

const ASCII_SYMBOLS: ProgressSymbols = ProgressSymbols {
	command_success: "+",
	step_start: ">",
	step_skip: "-",
	step_success: "+",
	step_failure: "x",
	error_branch: "`-",
	bullet: "-",
	log_pipe: "|",
	spinner_frames: &ASCII_SPINNER_FRAMES,
};

#[allow(clippy::struct_excessive_bools)]
pub(crate) struct CliProgressReporter {
	enabled: bool,
	color: bool,
	animate: bool,
	command_name: String,
	dry_run: bool,
	total_steps: usize,
	writer_lock: Arc<Mutex<()>>,
	active_spinner: Option<SpinnerState>,
	command_started: bool,
	render_mode: ProgressRenderMode,
	symbols: ProgressSymbols,
	event_sequence: u64,
}

struct SpinnerState {
	stop: Arc<AtomicBool>,
	rendered: Arc<AtomicBool>,
	handle: JoinHandle<()>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressPhaseTiming {
	label: String,
	duration_ms: u64,
}

impl CliProgressReporter {
	pub(crate) fn new(
		cli_command: &CliCommandDefinition,
		dry_run: bool,
		quiet: bool,
		format: ProgressFormat,
	) -> Self {
		let stderr_is_terminal = io::stderr().is_terminal();
		let color_enabled = stderr_is_terminal && env::var("TERM").is_ok_and(|term| term != "dumb");
		let no_color = env::var_os("NO_COLOR").is_some();
		let no_progress = env::var_os("MONOCHANGE_NO_PROGRESS").is_some();
		let (enabled, render_mode, symbols) = match format {
			ProgressFormat::Auto => {
				if quiet || no_progress || !stderr_is_terminal {
					(false, ProgressRenderMode::Human, UNICODE_SYMBOLS)
				} else {
					(true, ProgressRenderMode::Human, UNICODE_SYMBOLS)
				}
			}
			ProgressFormat::Unicode => (!quiet, ProgressRenderMode::Human, UNICODE_SYMBOLS),
			ProgressFormat::Ascii => (!quiet, ProgressRenderMode::Human, ASCII_SYMBOLS),
			ProgressFormat::Json => (!quiet, ProgressRenderMode::Json, ASCII_SYMBOLS),
		};
		let color =
			enabled && render_mode == ProgressRenderMode::Human && !no_color && color_enabled;
		let animate = enabled && render_mode == ProgressRenderMode::Human && stderr_is_terminal;
		Self {
			enabled,
			color,
			animate,
			command_name: cli_command.name.clone(),
			dry_run,
			total_steps: cli_command.steps.len(),
			writer_lock: Arc::new(Mutex::new(())),
			active_spinner: None,
			command_started: false,
			render_mode,
			symbols,
			event_sequence: 0,
		}
	}

	pub(crate) fn is_enabled(&self) -> bool {
		self.enabled
	}

	pub(crate) fn command_started(&mut self) {
		// Guard: skip if disabled or already started
		if !self.enabled || self.command_started {
			return;
		}
		self.command_started = true;

		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "command_started",
				"command": self.command_name,
				"dryRun": self.dry_run,
				"totalSteps": self.total_steps,
			}));
			return;
		}

		let suffix = if self.dry_run { " (dry-run)" } else { "" };
		self.print_line(&format!(
			"{} {}{}",
			self.paint("monochange", Style::Accent),
			self.paint(&format!("running `{}`", self.command_name), Style::Header),
			suffix,
		));
	}

	pub(crate) fn command_finished(&mut self, duration: Duration) {
		if !self.enabled || !self.command_started {
			return;
		}
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let sequence = self.next_sequence();
			self.emit_json_event(&serde_json::json!({
				"sequence": sequence,
				"event": "command_finished",
				"command": self.command_name,
				"dryRun": self.dry_run,
				"totalSteps": self.total_steps,
				"durationMs": duration_millis(duration),
			}));
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.command_success, Style::Success),
			self.paint(&format!("`{}` finished", self.command_name), Style::Header),
			self.paint(&format_duration(duration), Style::Muted),
		));
	}

	pub(crate) fn step_started(&mut self, step_index: usize, step: &CliStepDefinition) {
		if !self.enabled {
			return;
		}
		self.command_started();
		if self.render_mode == ProgressRenderMode::Json {
			self.emit_step_event("step_started", step_index, step, serde_json::Map::new());
			return;
		}
		let message = self.step_message(step_index, step);
		if self.animate {
			self.start_spinner(message);
		} else {
			self.print_line(&format!(
				"{} {message}",
				self.paint(self.symbols.step_start, Style::Accent)
			));
		}
	}

	pub(crate) fn step_skipped(
		&mut self,
		step_index: usize,
		step: &CliStepDefinition,
		condition: Option<&str>,
	) {
		if !self.enabled {
			return;
		}
		self.command_started();
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.extend(
				condition.map(|condition| ("condition".to_string(), condition.to_string().into())),
			);
			self.emit_step_event("step_skipped", step_index, step, payload);
			return;
		}
		let mut line = format!(
			"{} {} — {}",
			self.paint(self.symbols.step_skip, Style::Warning),
			self.step_message(step_index, step),
			self.paint("skipped", Style::Muted),
		);
		if let Some(condition) = condition {
			let _ = write!(
				line,
				" {}",
				self.paint(&format!("({condition})"), Style::Muted)
			);
		}
		self.print_line(&line);
	}

	pub(crate) fn step_finished(
		&mut self,
		step_index: usize,
		step: &CliStepDefinition,
		duration: Duration,
		phase_timings: &[StepPhaseTiming],
	) {
		if !self.enabled {
			return;
		}
		self.command_started();
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"durationMs".to_string(),
				serde_json::Value::from(duration_millis(duration)),
			);
			payload.insert(
				"phaseTimings".to_string(),
				serde_json::to_value(
					phase_timings
						.iter()
						.map(|phase| {
							ProgressPhaseTiming {
								label: phase.label.clone(),
								duration_ms: duration_millis(phase.duration),
							}
						})
						.collect::<Vec<_>>(),
				)
				.unwrap_or_else(|error| panic!("progress phase timing serialization: {error}")),
			);
			self.emit_step_event("step_finished", step_index, step, payload);
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_success, Style::Success),
			self.step_message(step_index, step),
			self.paint(&format_duration(duration), Style::Muted),
		));
		for phase in summarized_phase_timings(phase_timings) {
			self.print_line(&format!(
				"  {} {} {}",
				self.paint(self.symbols.bullet, Style::Muted),
				self.paint(&phase.label, Style::Detail),
				self.paint(&format_duration(phase.duration), Style::Muted),
			));
		}
	}

	pub(crate) fn step_failed(
		&mut self,
		step_index: usize,
		step: &CliStepDefinition,
		duration: Duration,
		error: &str,
	) {
		if !self.enabled {
			return;
		}
		self.command_started();
		self.stop_spinner();
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"durationMs".to_string(),
				serde_json::Value::from(duration_millis(duration)),
			);
			payload.insert(
				"error".to_string(),
				serde_json::Value::String(error.to_string()),
			);
			self.emit_step_event("step_failed", step_index, step, payload);
			return;
		}
		self.print_line(&format!(
			"{} {} {}",
			self.paint(self.symbols.step_failure, Style::Error),
			self.step_message(step_index, step),
			self.paint(&format_duration(duration), Style::Muted),
		));
		for (index, line) in error.lines().enumerate() {
			let branch = if index == 0 {
				self.symbols.error_branch
			} else {
				self.symbols.log_pipe
			};
			self.print_line(&format!(
				"  {} {}",
				self.paint(branch, Style::Error),
				self.paint(line, Style::Error),
			));
		}
	}

	pub(crate) fn log_command_output(
		&mut self,
		step_index: usize,
		step: &CliStepDefinition,
		stream: CommandStream,
		text: &str,
	) {
		if !self.enabled || text.is_empty() {
			return;
		}
		if self.render_mode == ProgressRenderMode::Json {
			let mut payload = serde_json::Map::new();
			payload.insert(
				"stream".to_string(),
				serde_json::Value::String(match stream {
					CommandStream::Stdout => "stdout".to_string(),
					CommandStream::Stderr => "stderr".to_string(),
				}),
			);
			payload.insert(
				"text".to_string(),
				serde_json::Value::String(text.to_string()),
			);
			self.emit_step_event("command_output", step_index, step, payload);
			return;
		}
		self.command_started();
		let stream_label = match stream {
			CommandStream::Stdout => self.paint("stdout", Style::Muted),
			CommandStream::Stderr => self.paint("stderr", Style::Warning),
		};
		let step_label = step.display_name();
		for line in text.lines() {
			self.print_line(&format!(
				"  {} {} {}",
				self.paint(self.symbols.log_pipe, Style::Muted),
				self.paint(&format!("{step_label} [{stream_label}]"), Style::Detail),
				line,
			));
		}
	}

	fn step_message(&self, step_index: usize, step: &CliStepDefinition) -> String {
		let name = step.display_name();
		let kind = step.kind_name();
		let detail = if name == kind {
			String::new()
		} else {
			format!(" {}", self.paint(&format!("({kind})"), Style::Muted))
		};
		format!(
			"{} {}{}",
			self.paint(
				&format!("[{}/{}]", step_index + 1, self.total_steps),
				Style::Muted,
			),
			self.paint(name, Style::Header),
			detail,
		)
	}

	fn start_spinner(&mut self, message: String) {
		self.stop_spinner();
		let stop = Arc::new(AtomicBool::new(false));
		let rendered = Arc::new(AtomicBool::new(false));
		let stop_flag = Arc::clone(&stop);
		let rendered_flag = Arc::clone(&rendered);
		let writer_lock = Arc::clone(&self.writer_lock);
		let color = self.color;
		let spinner_frames = self.symbols.spinner_frames;
		let handle = thread::spawn(move || {
			thread::sleep(SPINNER_DELAY);
			let mut frame_index = 0;
			while !stop_flag.load(Ordering::Relaxed) {
				let frame = spinner_frames[frame_index % spinner_frames.len()];
				with_stderr_lock(&writer_lock, || {
					eprint!(
						"\r\u{1b}[2K{} {}",
						paint_text(frame, Style::Accent, color),
						message,
					);
					let _ = io::stderr().flush();
				});
				rendered_flag.store(true, Ordering::Relaxed);
				thread::sleep(SPINNER_TICK);
				frame_index += 1;
			}
		});
		self.active_spinner = Some(SpinnerState {
			stop,
			rendered,
			handle,
		});
	}

	fn stop_spinner(&mut self) {
		let Some(spinner) = self.active_spinner.take() else {
			return;
		};
		spinner.stop.store(true, Ordering::Relaxed);
		let _ = spinner.handle.join();
		if spinner.rendered.load(Ordering::Relaxed) {
			with_stderr_lock(&self.writer_lock, || {
				eprint!("\r\u{1b}[2K");
				let _ = io::stderr().flush();
			});
		}
	}

	fn print_line(&self, text: &str) {
		with_stderr_lock(&self.writer_lock, || {
			eprint!("\r\u{1b}[2K");
			eprintln!("{text}");
			let _ = io::stderr().flush();
		});
	}

	fn paint(&self, text: &str, style: Style) -> String {
		paint_text(text, style, self.color)
	}

	fn next_sequence(&mut self) -> u64 {
		let current = self.event_sequence;
		self.event_sequence += 1;
		current
	}

	fn emit_step_event(
		&mut self,
		event: &str,
		step_index: usize,
		step: &CliStepDefinition,
		mut payload: serde_json::Map<String, serde_json::Value>,
	) {
		payload.insert(
			"sequence".to_string(),
			serde_json::Value::from(self.next_sequence()),
		);
		payload.insert(
			"event".to_string(),
			serde_json::Value::String(event.to_string()),
		);
		payload.insert(
			"command".to_string(),
			serde_json::Value::String(self.command_name.clone()),
		);
		payload.insert("dryRun".to_string(), serde_json::Value::Bool(self.dry_run));
		payload.insert(
			"stepIndex".to_string(),
			serde_json::Value::from(step_index + 1),
		);
		payload.insert(
			"totalSteps".to_string(),
			serde_json::Value::from(self.total_steps),
		);
		payload.insert(
			"stepKind".to_string(),
			serde_json::Value::String(step.kind_name().to_string()),
		);
		payload.insert(
			"stepDisplayName".to_string(),
			serde_json::Value::String(step.display_name().to_string()),
		);
		payload.insert(
			"stepName".to_string(),
			step.name().map_or(serde_json::Value::Null, |name| {
				serde_json::Value::String(name.to_string())
			}),
		);
		self.emit_json_event(&serde_json::Value::Object(payload));
	}

	fn emit_json_event(&self, value: &serde_json::Value) {
		with_stderr_lock(&self.writer_lock, || {
			eprintln!(
				"{}",
				serde_json::to_string(&value)
					.unwrap_or_else(|error| panic!("progress json event serialization: {error}"))
			);
			let _ = io::stderr().flush();
		});
	}
}

impl Drop for CliProgressReporter {
	fn drop(&mut self) {
		self.stop_spinner();
	}
}

#[derive(Clone, Copy)]
enum Style {
	Accent,
	Success,
	Warning,
	Error,
	Header,
	Detail,
	Muted,
}

fn paint_text(text: &str, style: Style, color: bool) -> String {
	if !color {
		return text.to_string();
	}
	let code = match style {
		Style::Accent => "36;1",
		Style::Success => "32;1",
		Style::Warning => "33;1",
		Style::Error => "31;1",
		Style::Header => "37;1",
		Style::Detail => "35",
		Style::Muted => "2",
	};
	format!("\u{1b}[{code}m{text}\u{1b}[0m")
}

fn with_stderr_lock(write_lock: &Arc<Mutex<()>>, action: impl FnOnce()) {
	let _lock = write_lock
		.lock()
		.unwrap_or_else(std::sync::PoisonError::into_inner);
	action();
}

fn format_duration(duration: Duration) -> String {
	if duration >= Duration::from_secs(60) {
		let seconds = duration.as_secs_f64();
		return format!("{seconds:.1}s");
	}
	if duration >= Duration::from_secs(1) {
		let seconds = duration.as_secs_f64();
		return format!("{seconds:.2}s");
	}
	if duration >= Duration::from_millis(1) {
		return format!("{}ms", duration.as_millis());
	}
	format!("{}µs", duration.as_micros())
}

fn duration_millis(duration: Duration) -> u64 {
	u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

fn summarized_phase_timings(phase_timings: &[StepPhaseTiming]) -> Vec<StepPhaseTiming> {
	let mut phase_timings = phase_timings
		.iter()
		.filter(|phase| phase.duration >= PHASE_TIMING_MINIMUM)
		.cloned()
		.collect::<Vec<_>>();
	phase_timings.sort_by_key(|phase| Reverse(phase.duration));
	phase_timings.truncate(PHASE_TIMING_DETAIL_LIMIT);
	phase_timings
}

#[cfg(test)]
mod tests {
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
	fn progress_reporter_animates_named_steps_and_stops_cleanly() {
		let mut reporter = progress_reporter(true, true);
		reporter.animate = true;
		let step = named_command_step("announce release");

		reporter.command_started();
		reporter.step_started(0, &step);
		thread::sleep(SPINNER_TICK + Duration::from_millis(20));
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
}
