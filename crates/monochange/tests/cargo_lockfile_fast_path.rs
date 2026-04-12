use std::env;
use std::ffi::OsString;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

mod test_support;
use test_support::monochange_command;
use test_support::setup_fixture;

#[cfg(unix)]
fn make_executable(path: &Path) {
	let metadata =
		fs::metadata(path).unwrap_or_else(|error| panic!("metadata {}: {error}", path.display()));
	let mut permissions = metadata.permissions();
	permissions.set_mode(0o755);
	fs::set_permissions(path, permissions)
		.unwrap_or_else(|error| panic!("set permissions {}: {error}", path.display()));
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) {}

fn prefixed_path(bin_dir: &Path) -> OsString {
	let existing = env::var_os("PATH").unwrap_or_default();
	let mut combined = env::split_paths(&existing).collect::<Vec<_>>();
	combined.insert(0, bin_dir.to_path_buf());
	env::join_paths(combined).unwrap_or_else(|error| panic!("join PATH entries: {error}"))
}

#[test]
fn release_keeps_cargo_lockfile_on_direct_rewrite_fast_path() {
	let tempdir = setup_fixture("monochange/cargo-lock-release");
	let root = tempdir.path();
	let manifest_path = root.join("crates/core/Cargo.toml");
	let manifest = fs::read_to_string(&manifest_path)
		.unwrap_or_else(|error| panic!("read Cargo.toml: {error}"));
	fs::write(
		&manifest_path,
		format!("{manifest}\n[dependencies]\nserde = \"1.0\"\n"),
	)
	.unwrap_or_else(|error| panic!("write Cargo.toml: {error}"));

	let fake_cargo = root.join("tools/bin/cargo");
	fs::write(
		&fake_cargo,
		"#!/bin/sh\nset -eu\ntouch cargo-invoked.txt\nprintf 'cargo should not run on the default fast path\\n' >&2\nexit 99\n",
	)
	.unwrap_or_else(|error| panic!("write fake cargo: {error}"));
	make_executable(&fake_cargo);

	let output = monochange_command(Some("2026-04-06"))
		.current_dir(root)
		.env("PATH", prefixed_path(&root.join("tools/bin")))
		.arg("release")
		.output()
		.unwrap_or_else(|error| panic!("release command: {error}"));

	assert!(
		output.status.success(),
		"release failed: {}",
		String::from_utf8_lossy(&output.stderr)
	);
	let stderr =
		String::from_utf8(output.stderr).unwrap_or_else(|error| panic!("stderr utf8: {error}"));
	assert!(
		stderr.contains("still looks incomplete after monochange rewrote it directly"),
		"expected manual refresh warning, got: {stderr}"
	);
	assert!(
		stderr.contains("cargo generate-lockfile") && stderr.contains("cargo check"),
		"expected refresh guidance in warning, got: {stderr}"
	);
	assert!(
		!root.join("cargo-invoked.txt").exists(),
		"default release path should not invoke cargo"
	);
	assert!(
		fs::read_to_string(root.join("Cargo.lock"))
			.unwrap_or_else(|error| panic!("read Cargo.lock: {error}"))
			.contains("version = \"1.1.0\""),
		"expected direct lockfile rewrite to update Cargo.lock"
	);
}
