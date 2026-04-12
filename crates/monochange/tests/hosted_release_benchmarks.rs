use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use insta::assert_snapshot;

mod test_support;
use test_support::current_test_name;
use test_support::snapshot_settings;

fn repo_root() -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap_or_else(|error| panic!("repo root: {error}"))
}

fn write_executable(path: &Path, contents: &str) {
	let display = path.display();
	fs::write(path, contents).unwrap_or_else(|error| panic!("write {display}: {error}"));
	let mut permissions = fs::metadata(path)
		.unwrap_or_else(|error| panic!("metadata {display}: {error}"))
		.permissions();
	permissions.set_mode(0o755);
	fs::set_permissions(path, permissions)
		.unwrap_or_else(|error| panic!("chmod {display}: {error}"));
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.arg("-C")
		.arg(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {:?} failed: {}",
		args,
		String::from_utf8_lossy(&output.stderr)
	);
	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("utf8 git stdout: {error}"))
}

#[test]
fn hosted_fixture_setup_script_bootstraps_local_pr_history() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_dir = tempdir.path().join("fixture");
	let script_path = repo_root().join("scripts/setup_hosted_benchmark_fixture.sh");

	let output = Command::new("bash")
		.arg(&script_path)
		.arg("--local-only")
		.arg("--output-dir")
		.arg(&fixture_dir)
		.arg("--owner")
		.arg("fixture-owner")
		.arg("--repo")
		.arg("fixture-repo")
		.arg("--package-count")
		.arg("3")
		.arg("--filler-commits")
		.arg("12")
		.arg("--release-prs")
		.arg("2")
		.arg("--commits-per-pr")
		.arg("2")
		.output()
		.unwrap_or_else(|error| panic!("run fixture setup script: {error}"));

	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let monochange_toml = fs::read_to_string(fixture_dir.join("monochange.toml"))
		.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
	assert!(monochange_toml.contains("owner = \"fixture-owner\""));
	assert!(monochange_toml.contains("repo = \"fixture-repo\""));

	let commit_count = git_stdout(&fixture_dir, &["rev-list", "--count", "HEAD"])
		.trim()
		.parse::<usize>()
		.unwrap_or_else(|error| panic!("parse commit count: {error}"));
	assert_eq!(commit_count, 21);

	let merges = git_stdout(&fixture_dir, &["log", "--merges", "--oneline"]);
	assert_eq!(merges.lines().count(), 2);

	let changeset_count = fs::read_dir(fixture_dir.join(".changeset"))
		.unwrap_or_else(|error| panic!("read .changeset: {error}"))
		.filter_map(Result::ok)
		.filter(|entry| {
			entry
				.path()
				.extension()
				.is_some_and(|extension| extension == "md")
		})
		.count();
	assert_eq!(changeset_count, 2);
}

