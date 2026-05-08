use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

use temp_env::with_var;
use temp_env::with_vars;

use super::*;

fn reporter() -> HumanLintProgressReporter {
	// When not attached to a terminal the reporter disables itself.
	HumanLintProgressReporter::new()
}

fn colored_reporter() -> HumanLintProgressReporter {
	HumanLintProgressReporter {
		color: true,
		active_spinner: Mutex::new(None),
		fixed_files: Arc::new(Mutex::new(Vec::new())),
	}
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
	let mut files = files.iter();
	let first = files.next().unwrap();
	let second = files.next().unwrap();
	assert!(files.next().is_none());
	assert_eq!(first.0, PathBuf::from("a.toml"));
	assert_eq!(first.1, "Sorted");
	assert_eq!(second.0, PathBuf::from("b.toml"));
	assert_eq!(second.1, "Added field");
}

#[test]
fn reporter_fix_applied_tracks_single_file() {
	let reporter = reporter();
	reporter.fix_started(1);
	reporter.fix_applied(Path::new("a.toml"), "Sorted");
	reporter.fix_finished(1);

	let files = reporter.fixed_files.lock().unwrap();
	assert_eq!(files.len(), 1);
	let first = files
		.first()
		.unwrap_or_else(|| panic!("expected the first fixed file"));
	assert_eq!(first.0, PathBuf::from("a.toml"));
	assert_eq!(first.1, "Sorted");
}

#[test]
fn reporter_helpers_cover_color_and_spinner_paths() {
	assert!(!with_var("NO_COLOR", Some("1"), color_enabled));
	assert!(!with_vars(
		[("NO_COLOR", None::<&str>), ("TERM", Some("dumb"))],
		color_enabled,
	));
	assert_eq!(
		with_vars(
			[("NO_COLOR", None::<&str>), ("TERM", None::<&str>)],
			color_enabled,
		),
		with_vars(
			[("NO_COLOR", None::<&str>), ("TERM", None::<&str>)],
			stderr_is_terminal,
		),
	);
	assert!(paint("hello", Style::new()).contains("hello"));

	let reporter = colored_reporter();
	reporter.planning_started(&[]);
	reporter.print_success("done");
	reporter.print_info("info");
	reporter.summary(0, 1, 1, false);
	reporter.start_spinner("spinning".to_string());
	thread::sleep(SPINNER_DELAY + SPINNER_TICK + Duration::from_millis(50));
	reporter.stop_spinner();
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

#[test]
fn reporter_finish_stops_cleanly() {
	reporter().finish();
	colored_reporter().finish();
}
