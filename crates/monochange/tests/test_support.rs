use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::{Map, Value};

#[allow(dead_code)]
#[path = "../../../testing/test_support/fs.rs"]
mod shared_fs_test_support;
#[allow(dead_code)]
#[path = "../../../testing/test_support/insta.rs"]
mod shared_insta_test_support;

#[allow(unused_imports)]
pub use shared_fs_test_support::copy_directory;
#[allow(unused_imports)]
pub use shared_fs_test_support::current_test_name;
#[allow(unused_imports)]
pub use shared_fs_test_support::fixture_path;
#[allow(unused_imports)]
pub use shared_fs_test_support::setup_scenario_workspace;
#[allow(unused_imports)]
pub use shared_insta_test_support::snapshot_settings;

#[allow(dead_code)]
pub fn monochange_command(release_date: Option<&str>) -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	if let Some(release_date) = release_date {
		command.env("MONOCHANGE_RELEASE_DATE", release_date);
	}
	command
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