#[test]
fn benchmark_cli_run_fixture_supports_hosted_fixture_metadata() {
	let mut settings = snapshot_settings();
	settings.set_snapshot_suffix(current_test_name());
	let _guard = settings.bind_to_scope();

	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture_dir = tempdir.path().join("fixture");
	let setup_script = repo_root().join("scripts/setup_hosted_benchmark_fixture.sh");

	let setup_output = Command::new("bash")
		.arg(&setup_script)
		.arg("--local-only")
		.arg("--output-dir")
		.arg(&fixture_dir)
		.arg("--package-count")
		.arg("2")
		.arg("--filler-commits")
		.arg("3")
		.arg("--release-prs")
		.arg("1")
		.arg("--commits-per-pr")
		.arg("1")
		.output()
		.unwrap_or_else(|error| panic!("seed local hosted fixture: {error}"));
	assert!(
		setup_output.status.success(),
		"{}",
		String::from_utf8_lossy(&setup_output.stderr)
	);

	let bin_dir = tempdir.path().join("bin");
	fs::create_dir_all(&bin_dir).unwrap_or_else(|error| panic!("mkdir bin: {error}"));
	let fake_main = bin_dir.join("mc-main");
	let fake_pr = bin_dir.join("mc-pr");
	let fake_hyperfine = bin_dir.join("hyperfine");
	let output_path = tempdir.path().join("comment.md");
	let violations_path = tempdir.path().join("violations.txt");

	let fake_main_script = r#"#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "--help" ]; then
  echo "--progress-format"
  exit 0
fi
if [ "${1:-}" = "--progress-format" ]; then
  shift 2
fi
if [ "${1:-}" = "release" ]; then
  duration=150
  phase=80
  if [ "${2:-}" = "--dry-run" ]; then
    duration=120
    phase=60
  fi
  printf '{"event":"step_finished","stepKind":"PrepareRelease","durationMs":%s,"phaseTimings":[{"label":"enrich changeset context via github","durationMs":%s}]}\n' "$duration" "$phase" >&2
fi
"#;
	let fake_pr_script = r#"#!/usr/bin/env bash
set -euo pipefail
if [ "${1:-}" = "--help" ]; then
  echo "--progress-format"
  exit 0
fi
if [ "${1:-}" = "--progress-format" ]; then
  shift 2
fi
if [ "${1:-}" = "release" ]; then
  duration=140
  phase=70
  if [ "${2:-}" = "--dry-run" ]; then
    duration=110
    phase=55
  fi
  printf '{"event":"step_finished","stepKind":"PrepareRelease","durationMs":%s,"phaseTimings":[{"label":"enrich changeset context via github","durationMs":%s}]}\n' "$duration" "$phase" >&2
fi
"#;
	let fake_hyperfine_script = r#"#!/usr/bin/env bash
set -euo pipefail
export_markdown=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --export-markdown)
      export_markdown="$2"
      shift 2
      ;;
    *)
      shift
      ;;
  esac
done
cat >"$export_markdown" <<'EOF'
| Command | Mean [ms] | Min [ms] | Max [ms] |
|:---|---:|---:|---:|
| main · mc validate | 10.0 | 9.0 | 11.0 |
| pr · mc validate | 11.0 | 10.0 | 12.0 |
| main · mc discover --format json | 12.0 | 11.0 | 13.0 |
| pr · mc discover --format json | 13.0 | 12.0 | 14.0 |
| main · mc release --dry-run | 50.0 | 49.0 | 51.0 |
| pr · mc release --dry-run | 48.0 | 47.0 | 49.0 |
| main · mc release | 60.0 | 59.0 | 61.0 |
| pr · mc release | 57.0 | 56.0 | 58.0 |
EOF
"#;
	write_executable(&fake_main, fake_main_script);
	write_executable(&fake_pr, fake_pr_script);
	write_executable(&fake_hyperfine, fake_hyperfine_script);

	let benchmark_script = repo_root().join(".github/scripts/benchmark_cli.sh");
	let output = Command::new("bash")
		.arg(&benchmark_script)
		.arg("run-fixture")
		.arg("--main-bin")
		.arg(&fake_main)
		.arg("--pr-bin")
		.arg(&fake_pr)
		.arg("--fixture-dir")
		.arg(&fixture_dir)
		.arg("--scenario-id")
		.arg("hosted_github")
		.arg("--scenario-name")
		.arg("Hosted GitHub fixture")
		.arg("--scenario-description")
		.arg("2 packages, local clone of the hosted fixture repo")
		.arg("--output")
		.arg(&output_path)
		.arg("--violations-output")
		.arg(&violations_path)
		.env("MONOCHANGE_HYPERFINE_BIN", &fake_hyperfine)
		.output()
		.unwrap_or_else(|error| panic!("run benchmark fixture mode: {error}"));

	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);

	let rendered = fs::read_to_string(&output_path)
		.unwrap_or_else(|error| panic!("read hosted benchmark comment: {error}"));
	assert_snapshot!(rendered);
	assert_eq!(
		fs::read_to_string(&violations_path)
			.unwrap_or_else(|error| panic!("read violations: {error}"))
			.trim(),
		"0"
	);
}
