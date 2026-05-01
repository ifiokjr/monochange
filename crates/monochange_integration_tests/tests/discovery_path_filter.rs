use std::fs;
use std::path::Path;

use monochange_core::DiscoveryPathFilter;
use monochange_test_helpers::copy_directory;
use tempfile::TempDir;
use tempfile::tempdir;

#[test]
fn discovery_path_filter_rejects_gitignored_paths() {
	let fixture = setup_discovery_fixture("ignore-gitignored-nested-worktree");
	let root = fixture.path();
	let filter = DiscoveryPathFilter::new(root);

	assert!(!filter.should_descend(&root.join(".claude")));
	assert!(!filter.allows(&root.join(".claude/worktrees/feature")));
	assert!(filter.allows(&root.join("crates/root/Cargo.toml")));
}

#[test]
fn discovery_path_filter_rejects_paths_under_nested_git_worktrees() {
	let fixture = setup_discovery_fixture("ignore-automatic-nested-worktree");
	let root = fixture.path();
	let filter = DiscoveryPathFilter::new(root);

	assert!(!filter.should_descend(&root.join("sandbox/feature")));
	assert!(!filter.allows(&root.join("sandbox/feature/crates/ignored/Cargo.toml")));
	assert!(filter.allows(&root.join("crates/root/Cargo.toml")));
}

#[test]
fn discovery_path_filter_does_not_treat_parent_git_dir_outside_root_as_nested_worktree() {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo/ignore-parent-git-outside-root");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());

	let root = tempdir.path().join("workspace");
	let filter = DiscoveryPathFilter::new(&root);

	assert!(filter.allows(&root.join("crates/root/Cargo.toml")));
	assert!(filter.allows(&tempdir.path().join("outside/Cargo.toml")));
}

fn setup_discovery_fixture(name: &str) -> TempDir {
	let source = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/cargo")
		.join(name);
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&source, tempdir.path());
	materialize_nested_worktree_gitdir(tempdir.path());
	tempdir
}

fn materialize_nested_worktree_gitdir(root: &Path) {
	for (placeholder, git_path) in [
		(
			root.join("sandbox/feature/gitdir.txt"),
			root.join("sandbox/feature/.git"),
		),
		(
			root.join("feature.gitdir"),
			root.join(".claude/worktrees/feature/.git"),
		),
	] {
		if placeholder.is_file() {
			let gitdir = fs::read_to_string(&placeholder)
				.unwrap_or_else(|error| panic!("read {}: {error}", placeholder.display()));
			if let Some(parent) = git_path.parent() {
				fs::create_dir_all(parent)
					.unwrap_or_else(|error| panic!("create parent {}: {error}", parent.display()));
			}
			fs::write(&git_path, gitdir)
				.unwrap_or_else(|error| panic!("write {}: {error}", git_path.display()));
		}
	}
}
