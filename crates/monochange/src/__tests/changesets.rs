use std::path::Path;
use std::path::PathBuf;

use super::batch_git_log;
use super::parse_batch_git_log_bytes;
use super::parse_batch_git_log_output;

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
