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

use crate::StepPhaseTiming;

const SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const SPINNER_TICK: Duration = Duration::from_millis(90);
const PHASE_TIMING_DETAIL_LIMIT: usize = 5;
const PHASE_TIMING_MINIMUM: Duration = Duration::from_millis(5);

#[derive(Clone, Copy)]
pub(crate) enum CommandStream {
	Stdout,
	Stderr,
}

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
}

struct SpinnerState {
	stop: Arc<AtomicBool>,
	handle: JoinHandle<()>,
}

impl CliProgressReporter {
	pub(crate) fn new(cli_command: &CliCommandDefinition, dry_run: bool, quiet: bool) -> Self {
		let color_enabled = env::var("TERM").is_ok_and(|term| term != "dumb");
		let enabled =
			!quiet && io::stderr().is_terminal() && env::var_os("MONOCHANGE_NO_PROGRESS").is_none();
		let color = enabled && env::var_os("NO_COLOR").is_none() && color_enabled;
		let animate = enabled && color;
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
		}
	}

	pub(crate) fn is_enabled(&self) -> bool {
		self.enabled
	}

	pub(crate) fn command_started(&mut self) {
		if !self.enabled || self.command_started {
			return;
		}
		let suffix = if self.dry_run { " (dry-run)" } else { "" };
		self.print_line(&format!(
			"{} {}{}",
			self.paint("monochange", Style::Accent),
			self.paint(&format!("running `{}`", self.command_name), Style::Header),
			suffix,
		));
		self.command_started = true;
	}

	pub(crate) fn command_finished(&mut self, duration: Duration) {
		if !self.enabled || !self.command_started {
			return;
		}
		self.stop_spinner();
		self.print_line(&format!(
			"{} {} {}",
			self.paint("✓", Style::Success),
			self.paint(&format!("`{}` finished", self.command_name), Style::Header),
			self.paint(&format_duration(duration), Style::Muted),
		));
	}

	pub(crate) fn step_started(&mut self, step_index: usize, step: &CliStepDefinition) {
		if !self.enabled {
			return;
		}
		self.command_started();
		let message = self.step_message(step_index, step);
		if self.animate {
			self.start_spinner(message);
		} else {
			self.print_line(&format!("{} {message}", self.paint("▶", Style::Accent)));
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
		let mut line = format!(
			"{} {} — {}",
			self.paint("○", Style::Warning),
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
		self.print_line(&format!(
			"{} {} {}",
			self.paint("✔", Style::Success),
			self.step_message(step_index, step),
			self.paint(&format_duration(duration), Style::Muted),
		));
		for phase in summarized_phase_timings(phase_timings) {
			self.print_line(&format!(
				"  {} {} {}",
				self.paint("·", Style::Muted),
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
		self.print_line(&format!(
			"{} {} {}",
			self.paint("✖", Style::Error),
			self.step_message(step_index, step),
			self.paint(&format_duration(duration), Style::Muted),
		));
		self.print_line(&format!(
			"  {} {}",
			self.paint("└─", Style::Error),
			self.paint(error, Style::Error),
		));
	}

	pub(crate) fn log_command_output(
		&mut self,
		step: &CliStepDefinition,
		stream: CommandStream,
		text: &str,
	) {
		if !self.enabled || text.trim().is_empty() {
			return;
		}
		self.command_started();
		let stream_label = match stream {
			CommandStream::Stdout => self.paint("stdout", Style::Muted),
			CommandStream::Stderr => self.paint("stderr", Style::Warning),
		};
		let step_label = step.display_name();
		for line in text.lines().filter(|line| !line.trim().is_empty()) {
			self.print_line(&format!(
				"  {} {} {}",
				self.paint("│", Style::Muted),
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
		let stop_flag = Arc::clone(&stop);
		let writer_lock = Arc::clone(&self.writer_lock);
		let color = self.color;
		let handle = thread::spawn(move || {
			let mut frame_index = 0;
			while !stop_flag.load(Ordering::Relaxed) {
				let frame = SPINNER_FRAMES[frame_index % SPINNER_FRAMES.len()];
				with_stderr_lock(&writer_lock, || {
					eprint!(
						"\r\u{1b}[2K{} {}",
						paint_text(frame, Style::Accent, color),
						message,
					);
					let _ = io::stderr().flush();
				});
				thread::sleep(SPINNER_TICK);
				frame_index += 1;
			}
		});
		self.active_spinner = Some(SpinnerState { stop, handle });
	}

	fn stop_spinner(&mut self) {
		let Some(spinner) = self.active_spinner.take() else {
			return;
		};
		spinner.stop.store(true, Ordering::Relaxed);
		let _ = spinner.handle.join();
		with_stderr_lock(&self.writer_lock, || {
			eprint!("\r\u{1b}[2K");
			let _ = io::stderr().flush();
		});
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
	fn progress_reporter_renders_skips_failures_and_stderr_output_when_enabled() {
		let mut reporter = progress_reporter(true, false);
		let step = named_command_step("announce release");

		reporter.step_skipped(0, &step, None);
		reporter.step_skipped(0, &step, Some("{{ false }}"));
		reporter.log_command_output(&step, CommandStream::Stderr, "warn line\n");
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
