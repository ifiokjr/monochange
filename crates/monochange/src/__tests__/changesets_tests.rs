#![allow(clippy::disallowed_methods)]
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::BumpSeverity;
use monochange_core::ChangesetTargetKind;

use super::batch_git_log;
use super::diagnose_changesets;
use super::parse_batch_git_log_bytes;
use super::parse_batch_git_log_output;

fn setup_fixture(relative: &str) -> tempfile::TempDir {
	monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnose_changesets_loads_multiple_files_with_shared_context() {
	let fixture = setup_fixture("monochange/changeset-policy-base");
	fs::create_dir_all(fixture.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::write(
		fixture.path().join(".changeset/bug-fix.md"),
		"---\ncore: patch\n---\n\nFix a bug.\n",
	)
	.unwrap_or_else(|error| panic!("write bug fix changeset: {error}"));
	fs::write(
		fixture.path().join(".changeset/feature.md"),
		"---\ncore: minor\n---\n\nAdd a feature.\n",
	)
	.unwrap_or_else(|error| panic!("write feature changeset: {error}"));

	let report = diagnose_changesets(fixture.path(), &[])
		.await
		.unwrap_or_else(|error| panic!("diagnose changesets: {error}"));

	assert_eq!(
		report.requested_changesets,
		vec![
			PathBuf::from(".changeset/bug-fix.md"),
			PathBuf::from(".changeset/feature.md")
		]
	);
	assert_eq!(report.changesets.len(), 2);
	assert!(report.changesets.iter().all(|changeset| {
		changeset
			.targets
			.iter()
			.any(|target| target.id == "core" && target.kind == ChangesetTargetKind::Package)
	}));
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnose_changesets_uses_configuration_index_before_workspace_discovery() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(tempdir.path().join("crates/core/Cargo.toml"), "not toml\n")
		.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(
		tempdir.path().join(".changeset/core.md"),
		"---\ncore: patch\n---\n\nFix core.\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\n\
		package_type = \"cargo\"\n\
		\n\
		[package.core]\n\
		path = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let report = diagnose_changesets(tempdir.path(), &[])
		.await
		.unwrap_or_else(|error| panic!("diagnose changesets: {error}"));

	assert_eq!(report.changesets.len(), 1);
	assert!(
		report.changesets[0]
			.targets
			.iter()
			.any(|target| { target.id == "core" && target.kind == ChangesetTargetKind::Package })
	);
}

#[tokio::test(flavor = "multi_thread")]
async fn diagnose_changesets_falls_back_to_workspace_versions_for_explicit_versions() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset directory: {error}"));
	fs::create_dir_all(tempdir.path().join("crates/core/src"))
		.unwrap_or_else(|error| panic!("create source tree: {error}"));
	fs::write(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"real-core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	)
	.unwrap_or_else(|error| panic!("write package manifest: {error}"));
	fs::write(
		tempdir.path().join("crates/core/src/lib.rs"),
		"pub fn core() {}\n",
	)
	.unwrap_or_else(|error| panic!("write source file: {error}"));
	fs::write(
		tempdir.path().join(".changeset/core.md"),
		"---\ncore:\n  version: 1.2.0\n---\n\nPin core.\n",
	)
	.unwrap_or_else(|error| panic!("write changeset: {error}"));
	fs::write(
		tempdir.path().join("monochange.toml"),
		"[defaults]\n\
		package_type = \"cargo\"\n\
		\n\
		[package.core]\n\
		path = \"crates/core\"\n",
	)
	.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

	let report = diagnose_changesets(tempdir.path(), &[])
		.await
		.unwrap_or_else(|error| panic!("diagnose changesets: {error}"));
	let target = report.changesets[0]
		.targets
		.iter()
		.find(|target| target.id == "core")
		.unwrap_or_else(|| panic!("expected core target"));
	assert_eq!(target.bump, Some(BumpSeverity::Minor));
}

#[test]
fn batch_git_log_returns_empty_maps_for_empty_paths() {
	let (introduced, last_updated) = batch_git_log(Path::new("."), &[]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn batch_git_log_returns_empty_maps_when_git_log_fails() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let (introduced, last_updated) =
		batch_git_log(tempdir.path(), &[PathBuf::from(".changeset/feature.md")]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn parse_batch_git_log_bytes_returns_empty_maps_for_invalid_utf8_output() {
	let (introduced, last_updated) =
		parse_batch_git_log_bytes(b"\xff", &[PathBuf::from(".changeset/feature.md")]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}

#[test]
fn parse_batch_git_log_output_ignores_malformed_name_status_lines() {
	let (introduced, last_updated) = parse_batch_git_log_output(
		"abc123\x1fIfiok\x1fifiok@example.com\x1f2026-04-06T00:00:00Z\x1f2026-04-06T00:00:00Z\nM\n",
		&[PathBuf::from(".changeset/feature.md")],
	);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}
