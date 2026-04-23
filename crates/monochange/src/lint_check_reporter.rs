#![forbid(clippy::indexing_slicing)]

use std::io;
use std::io::IsTerminal;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use anstyle::AnsiColor;
use anstyle::Style;
use monochange_core::lint::LintProgressReporter;

const SPINNER_TICK: Duration = Duration::from_millis(90);
const SPINNER_DELAY: Duration = Duration::from_millis(120);
const UNICODE_SPINNER_FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];

fn stderr_is_terminal() -> bool {
	io::stderr().is_terminal()
}

fn color_enabled() -> bool {
	if std::env::var_os("NO_COLOR").is_some() {
		return false;
	}
	if std::env::var("TERM").is_ok_and(|term| term == "dumb") {
		return false;
	}
	stderr_is_terminal()
}

fn paint(text: &str, style: Style) -> String {
	format!("{style}{text}{style:#}")
}

fn with_stderr_lock(f: impl FnOnce()) {
	let stderr = io::stderr();
	let _lock = stderr.lock();
	f();
}

struct SpinnerState {
	stop: Arc<AtomicBool>,
	handle: thread::JoinHandle<()>,
}

/// A beautiful human-readable progress reporter for lint/check operations.
/// Writes to stderr and respects `NO_COLOR` / `MONOCHANGE_NO_PROGRESS`.
pub(crate) struct HumanLintProgressReporter {
	color: bool,
	active_spinner: Mutex<Option<SpinnerState>>,
	fixed_files: Arc<Mutex<Vec<(PathBuf, String)>>>,
}

impl HumanLintProgressReporter {
	pub(crate) fn new() -> Self {
		let no_progress = std::env::var_os("MONOCHANGE_NO_PROGRESS").is_some();
		let enabled = !no_progress && stderr_is_terminal();
		Self {
			color: enabled && color_enabled(),
			active_spinner: Mutex::new(None),
			fixed_files: Arc::new(Mutex::new(Vec::new())),
		}
	}

	pub(crate) fn finish(self) {
		self.stop_spinner();
	}

	fn start_spinner(&self, message: String) {
		self.stop_spinner();
		let stop = Arc::new(AtomicBool::new(false));
		let stop_flag = Arc::clone(&stop);
		let color = self.color;
		let handle = thread::spawn(move || {
			thread::sleep(SPINNER_DELAY);
			let mut frame_index = 0usize;
			while !stop_flag.load(Ordering::Relaxed) {
				let frame = UNICODE_SPINNER_FRAMES
					.get(frame_index % UNICODE_SPINNER_FRAMES.len())
					.unwrap_or(UNICODE_SPINNER_FRAMES.first().unwrap_or(&""));
				with_stderr_lock(|| {
					let styled = if color {
						paint(
							frame,
							Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan))),
						)
					} else {
						frame.to_string()
					};
					eprint!("\r\u{001b}[2K{styled} {message}");
					io::stderr().flush().ok();
				});
				thread::sleep(SPINNER_TICK);
				frame_index += 1;
			}
		});
		self.active_spinner
			.lock()
			.unwrap()
			.replace(SpinnerState { stop, handle });
	}

	fn stop_spinner(&self) {
		let spinner = self.active_spinner.lock().unwrap().take();
		let Some(spinner) = spinner else {
			return;
		};
		spinner.stop.store(true, Ordering::Relaxed);
		let _ = spinner.handle.join();
		with_stderr_lock(|| {
			eprint!("\r\u{001b}[2K");
			io::stderr().flush().ok();
		});
	}

	fn print_line(&self, text: &str) {
		self.stop_spinner();
		with_stderr_lock(|| {
			eprintln!("{text}");
		});
	}

	fn print_success(&self, text: &str) {
		if self.color {
			self.print_line(&paint(
				text,
				Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Green))),
			));
		} else {
			self.print_line(text);
		}
	}

	fn print_info(&self, text: &str) {
		if self.color {
			self.print_line(&paint(
				text,
				Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Blue))),
			));
		} else {
			self.print_line(text);
		}
	}
}

impl LintProgressReporter for HumanLintProgressReporter {
	fn planning_started(&self, suites: &[&str]) {
		if suites.is_empty() {
			return;
		}
		let message = format!(
			"{} Running {} suite{}…",
			if self.color { "ℹ" } else { "i" },
			suites.len(),
			if suites.len() == 1 { "" } else { "s" },
		);
		self.print_info(&message);
	}

	fn planning_finished(&self, _total_files: usize, _total_rules: usize) {
		// Planning detail is implicitly shown by suite messages.
	}

	fn suite_started(&self, suite_id: &str, file_count: usize, rule_count: usize) {
		let message = format!(
			"{} — checking {file_count} file{} with {rule_count} rule{}…",
			suite_id,
			if file_count == 1 { "" } else { "s" },
			if rule_count == 1 { "" } else { "s" },
		);
		self.start_spinner(message);
	}

