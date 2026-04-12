use std::fs;
use std::path::Path;

mod test_support;
use test_support::setup_fixture;

#[cfg(unix)]
fn run_tty_command_result(
	workspace: &Path,
	command_name: &str,
) -> (std::process::ExitStatus, String) {
	const SCRIPT: &str = r"
import os
import pty
import select
import subprocess
import sys
import time

workspace = os.environ['MC_WORKSPACE']
command_name = os.environ['MC_COMMAND']
mc_bin = os.environ['MC_BIN']
master, slave = pty.openpty()
proc = subprocess.Popen(
    [mc_bin, command_name],
    cwd=workspace,
    stdin=slave,
    stdout=slave,
    stderr=slave,
    text=False,
    close_fds=True,
    env={**os.environ, 'NO_COLOR': '1'},
)
os.close(slave)
transcript = bytearray()
while True:
    ready, _, _ = select.select([master], [], [], 0.05)
    if master in ready:
        try:
            data = os.read(master, 4096)
        except OSError:
            break
        if not data:
            break
        transcript.extend(data)
    if proc.poll() is not None and not ready:
        break
proc.wait(timeout=10)
time.sleep(0.1)
while True:
    ready, _, _ = select.select([master], [], [], 0.01)
    if master not in ready:
        break
    data = os.read(master, 4096)
    if not data:
        break
    transcript.extend(data)
os.close(master)
sys.stdout.buffer.write(transcript)
sys.exit(proc.returncode)
";

	let output = std::process::Command::new("python3")
		.arg("-c")
		.arg(SCRIPT)
		.env("MC_BIN", insta_cmd::get_cargo_bin("mc"))
		.env("MC_WORKSPACE", workspace)
		.env("MC_COMMAND", command_name)
		.output()
		.unwrap_or_else(|error| panic!("tty python harness: {error}"));
	let transcript =
		String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("tty output utf8: {error}"));
	(output.status, transcript)
}

#[cfg(unix)]
fn run_tty_command(workspace: &Path, command_name: &str) -> String {
	let (status, transcript) = run_tty_command_result(workspace, command_name);
	assert!(status.success(), "{}", transcript);
	transcript
}

#[cfg(not(unix))]
fn run_tty_command(_workspace: &Path, _command_name: &str) -> String {
	String::new()
}

#[test]
#[cfg(unix)]
fn release_progress_streams_named_steps_on_tty() {
	let tempdir = setup_fixture("monochange/release-base");
	let config_path = tempdir.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		r#"

[cli.progress-release]
help_text = "Prepare a release with progress output"
steps = [
	{ type = "PrepareRelease", name = "plan release" },
	{ type = "Command", name = "stream summary", shell = true, command = "printf 'streamed line 1\n'; sleep 0.1; printf 'streamed line 2\n'" },
]
"#,
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let transcript = run_tty_command(tempdir.path(), "progress-release");

	assert!(transcript.contains("[1/2] plan release (PrepareRelease)"));
	assert!(transcript.contains("[2/2] stream summary (Command)"));
	assert!(transcript.contains("stream summary [stdout] streamed line 1"));
	assert!(transcript.contains("stream summary [stdout] streamed line 2"));
	assert!(transcript.contains("`progress-release` finished"));
	assert!(transcript.contains("command `progress-release` completed"));
}

#[test]
#[cfg(unix)]
fn release_progress_renders_skipped_failed_steps_and_stderr_on_tty() {
	let tempdir = setup_fixture("monochange/release-base");
	let config_path = tempdir.path().join("monochange.toml");
	let mut config = fs::read_to_string(&config_path)
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	config.push_str(
		r#"

[cli.progress-failure]
help_text = "Exercise skipped and failed progress output"
steps = [
	{ type = "Validate", name = "skip validate", when = "{{ false }}" },
	{ type = "Command", name = "stderr only", shell = true, command = "printf 'warn line\n' >&2" },
	{ type = "Command", name = "fail loud", shell = true, command = "printf 'bad line\n' >&2; exit 3" },
]
"#,
	);
	fs::write(&config_path, config)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let (status, transcript) = run_tty_command_result(tempdir.path(), "progress-failure");

	assert!(
		!status.success(),
		"expected failure transcript:\n{transcript}"
	);
	assert!(transcript.contains("○ [1/3] skip validate (Validate) — skipped ({{ false }})"));
	assert!(transcript.contains("stderr only [stderr] warn line"));
	assert!(transcript.contains("✖ [3/3] fail loud (Command)"));
	assert!(transcript.contains("fail loud [stderr] bad line"));
	assert!(transcript.contains("└─ command `printf 'bad line\\n' >&2; exit 3` failed: bad line"));
}

#[test]
#[cfg(not(unix))]
fn release_progress_streams_named_steps_on_tty() {}

#[test]
#[cfg(not(unix))]
fn release_progress_renders_skipped_failed_steps_and_stderr_on_tty() {}
