use std::fs;
use std::path::Path;

use monochange_test_helpers::copy_directory;
use monochange_test_helpers::copy_directory_skip_git;
use monochange_test_helpers::current_test_name;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output;
use monochange_test_helpers::git_output_trimmed;

fn write_file(path: &Path, contents: &str) {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create parent: {error}"));
	}
	fs::write(path, contents).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
}

#[test]
fn helper_fs_support_copies_fixture_trees_and_preserves_named_tests() {
	assert_eq!(
		current_test_name(),
		"helper_fs_support_copies_fixture_trees_and_preserves_named_tests"
	);
	let named = std::thread::Builder::new()
		.name("case_7_large_fixture_helper".to_string())
		.spawn(current_test_name)
		.unwrap_or_else(|error| panic!("spawn thread: {error}"))
		.join()
		.unwrap_or_else(|error| panic!("join thread: {error:?}"));
	assert_eq!(named, "large_fixture_helper");

	let source = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(&source.path().join("root.txt"), "root\n");
	write_file(&source.path().join("nested/child.txt"), "child\n");
	write_file(&source.path().join(".git/HEAD"), "ref: refs/heads/main\n");

	let copied = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(source.path(), copied.path());
	assert_eq!(
		fs::read_to_string(copied.path().join("nested/child.txt"))
			.unwrap_or_else(|error| panic!("read nested child: {error}")),
		"child\n"
	);
	assert!(copied.path().join(".git/HEAD").exists());

	let skipped = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory_skip_git(source.path(), skipped.path());
	assert_eq!(
		fs::read_to_string(skipped.path().join("root.txt"))
			.unwrap_or_else(|error| panic!("read root file: {error}")),
		"root\n"
	);
	assert!(!skipped.path().join(".git").exists());
}

#[test]
fn helper_git_support_uses_repo_root_and_strips_env_noise() {
	let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	git(repo.path(), &["init", "--initial-branch=main"]);
	git(repo.path(), &["config", "user.name", "monochange"]);
	git(
		repo.path(),
		&["config", "user.email", "monochange@example.com"],
	);

	write_file(&repo.path().join("README.md"), "hello\n");
	git(repo.path(), &["add", "README.md"]);
	git(repo.path(), &["commit", "-m", "feat: seed repo"]);

	let head = git_output_trimmed(repo.path(), &["rev-parse", "--abbrev-ref", "HEAD"]);
	assert_eq!(head, "main");

	let original_dir = std::env::var_os("GIT_DIR");
	let original_work_tree = std::env::var_os("GIT_WORK_TREE");
	let original_common_dir = std::env::var_os("GIT_COMMON_DIR");
	let original_index = std::env::var_os("GIT_INDEX_FILE");
	let original_object_dir = std::env::var_os("GIT_OBJECT_DIRECTORY");
	let original_alternates = std::env::var_os("GIT_ALTERNATE_OBJECT_DIRECTORIES");

	unsafe {
		std::env::set_var("GIT_DIR", "/definitely/wrong");
		std::env::set_var("GIT_WORK_TREE", "/definitely/wrong");
		std::env::set_var("GIT_COMMON_DIR", "/definitely/wrong");
		std::env::set_var("GIT_INDEX_FILE", "/definitely/wrong");
		std::env::set_var("GIT_OBJECT_DIRECTORY", "/definitely/wrong");
		std::env::set_var("GIT_ALTERNATE_OBJECT_DIRECTORIES", "/definitely/wrong");
	}

	let status = git_output(repo.path(), &["status", "--short"]);
	assert!(status.is_empty());

	unsafe {
		match original_dir {
			Some(value) => std::env::set_var("GIT_DIR", value),
			None => std::env::remove_var("GIT_DIR"),
		}
		match original_work_tree {
			Some(value) => std::env::set_var("GIT_WORK_TREE", value),
			None => std::env::remove_var("GIT_WORK_TREE"),
		}
		match original_common_dir {
			Some(value) => std::env::set_var("GIT_COMMON_DIR", value),
			None => std::env::remove_var("GIT_COMMON_DIR"),
		}
		match original_index {
			Some(value) => std::env::set_var("GIT_INDEX_FILE", value),
			None => std::env::remove_var("GIT_INDEX_FILE"),
		}
		match original_object_dir {
			Some(value) => std::env::set_var("GIT_OBJECT_DIRECTORY", value),
			None => std::env::remove_var("GIT_OBJECT_DIRECTORY"),
		}
		match original_alternates {
			Some(value) => std::env::set_var("GIT_ALTERNATE_OBJECT_DIRECTORIES", value),
			None => std::env::remove_var("GIT_ALTERNATE_OBJECT_DIRECTORIES"),
		}
	}
}