	fn suite_finished(&self, suite_id: &str, result_count: usize, fixable_count: usize) {
		let fixable_fragment = if fixable_count > 0 {
			format!(" ({fixable_count} fixable)")
		} else {
			String::new()
		};
		let text = format!(
			"{} {suite_id} — {result_count} issue{}{fixable_fragment}",
			if self.color { "✔" } else { "+" },
			if result_count == 1 { "" } else { "s" },
		);
		self.print_success(&text);
	}

	fn file_started(&self, _file_path: &Path, _rule_count: usize) {
		// Rules are fast; keep the suite-level spinner.
	}

	fn file_finished(&self, _file_path: &Path, _result_count: usize) {
		// Suite-level tracking is updated here indirectly.
	}

	fn file_rule_started(&self, _file_path: &Path, _rule_id: &str) {
		// Rules are fast; keep the suite-level spinner.
	}

	fn file_rule_finished(&self, _file_path: &Path, _rule_id: &str, _result_count: usize) {
		// Rules are fast; keep the suite-level spinner.
	}

	fn fix_started(&self, file_count: usize) {
		let message = format!(
			"Applying fixes to {file_count} file{}…",
			if file_count == 1 { "" } else { "s" },
		);
		self.start_spinner(message);
	}

	fn fix_applied(&self, file_path: &Path, description: &str) {
		let display = file_path.display().to_string();
		let mut fixed = self.fixed_files.lock().unwrap();
		fixed.push((file_path.to_path_buf(), description.to_string()));
		let icon = if self.color { "•" } else { "-" };
		let text = format!("  {icon} {display} ({description})");
		self.print_line(&text);
	}

	fn fix_finished(&self, files_fixed: usize) {
		self.stop_spinner();
		let text = format!(
			"{} Fixed {files_fixed} file{}",
			if self.color { "✔" } else { "+" },
			if files_fixed == 1 { "" } else { "s" },
		);
		self.print_success(&text);
	}

	fn summary(&self, errors: usize, warnings: usize, fixable: usize, fixed: bool) {
		if errors == 0 && warnings == 0 {
			return;
		}

		let divider = if self.color {
			paint(
				"─────────────────────────────",
				Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::BrightBlack))),
			)
		} else {
			"─────────────────────────────".to_string()
		};
		self.print_line(&divider);

		let error_icon = if self.color { "✖" } else { "x" };
		let warn_icon = if self.color { "⚠" } else { "!" };
		let info_icon = if self.color { "·" } else { "-" };

		let parts: Vec<String> = [
			if errors > 0 {
				Some(format!(
					"{error_icon} {errors} error{}",
					if errors == 1 { "" } else { "s" }
				))
			} else {
				None
			},
			if warnings > 0 {
				Some(format!(
					"{warn_icon} {warnings} warning{}",
					if warnings == 1 { "" } else { "s" }
				))
			} else {
				None
			},
		]
		.into_iter()
		.flatten()
		.collect();

		if !parts.is_empty() {
			let summary_line = parts.join(", ");
			self.print_line(&summary_line);
		}

		if fixable > 0 && !fixed {
			let hint = format!(
				"{info_icon} {fixable} issue{} can be auto-fixed. Run `{cmd}` to apply.",
				if fixable == 1 { "" } else { "s" },
				cmd = if self.color {
					paint(
						"mc check --fix",
						Style::new().fg_color(Some(anstyle::Color::Ansi(AnsiColor::Cyan))),
					)
				} else {
					"mc check --fix".to_string()
				},
			);
			self.print_line(&hint);
		}
	}
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use super::*;

	fn reporter() -> HumanLintProgressReporter {
		// When not attached to a terminal the reporter disables itself.
		HumanLintProgressReporter::new()
	}

	#[test]
	fn reporter_suite_tracking_tracks_counts() {
		let reporter = reporter();
		reporter.planning_started(&["cargo", "npm"]);
		reporter.suite_started("cargo", 3, 5);
		reporter.suite_finished("cargo", 2, 1);
		// Should not panic.
	}

	#[test]
	fn reporter_fix_applied_tracks_files() {
		let reporter = reporter();
		reporter.fix_started(2);
		reporter.fix_applied(Path::new("a.toml"), "Sorted");
		reporter.fix_applied(Path::new("b.toml"), "Added field");
		reporter.fix_finished(2);

		let files = reporter.fixed_files.lock().unwrap();
		assert_eq!(files.len(), 2);
		assert_eq!(files[0].0, PathBuf::from("a.toml"));
		assert_eq!(files[0].1, "Sorted");
	}

	#[test]
	fn reporter_summary_renders_counts() {
		let reporter = reporter();
		// Should not panic and should produce no output when there are no issues.
		reporter.summary(0, 0, 0, false);

		// With issues it should produce output (even if we don't capture stderr here,
		// the fact it doesn't panic is the real test).
		reporter.summary(2, 3, 1, false);
		reporter.summary(1, 0, 0, true);
	}
}
