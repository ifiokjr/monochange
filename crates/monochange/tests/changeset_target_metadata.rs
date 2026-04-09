use std::fs;
use std::path::Path;

use serde_json::Value;

mod test_support;
use test_support::{monochange_command, setup_scenario_workspace};

fn cli() -> std::process::Command {
	monochange_command(Some("2026-04-06"))
}

#[cfg(unix)]
fn run_interactive_change_cli(workspace: &Path, output_path: &Path) -> std::process::Output {
	const SCRIPT: &str = r"
import os
import pty
import select
import subprocess
import time
import sys

workspace = os.environ['MC_WORKSPACE']
output_path = os.environ['MC_OUTPUT']
mc_bin = os.environ['MC_BIN']
master, slave = pty.openpty()
proc = subprocess.Popen(
    [mc_bin, 'change', '--interactive', '--reason', 'interactive reason', '--details', 'interactive details', '--output', output_path],
    cwd=workspace,
    stdin=slave,
    stdout=slave,
    stderr=slave,
    text=False,
    close_fds=True,
)
os.close(slave)

transcript = bytearray()

def drain(seconds):
    end = time.time() + seconds
    while time.time() < end:
        ready, _, _ = select.select([master], [], [], 0.05)
        if master in ready:
            try:
                data = os.read(master, 4096)
            except OSError:
                break
            if not data:
                break
            transcript.extend(data)
        if proc.poll() is not None:
            break

drain(0.5)
os.write(master, b' ')
time.sleep(0.1)
os.write(master, b'\r')
drain(0.5)
os.write(master, b'\x1b[B')
time.sleep(0.1)
os.write(master, b'\r')
drain(0.5)
os.write(master, b'\r')
drain(0.5)
os.write(master, b'\r')
drain(0.5)
proc.wait(timeout=10)
drain(0.2)
os.close(master)
sys.stdout.buffer.write(transcript)
sys.exit(proc.returncode)
";

	std::process::Command::new("python3")
		.arg("-c")
		.arg(SCRIPT)
		.env("MC_BIN", insta_cmd::get_cargo_bin("mc"))
		.env("MC_WORKSPACE", workspace)
		.env("MC_OUTPUT", output_path)
		.output()
		.unwrap_or_else(|error| panic!("interactive python harness: {error}"))
}

#[test]
fn change_cli_writes_scalar_type_shorthand_when_no_default_bump_is_configured() {
	let tempdir = setup_scenario_workspace("changeset-target-metadata/cli-type-only-change");
	let output_path = tempdir.path().join(".changeset/core-docs.md");

	let output = cli()
		.current_dir(tempdir.path())
		.arg("change")
		.arg("--package")
		.arg("core")
		.arg("--type")
		.arg("docs")
		.arg("--reason")
		.arg("clarify migration guide")
		.arg("--output")
		.arg(&output_path)
		.output()
		.unwrap_or_else(|error| panic!("change output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let contents = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(contents.contains("core: docs"));
	assert!(!contents.contains("bump:"));
}

#[test]
fn change_cli_rejects_unknown_change_type_for_configured_target() {
	let tempdir = setup_scenario_workspace("changeset-target-metadata/cli-type-only-change");

	let output = cli()
		.current_dir(tempdir.path())
		.arg("change")
		.arg("--package")
		.arg("core")
		.arg("--type")
		.arg("security")
		.arg("--reason")
		.arg("should fail")
		.output()
		.unwrap_or_else(|error| panic!("change output: {error}"));
	assert!(!output.status.success());
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(stderr.contains("invalid value 'security'"));
	assert!(stderr.contains("[possible values: docs, test]"));
}

#[test]
fn validate_accepts_scalar_type_shorthand_changesets() {
	let tempdir = setup_scenario_workspace("changeset-target-metadata/release-workspace");

	let output = cli()
		.current_dir(tempdir.path())
		.arg("validate")
		.output()
		.unwrap_or_else(|error| panic!("validate output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(String::from_utf8_lossy(&output.stdout).contains("workspace validation passed"));
}

#[test]
fn release_dry_run_json_supports_scalar_type_default_bumps() {
	let tempdir = setup_scenario_workspace("changeset-target-metadata/release-workspace");

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let json: Value = serde_json::from_slice(&output.stdout)
		.unwrap_or_else(|error| panic!("parse json: {error}"));
	let decisions = json["plan"]["decisions"]
		.as_array()
		.unwrap_or_else(|| panic!("decisions array"));
	assert!(decisions.iter().any(|decision| {
		decision["package"]
			.as_str()
			.is_some_and(|package| package.contains("crates/core/Cargo.toml"))
			&& decision["bump"].as_str() == Some("minor")
	}));
	assert!(decisions.iter().any(|decision| {
		decision["package"]
			.as_str()
			.is_some_and(|package| package.contains("crates/app/Cargo.toml"))
			&& decision["bump"].as_str() == Some("minor")
	}));
}

#[cfg(unix)]
#[test]
fn interactive_change_cli_writes_selected_bump() {
	let tempdir = setup_scenario_workspace("changeset-target-metadata/render-workspace");
	let output_path = tempdir.path().join("interactive.md");

	let output = run_interactive_change_cli(tempdir.path(), &output_path);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stdout)
	);
	assert!(String::from_utf8_lossy(&output.stdout).contains("wrote change file interactive.md"));

	let contents = fs::read_to_string(output_path).unwrap_or_else(|error| panic!("read: {error}"));
	assert!(contents.contains("sdk: patch"));
	assert!(contents.contains("# interactive reason"));
}

#[test]
fn release_rejects_legacy_reserved_metadata_blocks() {
	let tempdir = setup_scenario_workspace("monochange/release-base");
	fs::copy(
		tempdir.path().join(".changeset/feature.md"),
		tempdir.path().join(".changeset/base.md"),
	)
	.unwrap_or_else(|error| panic!("preserve base changeset: {error}"));
	fs::copy(
		Path::new(env!("CARGO_MANIFEST_DIR")).join(
			"../../fixtures/tests/monochange/release-with-compat-evidence/.changeset/feature.md",
		),
		tempdir.path().join(".changeset/feature.md"),
	)
	.unwrap_or_else(|error| panic!("seed legacy-style changeset: {error}"));

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release")
		.arg("--dry-run")
		.arg("--format")
		.arg("json")
		.output()
		.unwrap_or_else(|error| panic!("release output: {error}"));
	assert!(!output.status.success());
	assert!(String::from_utf8_lossy(&output.stderr)
		.contains("target `origin` uses unsupported field(s): core"));
}
