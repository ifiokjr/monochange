use std::fs;
use std::path::Path;
use std::process::Command;

use insta_cmd::get_cargo_bin;
use serde_json::Value;
use tempfile::tempdir;

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

#[test]
fn release_manifest_records_git_changeset_provenance_and_renders_provenance_templates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_git_release_fixture(tempdir.path());

	run_git(tempdir.path(), &["init"]);
	run_git(tempdir.path(), &["config", "user.name", "MonoChange Tests"]);
	run_git(
		tempdir.path(),
		&["config", "user.email", "monochange-tests@example.com"],
	);
	run_git(tempdir.path(), &["add", "."]);
	run_git(
		tempdir.path(),
		&["commit", "-m", "chore: seed release fixture"],
	);

	fs::write(
		tempdir.path().join(".changeset/feature.md"),
		r"---
core: minor
---

#### add provenance context
",
	)
	.unwrap_or_else(|error| panic!("changeset write: {error}"));
	run_git(tempdir.path(), &["add", ".changeset/feature.md"]);
	run_git(tempdir.path(), &["commit", "-m", "feat: add changeset"]);
	let introduced_sha = git_stdout(tempdir.path(), &["rev-parse", "HEAD"])
		.trim()
		.to_string();

	fs::write(
		tempdir.path().join(".changeset/feature.md"),
		r"---
core: minor
---

#### add provenance context

Track the commit history in release notes.
",
	)
	.unwrap_or_else(|error| panic!("changeset update: {error}"));
	run_git(tempdir.path(), &["add", ".changeset/feature.md"]);
	run_git(
		tempdir.path(),
		&["commit", "-m", "docs: refine changeset details"],
	);
	let updated_sha = git_stdout(tempdir.path(), &["rev-parse", "HEAD"])
		.trim()
		.to_string();

	let output = cli()
		.current_dir(tempdir.path())
		.arg("release-manifest")
		.arg("--dry-run")
		.output()
		.unwrap_or_else(|error| panic!("release-manifest: {error}"));
	assert!(
		output.status.success(),
		"release-manifest failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);

	let manifest_path = tempdir.path().join(".monochange/release-manifest.json");
	let manifest = fs::read_to_string(&manifest_path)
		.unwrap_or_else(|error| panic!("read manifest {}: {error}", manifest_path.display()));
	let parsed: Value =
		serde_json::from_str(&manifest).unwrap_or_else(|error| panic!("manifest json: {error}"));

	assert_eq!(
		parsed["changesets"][0]["path"].as_str(),
		Some(".changeset/feature.md")
	);
	assert_eq!(
		parsed["changesets"][0]["provenance"]["provider"].as_str(),
		Some("generic_git")
	);
	assert_eq!(
		parsed["changesets"][0]["provenance"]["introduced"]["commit"]["shortSha"].as_str(),
		Some(&introduced_sha[..7])
	);
	assert_eq!(
		parsed["changesets"][0]["provenance"]["lastUpdated"]["commit"]["shortSha"].as_str(),
		Some(&updated_sha[..7])
	);
	let rendered = parsed["changelogs"][0]["rendered"]
		.as_str()
		.unwrap_or_else(|| panic!("expected rendered changelog"));
	assert!(rendered.contains("> Changeset: `.changeset/feature.md`"));
	assert!(rendered.contains(&introduced_sha[..7]));
	assert!(rendered.contains(&updated_sha[..7]));
}

fn seed_git_release_fixture(root: &Path) {
	fs::create_dir_all(root.join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create core dir: {error}"));
	fs::create_dir_all(root.join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::create_dir_all(root.join(".monochange"))
		.unwrap_or_else(|error| panic!("create monochange dir: {error}"));
	fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers = [\"crates/core\"]\nresolver = \"2\"\n",
	)
	.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	fs::write(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"workflow-core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("core manifest: {error}"));
	fs::write(
		root.join("crates/core/src/lib.rs"),
		"pub fn answer() -> u32 { 42 }\n",
	)
	.unwrap_or_else(|error| panic!("core lib: {error}"));
	fs::write(root.join("crates/core/CHANGELOG.md"), "# Changelog\n")
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	fs::write(
		root.join("monochange.toml"),
		r#####"
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{path}/CHANGELOG.md"
format = "monochange"

[release_notes]
change_templates = ["#### $summary\n\n$details\n\n$provenance", "#### $summary\n\n$provenance", "- $summary"]

[package.core]
path = "crates/core"

[cli.release-manifest]
help_text = "Prepare a release and write a stable JSON manifest"

[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"
"#####,
	)
	.unwrap_or_else(|error| panic!("monochange config: {error}"));
}

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: {}{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("utf8: {error}"))
}
