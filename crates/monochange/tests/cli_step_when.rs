use std::{fs, path::Path, process::Output};

mod test_support;
use test_support::{monochange_command, setup_scenario_workspace};

#[test]
fn cli_step_when_supports_logical_and_for_command_steps() {
	let root = setup_scenario_workspace("cli-step-when");

	let output = run_command(root.path(), "when-command", &["--run"]);
	assert!(output.status.success(), "{}", to_stderr(&output));
	assert!(
		!root.path().join("when-command-output.txt").exists(),
		"command should not run when extra=false"
	);

	let output = run_command(root.path(), "when-command", &["--run", "--extra"]);
	assert!(output.status.success(), "{}", to_stderr(&output));
	assert_eq!(
		fs::read_to_string(root.path().join("when-command-output.txt"))
			.unwrap_or_else(|error| panic!("read output: {error}")),
		"hello"
	);
}

#[test]
fn cli_step_when_supports_not_operator_for_command_steps() {
	let root = setup_scenario_workspace("cli-step-when");

	let output = run_command(root.path(), "not-condition", &[]);
	assert!(output.status.success(), "{}", to_stderr(&output));
	assert!(
		!root.path().join("not-output.txt").exists(),
		"command should not run when skip=true"
	);

	let output = run_command(root.path(), "not-condition", &["--skip=false"]);
	assert!(output.status.success(), "{}", to_stderr(&output));
	assert_eq!(
		fs::read_to_string(root.path().join("not-output.txt"))
			.unwrap_or_else(|error| panic!("read output: {error}")),
		"okay"
	);
}

#[test]
fn cli_step_when_skips_non_command_steps_when_false() {
	let root = setup_scenario_workspace("cli-step-when");
	let output = run_command(root.path(), "when-validate", &[]);
	assert!(output.status.success(), "{}", to_stderr(&output));
	let text = String::from_utf8_lossy(&output.stdout);
	assert!(text.contains("command `when-validate` completed"));
}

fn run_command(root: &Path, command: &str, args: &[&str]) -> Output {
	let output = monochange_command(None)
		.current_dir(root)
		.arg(command)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("command output: {error}"));
	if !output.status.success() {
		panic!("command failed: {}", to_stderr(&output));
	}
	output
}

fn to_stderr(output: &Output) -> String {
	String::from_utf8_lossy(&output.stderr).into_owned()
}
