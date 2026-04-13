use std::path::Path;
use std::process::Command;

pub fn git(root: &Path, args: &[&str]) {
	let output = git_command(root, args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

pub fn git_output(root: &Path, args: &[&str]) -> String {
	let output = git_command(root, args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("git output utf8: {error}"))
}

pub fn git_output_trimmed(root: &Path, args: &[&str]) -> String {
	git_output(root, args).trim().to_string()
}

fn git_command(root: &Path, args: &[&str]) -> Command {
	let mut command = Command::new("git");
	command.current_dir(root);
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}
	if matches!(args.first(), Some(&"commit")) {
		command.args(["-c", "commit.gpgsign=false"]);
	}
	command.args(args);
	command
}
