use std::fs;
use std::path::Path;
use std::process::Command;

use insta::assert_snapshot;

mod test_support;
use test_support::current_test_name;
use test_support::fixture_path;
use test_support::snapshot_settings;

fn repo_root() -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap_or_else(|error| panic!("repo root: {error}"))
}

#[test]
fn benchmark_cli_comment_renderer_matches_snapshot() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let fixture = fixture_path("monochange/benchmark-cli-comment");
	let output_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let output_path = output_dir.path().join("comment.md");
	let script_path = repo_root().join("scripts/benchmark-cli.mjs");

	let output = Command::new("pnpm")
		.arg("node")
		.arg(&script_path)
		.arg("render-fixture")
		.arg("--fixture-dir")
		.arg(&fixture)
		.arg("--output")
		.arg(&output_path)
		.output()
		.unwrap_or_else(|error| panic!("render benchmark comment: {error}"));

	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let rendered = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read rendered benchmark comment: {error}"));
	assert_snapshot!(rendered);
}
