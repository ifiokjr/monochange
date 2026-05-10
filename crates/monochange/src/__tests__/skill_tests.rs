#![allow(clippy::disallowed_methods)]
use std::fs;
use std::path::PathBuf;

use tempfile::tempdir;

use super::*;

fn fake_path(bin_dir: &Path) -> std::ffi::OsString {
	env::join_paths([PathBuf::from(bin_dir)])
		.unwrap_or_else(|error| panic!("join fake PATH {}: {error}", bin_dir.display()))
}

#[test]
fn skill_runner_detection_prefers_npx_then_pnpm_then_bunx() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let bin_dir = tempdir.path().join("bin");
	fs::create_dir_all(&bin_dir)
		.unwrap_or_else(|error| panic!("create fake bin dir {}: {error}", bin_dir.display()));

	let npx = bin_dir.join("npx");
	let pnpm = bin_dir.join("pnpm");
	let bunx = bin_dir.join("bunx");
	for path in [&npx, &pnpm, &bunx] {
		fs::write(path, "#!/bin/sh\nexit 0\n")
			.unwrap_or_else(|error| panic!("write fake tool {}: {error}", path.display()));
	}

	let path = fake_path(&bin_dir);
	assert_eq!(
		SkillRunner::detect_with(Some(path.as_os_str()), None)
			.unwrap_or_else(|error| panic!("detect npx runner: {error}")),
		SkillRunner::Npx
	);

	fs::remove_file(&npx).unwrap_or_else(|error| panic!("remove fake npx: {error}"));
	let path = fake_path(&bin_dir);
	assert_eq!(
		SkillRunner::detect_with(Some(path.as_os_str()), None)
			.unwrap_or_else(|error| panic!("detect pnpm runner: {error}")),
		SkillRunner::Pnpm
	);

	fs::remove_file(&pnpm).unwrap_or_else(|error| panic!("remove fake pnpm: {error}"));
	let path = fake_path(&bin_dir);
	assert_eq!(
		SkillRunner::detect_with(Some(path.as_os_str()), None)
			.unwrap_or_else(|error| panic!("detect bunx runner: {error}")),
		SkillRunner::Bunx
	);
}

#[test]
fn skill_runner_detection_reports_missing_launchers() {
	let error = SkillRunner::detect_with(None, None)
		.err()
		.unwrap_or_else(|| panic!("expected missing launcher error"));
	assert!(
		error
			.to_string()
			.contains("expected one of `npx`, `pnpm`, or `bunx` in PATH")
	);
}

#[test]
fn skill_runner_override_supports_supported_values_and_rejects_invalid_values() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let bin_dir = tempdir.path().join("bin");
	fs::create_dir_all(&bin_dir)
		.unwrap_or_else(|error| panic!("create fake bin dir {}: {error}", bin_dir.display()));
	let pnpm = bin_dir.join("pnpm");
	fs::write(&pnpm, "#!/bin/sh\nexit 0\n")
		.unwrap_or_else(|error| panic!("write fake pnpm {}: {error}", pnpm.display()));
	let path = fake_path(&bin_dir);
	assert_eq!(
		SkillRunner::detect_with(Some(path.as_os_str()), Some("pnpm"))
			.unwrap_or_else(|error| panic!("detect overridden pnpm runner: {error}")),
		SkillRunner::Pnpm
	);

	let missing_runner_error = SkillRunner::detect_with(Some(path.as_os_str()), Some("npx"))
		.err()
		.unwrap_or_else(|| panic!("expected missing overridden runner error"));
	assert!(
		missing_runner_error
			.to_string()
			.contains("configured skill runner `npx` was not found in PATH")
	);

	let invalid_error = SkillRunner::detect_with(Some(path.as_os_str()), Some("nope"))
		.err()
		.unwrap_or_else(|| panic!("expected invalid runner override error"));
	assert!(
		invalid_error
			.to_string()
			.contains("unsupported skill runner `nope`")
	);
}

#[test]
fn skill_runner_render_command_matches_supported_launchers() {
	let forwarded_args = vec!["-g".to_string(), "-y".to_string()];
	assert_eq!(
		SkillRunner::Npx.render_command("/tmp/source", &forwarded_args),
		"npx -y skills add /tmp/source -g -y"
	);
	assert_eq!(
		SkillRunner::Pnpm.render_command("/tmp/source", &forwarded_args),
		"pnpm dlx skills add /tmp/source -g -y"
	);
	assert_eq!(
		SkillRunner::Bunx.render_command("/tmp/source", &forwarded_args),
		"bunx skills add /tmp/source -g -y"
	);
}

#[test]
fn skill_source_prefers_env_override() {
	temp_env::with_var(
		"MONOCHANGE_SKILL_SOURCE",
		Some("./fixtures/skill-source"),
		|| {
			assert_eq!(skill_source(), "./fixtures/skill-source");
		},
	);
	assert_eq!(skill_source(), DEFAULT_SKILL_SOURCE);
}

#[test]
fn run_skill_reports_spawn_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing_root = tempdir.path().join("missing-root");
	let source_dir = tempdir.path().join("skill-source");
	assert!(
		fs::create_dir_all(&source_dir).is_ok(),
		"create fake skill source dir {}",
		source_dir.display()
	);

	let rendered = SkillRunner::Npx.render_command(&source_dir.to_string_lossy(), &[]);
	let error = run_skill_with(
		&missing_root,
		&SkillOptions::default(),
		SkillRunner::Npx,
		&source_dir.to_string_lossy(),
		&rendered,
	)
	.expect_err("expected spawn failure error");
	assert!(
		error
			.to_string()
			.contains("failed to run `npx -y skills add")
	);
	assert!(
		error
			.to_string()
			.contains(&missing_root.display().to_string())
	);
}
