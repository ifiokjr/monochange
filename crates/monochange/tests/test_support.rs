#![allow(clippy::indexing_slicing)]

use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
#[allow(unused_imports)]
pub use monochange_test_helpers::copy_directory;
#[allow(unused_imports)]
pub use monochange_test_helpers::current_test_name;
#[allow(unused_imports)]
pub use monochange_test_helpers::snapshot_settings;
#[cfg(unix)]
use portable_pty::CommandBuilder;
#[cfg(unix)]
use portable_pty::PtySize;
#[cfg(unix)]
use portable_pty::native_pty_system;
use serde_json::Map;
use serde_json::Value;

#[allow(unused_macros, unused_macro_rules)]
macro_rules! assert_readable_json_snapshot {
	($value:expr) => {{
		let value = serde_json::to_value(&$value)
			.unwrap_or_else(|error| panic!("serialize readable json snapshot: {error}"));
		let (redacted, multiline_fields) = $crate::test_support::redact_multiline_strings(&value);
		insta::assert_json_snapshot!(redacted);
		for (path, contents) in multiline_fields {
			insta::assert_snapshot!(
				format!(
					"multiline_{}",
					$crate::test_support::snapshot_path_slug(&path)
				),
				contents
			);
		}
	}};
	($name:expr, $value:expr) => {{
		let value = serde_json::to_value(&$value)
			.unwrap_or_else(|error| panic!("serialize readable json snapshot: {error}"));
		let (redacted, multiline_fields) = $crate::test_support::redact_multiline_strings(&value);
		insta::assert_json_snapshot!($name, redacted);
		for (path, contents) in multiline_fields {
			insta::assert_snapshot!(
				format!(
					"{}_multiline_{}",
					$name,
					$crate::test_support::snapshot_path_slug(&path)
				),
				contents
			);
		}
	}};
}

#[allow(unused_imports)]
pub(crate) use assert_readable_json_snapshot;

#[allow(dead_code)]
pub fn redact_multiline_strings(value: &Value) -> (Value, Vec<(String, String)>) {
	let mut redacted = value.clone();
	let mut multiline_fields = Vec::new();
	redact_multiline_strings_at(&mut redacted, "$", &mut multiline_fields);
	(redacted, multiline_fields)
}

#[allow(dead_code)]
pub fn snapshot_path_slug(path: &str) -> String {
	path.chars()
		.map(|character| {
			match character {
				'a'..='z' | 'A'..='Z' | '0'..='9' => character.to_ascii_lowercase(),
				_ => '_',
			}
		})
		.collect::<String>()
		.trim_matches('_')
		.to_owned()
}

fn readable_multiline_snapshot_contents(
	path: &str,
	contents: &str,
	multiline_fields: &mut Vec<(String, String)>,
) -> String {
	let mut readable = String::new();
	let mut remaining = contents;
	let mut code_block_index = 0;

	while let Some(start) = remaining.find("```json\n") {
		let (before, after_start) = remaining.split_at(start);
		let after_start = &after_start["```json\n".len()..];
		let Some(end) = after_start.find("\n```") else {
			break;
		};

		let (json_text, after_json) = after_start.split_at(end);
		let Ok(json_value) = serde_json::from_str::<Value>(json_text) else {
			break;
		};

		let (redacted_json, nested_fields) = redact_multiline_strings(&json_value);
		for (nested_path, nested_contents) in nested_fields {
			multiline_fields.push((
				format!("{path}.jsonBlocks[{code_block_index}]{nested_path}"),
				nested_contents,
			));
		}

		readable.push_str(before);
		readable.push_str("```json\n");
		readable.push_str(
			&serde_json::to_string_pretty(&redacted_json)
				.unwrap_or_else(|error| panic!("serialize redacted json code block: {error}")),
		);
		readable.push_str("\n```");
		remaining = &after_json["\n```".len()..];
		code_block_index += 1;
	}

	if readable.is_empty() {
		contents.to_owned()
	} else {
		readable.push_str(remaining);
		readable
	}
}

fn redact_multiline_strings_at(
	value: &mut Value,
	path: &str,
	multiline_fields: &mut Vec<(String, String)>,
) {
	match value {
		Value::String(contents) if contents.contains('\n') => {
			let snapshot_contents =
				readable_multiline_snapshot_contents(path, contents, multiline_fields);
			multiline_fields.push((path.to_owned(), snapshot_contents));
			"[multiline text]".clone_into(contents);
		}
		Value::Array(items) => {
			for (index, item) in items.iter_mut().enumerate() {
				redact_multiline_strings_at(item, &format!("{path}[{index}]"), multiline_fields);
			}
		}
		Value::Object(fields) => {
			for (key, field) in fields {
				redact_multiline_strings_at(field, &format!("{path}.{key}"), multiline_fields);
			}
		}
		_ => {}
	}
}

