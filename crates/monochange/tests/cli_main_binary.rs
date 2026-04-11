use std::process::Command;

use insta_cmd::assert_cmd_snapshot;
use insta_cmd::get_cargo_bin;

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
