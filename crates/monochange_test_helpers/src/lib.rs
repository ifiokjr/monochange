//! # `monochange_test_helpers`
//!
//! <!-- {=monochangeTestHelpersCrateDocs|trim|linePrefix:"//! ":true} -->
//! `monochange_test_helpers` packages the shared fixture, snapshot, git, and RMCP helpers used across the workspace test suite.
//!
//! Reach for this crate when you are writing integration or fixture-heavy tests that need scenario workspaces, command snapshots, or temporary git repositories.
//!
//! ## Why use it?
//!
//! - keep tests focused on behavior instead of tempdir and setup boilerplate
//! - share consistent fixture loading across crates
//! - reuse snapshot and git helpers in integration suites
//!
//! ## Best for
//!
//! - copying fixture workspaces into temp directories
//! - writing git-backed integration tests
//! - configuring `insta` snapshots and RMCP content assertions
//!
//! ## Public entry points
//!
//! - `copy_directory` and `copy_directory_skip_git` clone fixture trees into temp workspaces
//! - `git`, `git_output`, and `git_output_trimmed` run test git commands
//! - `snapshot_settings()` configures shared snapshot behavior
//! - `fixture_path!`, `setup_fixture!`, and `setup_scenario_workspace!` locate and materialize test fixtures
//! <!-- {/monochangeTestHelpersCrateDocs} -->

pub mod fs;
pub mod git;
pub mod insta;
pub mod rmcp;
pub mod workspace_ops;

pub use fs::copy_directory;
pub use fs::copy_directory_skip_git;
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