#[allow(dead_code)]
pub fn fixture_path(relative: &str) -> std::path::PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[allow(dead_code)]
pub fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[allow(dead_code)]
pub fn setup_scenario_workspace(relative: &str) -> tempfile::TempDir {
	let tempdir = monochange_test_helpers::fs::setup_scenario_workspace_from(
		env!("CARGO_MANIFEST_DIR"),
		relative,
	);
	if !relative.starts_with("affected/") {
		append_legacy_cli_commands_for_integration_tests(tempdir.path());
	}
	tempdir
}

fn append_legacy_cli_commands_for_integration_tests(root: &Path) {
	let config_path = root.join("monochange.toml");
	let Ok(mut config) = fs::read_to_string(&config_path) else {
		return;
	};

	let mut appended = String::new();

	for (name, table) in LEGACY_TEST_CLI_COMMANDS {
		if !config.contains(&format!("[cli.{name}]")) {
			appended.push_str("\n\n");
			appended.push_str(table);
		}
	}

	if appended.is_empty() {
		return;
	}

	config.push_str(&appended);
	fs::write(&config_path, config).unwrap_or_else(|error| {
		panic!("write test cli defaults {}: {error}", config_path.display())
	});
}

const LEGACY_TEST_CLI_COMMANDS: &[(&str, &str)] = &[
	(
		"discover",
		r#"[cli.discover]
help_text = "Discover packages across supported ecosystems"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json"], default = "text" },
]
steps = [{ name = "discover packages", type = "Discover" }]
"#,
	),
	(
		"change",
		r#"[cli.change]
help_text = "Create a change file for one or more packages"
inputs = [
	{ name = "interactive", type = "boolean", help_text = "Select packages, bumps, and options interactively", short = "i" },
	{ name = "package", type = "string_list", help_text = "Package or group to include in the change" },
	{ name = "bump", type = "choice", help_text = "Requested semantic version bump", choices = ["none", "patch", "minor", "major"], default = "patch" },
	{ name = "version", type = "string", help_text = "Pin an explicit version for this release" },
	{ name = "reason", type = "string", help_text = "Short release-note summary for this change" },
	{ name = "type", type = "string", help_text = "Optional release-note type such as `security` or `note`" },
	{ name = "caused_by", type = "string_list", help_text = "Package or group ids that caused this dependent change" },
	{ name = "details", type = "string", help_text = "Optional multi-line release-note details" },
	{ name = "output", type = "path", help_text = "Write the generated change file to a specific path" },
]
steps = [{ name = "create change file", type = "CreateChangeFile" }]
"#,
	),
	(
		"release",
		r#"[cli.release]
help_text = "Prepare a release from discovered change files"
inputs = [
	{ name = "format", type = "choice", choices = ["markdown", "text", "json"], default = "markdown" },
]
steps = [{ name = "prepare release", type = "PrepareRelease" }]
"#,
	),
	(
		"versions",
		r#"[cli.versions]
help_text = "Display planned package and group versions from discovered change files"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "text" },
]
steps = [{ name = "display versions", type = "DisplayVersions" }]
"#,
	),
	(
		"placeholder-publish",
		r#"[cli.placeholder-publish]
help_text = "Publish placeholder package versions for packages missing from their registries"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "text" },
	{ name = "package", type = "string_list", help_text = "Restrict placeholder publishing to explicit package ids" },
]
steps = [{ name = "publish placeholder packages", type = "PlaceholderPublish" }]
"#,
	),
	(
		"diagnostics",
		r#"[cli.diagnostics]
help_text = "Show per-changeset diagnostics including context and commit/PR context"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json"], default = "text" },
	{ name = "changeset", type = "string_list", help_text = "Changeset path(s) to inspect, relative to .changeset (omit for all changesets)" },
]
steps = [{ name = "diagnose changesets", type = "DiagnoseChangesets" }]
"#,
	),
];

#[allow(dead_code)]
pub fn monochange_command(release_date: Option<&str>) -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");

	if let Some(release_date) = release_date {
		command.env("MONOCHANGE_RELEASE_DATE", release_date);
	}

	command
}

#[cfg(unix)]
#[allow(dead_code)]
pub enum TtyAction<'a> {
	Sleep(std::time::Duration),
	Send {
		bytes: &'a [u8],
		pause_after: std::time::Duration,
	},
}

