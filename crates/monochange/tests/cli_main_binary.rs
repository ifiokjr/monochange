use std::path::PathBuf;
use std::process::Command;

use insta_cmd::assert_cmd_snapshot;
use insta_cmd::get_cargo_bin;

fn fixture_path(relative: &str) -> PathBuf {
	monochange_test_helpers::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), relative)
}

fn monochange_cli() -> Command {
	let mut command = Command::new(get_cargo_bin("monochange"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn monochange_binary_prints_help() {
	assert_cmd_snapshot!(monochange_cli().arg("--help"));
}

#[test]
fn monochange_binary_renders_cli_errors() {
	assert_cmd_snapshot!(monochange_cli().arg("not-a-command"));
}

#[test]
fn monochange_binary_quiet_suppresses_cli_errors() {
	let output = monochange_cli()
		.arg("--quiet")
		.arg("not-a-command")
		.output()
		.unwrap_or_else(|error| panic!("quiet cli error output: {error}"));
	assert!(!output.status.success());
	assert!(
		output.stdout.is_empty(),
		"quiet mode should suppress stdout"
	);
	assert!(
		output.stderr.is_empty(),
		"quiet mode should suppress stderr"
	);
}

#[test]
fn monochange_binary_accepts_equals_format_flags() {
	let root = fixture_path("cli-output/discover-mixed");
	assert_cmd_snapshot!(
		monochange_cli()
			.current_dir(root)
			.arg("step:discover")
			.arg("--format=json")
	);
}
