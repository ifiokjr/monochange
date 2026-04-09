pub mod fs;
pub mod git;
pub mod insta;
pub mod rmcp;

pub use fs::copy_directory;
pub use fs::current_test_name;
pub use git::git;
pub use git::git_output;
pub use git::git_output_trimmed;
pub use insta::snapshot_settings;
pub use rmcp::content_text;

#[macro_export]
macro_rules! fixture_path {
	($relative:expr) => {
		$crate::fs::fixture_path_from(env!("CARGO_MANIFEST_DIR"), $relative)
	};
}

#[macro_export]
macro_rules! setup_fixture {
	($relative:expr) => {
		$crate::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), $relative)
	};
}

#[macro_export]
macro_rules! setup_scenario_workspace {
	($relative:expr) => {
		$crate::fs::setup_scenario_workspace_from(env!("CARGO_MANIFEST_DIR"), $relative)
	};
}
