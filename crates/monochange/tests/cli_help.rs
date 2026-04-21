//! Integration tests for `mc help` subcommand output.

use insta_cmd::assert_cmd_snapshot;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::snapshot_settings;

fn mc_command() -> std::process::Command {
	let mut command = std::process::Command::new(insta_cmd::get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command.env_remove("RUST_LOG");
	command
}

#[test]
fn help_overview_lists_all_commands() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help"));
}

#[test]
fn help_change_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("change"));
}

#[test]
fn help_release_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("release"));
}

#[test]
fn help_unknown_command_shows_error_and_list() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("nonexistent"));
}

#[test]
fn help_init_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("init"));
}

#[test]
fn help_analyze_shows_detailed_help() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	assert_cmd_snapshot!(mc_command().arg("help").arg("analyze"));
}