#[cfg(unix)]
#[allow(dead_code)]
pub fn run_in_tty(
	workspace: &Path,
	args: &[&str],
	release_date: Option<&str>,
	actions: &[TtyAction<'_>],
) -> (i32, String) {
	use std::io::Read as _;
	use std::io::Write as _;
	use std::thread;

	let pty_system = native_pty_system();
	let pair = pty_system
		.openpty(PtySize {
			rows: 24,
			cols: 80,
			pixel_width: 0,
			pixel_height: 0,
		})
		.unwrap_or_else(|error| panic!("open pty: {error}"));
	let mut command = CommandBuilder::new(get_cargo_bin("mc"));
	command.cwd(workspace);
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	if let Some(release_date) = release_date {
		command.env("MONOCHANGE_RELEASE_DATE", release_date);
	}
	for arg in args {
		command.arg(arg);
	}
	let mut child = pair
		.slave
		.spawn_command(command)
		.unwrap_or_else(|error| panic!("spawn tty command: {error}"));
	drop(pair.slave);

	let mut reader = pair
		.master
		.try_clone_reader()
		.unwrap_or_else(|error| panic!("clone tty reader: {error}"));
	let reader_thread = thread::spawn(move || {
		let mut transcript = Vec::new();
		reader
			.read_to_end(&mut transcript)
			.unwrap_or_else(|error| panic!("read tty transcript: {error}"));
		transcript
	});
	let mut writer = pair
		.master
		.take_writer()
		.unwrap_or_else(|error| panic!("take tty writer: {error}"));
	for action in actions {
		match action {
			TtyAction::Sleep(duration) => thread::sleep(*duration),
			TtyAction::Send { bytes, pause_after } => {
				match writer.write_all(bytes) {
					Ok(()) => {
						writer
							.flush()
							.unwrap_or_else(|error| panic!("flush tty input: {error}"));
						thread::sleep(*pause_after);
					}
					Err(error) if error.raw_os_error() == Some(5) => break,
					Err(error) => panic!("write tty input: {error}"),
				}
			}
		}
	}
	drop(writer);
	let status = child
		.wait()
		.unwrap_or_else(|error| panic!("wait for tty command: {error}"));
	drop(pair.master);
	let transcript = reader_thread
		.join()
		.unwrap_or_else(|_| panic!("tty reader thread panicked"));
	let status_code = status
		.exit_code()
		.try_into()
		.unwrap_or_else(|error| panic!("tty exit status conversion: {error}"));
	(
		status_code,
		String::from_utf8(transcript)
			.unwrap_or_else(|error| panic!("tty transcript utf8: {error}")),
	)
}

#[allow(dead_code)]
pub fn run_json_command(root: &Path, command: &str, release_date: Option<&str>) -> Value {
	let output = monochange_command(release_date)
		.current_dir(root)
		.arg(command)
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse command json: {error}"))
}

#[allow(dead_code)]
pub fn json_subset(value: &Value, fields: &[(&str, &str)]) -> Value {
	let mut subset = Map::new();
	for (key, pointer) in fields {
		subset.insert(
			(*key).to_string(),
			value.pointer(pointer).cloned().unwrap_or(Value::Null),
		);
	}
	Value::Object(subset)
}

#[cfg(test)]
mod tests {
	use std::fs;

	use rstest::rstest;
	use tempfile::TempDir;

	use super::copy_directory;
	use super::current_test_name;
	use super::fixture_path;
	use super::setup_fixture;
	use super::setup_scenario_workspace;

	#[test]
	fn current_test_name_returns_plain_function_name() {
		assert_eq!(
			current_test_name(),
			"current_test_name_returns_plain_function_name"
		);
	}

	#[rstest]
	fn case_1_strips_numeric_rstest_prefix_from_current_test_name() {
		assert_eq!(
			current_test_name(),
			"strips_numeric_rstest_prefix_from_current_test_name"
		);
	}

	#[test]
	fn fixture_path_resolves_known_fixture_directory() {
		let path = fixture_path("test-support/setup-fixture");
		assert!(path.is_dir());
		assert!(path.ends_with("fixtures/tests/test-support/setup-fixture"));
	}

	#[test]
	fn copy_directory_copies_nested_fixture_files() {
		let destination_root = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let destination = destination_root.path().join("copied");
		copy_directory(&fixture_path("test-support/setup-fixture"), &destination);
		assert_eq!(
			fs::read_to_string(destination.join("root.txt"))
				.unwrap_or_else(|error| panic!("read root fixture: {error}")),
			"root fixture\n"
		);
		assert_eq!(
			fs::read_to_string(destination.join("nested/child.txt"))
				.unwrap_or_else(|error| panic!("read nested fixture: {error}")),
			"nested child\n"
		);
	}

	#[test]
	fn setup_fixture_copies_fixture_contents_into_tempdir() {
		let tempdir = setup_fixture("test-support/setup-fixture");
		assert_eq!(
			fs::read_to_string(tempdir.path().join("nested/child.txt"))
				.unwrap_or_else(|error| panic!("read setup fixture: {error}")),
			"nested child\n"
		);
	}

	#[test]
	fn setup_scenario_workspace_prefers_workspace_directory_and_skips_expected_outputs() {
		let tempdir = setup_scenario_workspace("test-support/scenario-workspace");
		assert_eq!(
			fs::read_to_string(tempdir.path().join("workspace-only.txt"))
				.unwrap_or_else(|error| panic!("read workspace scenario file: {error}")),
			"workspace marker\n"
		);
		assert!(!tempdir.path().join("scenario-root-only.txt").exists());
		assert!(!tempdir.path().join("expected").exists());
	}

	#[test]
	fn setup_scenario_workspace_falls_back_to_scenario_root_when_no_workspace_exists() {
		let tempdir = setup_scenario_workspace("test-support/scenario-root");
		assert_eq!(
			fs::read_to_string(tempdir.path().join("root-only.txt"))
				.unwrap_or_else(|error| panic!("read root scenario file: {error}")),
			"root scenario\n"
		);
		assert!(!tempdir.path().join("expected").exists());
	}
}
