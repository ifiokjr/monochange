use std::env;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command as ProcessCommand;

use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

const DEFAULT_SKILL_SOURCE: &str =
	"https://github.com/ifiokjr/monochange/tree/main/packages/monochange__skill";
const SKILL_SOURCE_ENV_VAR: &str = "MONOCHANGE_SKILL_SOURCE";
const SKILL_RUNNER_ENV_VAR: &str = "MONOCHANGE_SKILL_RUNNER";

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SkillOptions {
	pub(crate) forwarded_args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SkillRunner {
	Npx,
	Pnpm,
	Bunx,
}

impl SkillRunner {
	fn detect() -> MonochangeResult<Self> {
		let path = env::var_os("PATH");
		let runner_override = env::var(SKILL_RUNNER_ENV_VAR).ok();
		Self::detect_with(path.as_deref(), runner_override.as_deref())
	}

	fn detect_with(path: Option<&OsStr>, runner_override: Option<&str>) -> MonochangeResult<Self> {
		if let Some(runner_override) = runner_override {
			let runner = Self::from_override_value(runner_override)?;
			return command_exists_in_path(path, runner.program())
				.then_some(runner)
				.ok_or_else(|| {
					MonochangeError::Config(format!(
						"configured skill runner `{runner_override}` was not found in PATH"
					))
				});
		}

		for runner in [Self::Npx, Self::Pnpm, Self::Bunx] {
			if command_exists_in_path(path, runner.program()) {
				return Ok(runner);
			}
		}

		Err(MonochangeError::Config(
			"expected one of `npx`, `pnpm`, or `bunx` in PATH to install @monochange/skill"
				.to_string(),
		))
	}

	fn from_override_value(value: &str) -> MonochangeResult<Self> {
		match value {
			"npx" => Ok(Self::Npx),
			"pnpm" => Ok(Self::Pnpm),
			"bunx" => Ok(Self::Bunx),
			_ => {
				Err(MonochangeError::Config(format!(
					"unsupported skill runner `{value}`; expected `npx`, `pnpm`, or `bunx`"
				)))
			}
		}
	}

	fn program(self) -> &'static str {
		match self {
			Self::Npx => "npx",
			Self::Pnpm => "pnpm",
			Self::Bunx => "bunx",
		}
	}

	fn render_command(self, source: &str, forwarded_args: &[String]) -> String {
		let mut args = match self {
			Self::Npx => vec!["npx", "-y", "skills", "add", source],
			Self::Pnpm => vec!["pnpm", "dlx", "skills", "add", source],
			Self::Bunx => vec!["bunx", "skills", "add", source],
		}
		.into_iter()
		.map(str::to_string)
		.collect::<Vec<_>>();
		args.extend(forwarded_args.iter().cloned());

		shlex::try_join(args.iter().map(String::as_str))
			.unwrap_or_else(|error| panic!("render skill install command as shell string: {error}"))
	}

	fn build_process_command(
		self,
		root: &Path,
		source: &str,
		forwarded_args: &[String],
	) -> ProcessCommand {
		let mut command = ProcessCommand::new(self.program());
		if self == Self::Npx {
			command.arg("-y");
		} else if self == Self::Pnpm {
			command.arg("dlx");
		}
		command.args(["skills", "add"]);
		command.current_dir(root);
		command.arg(source);
		command.args(forwarded_args);

		command
	}
}

pub(crate) fn run_skill(root: &Path, options: &SkillOptions) -> MonochangeResult<String> {
	let runner = SkillRunner::detect()?;
	let source = skill_source();
	let rendered = runner.render_command(&source, &options.forwarded_args);
	run_skill_with(root, options, runner, &source, &rendered)
}

fn run_skill_with(
	root: &Path,
	options: &SkillOptions,
	runner: SkillRunner,
	source: &str,
	rendered: &str,
) -> MonochangeResult<String> {
	let status = runner
		.build_process_command(root, source, &options.forwarded_args)
		.status()
		.map_err(|error| {
			MonochangeError::Io(format!(
				"failed to run `{rendered}` in {}: {error}",
				root.display()
			))
		})?;

	if !status.success() {
		return Err(MonochangeError::Config(format!(
			"`{rendered}` failed with {status}"
		)));
	}

	Ok(String::new())
}

fn skill_source() -> String {
	env::var(SKILL_SOURCE_ENV_VAR).unwrap_or_else(|_| DEFAULT_SKILL_SOURCE.to_string())
}

fn command_exists_in_path(path: Option<&OsStr>, program: &str) -> bool {
	let Some(path) = path else {
		return false;
	};

	env::split_paths(path).any(|dir| {
		executable_candidates(program)
			.into_iter()
			.any(|candidate| dir.join(candidate).is_file())
	})
}

#[cfg(windows)]
fn executable_candidates(program: &str) -> Vec<String> {
	if program.contains('.') {
		return vec![program.to_string()];
	}

	let pathext = env::var_os("PATHEXT")
		.unwrap_or_else(|| ".COM;.EXE;.BAT;.CMD".into())
		.to_string_lossy()
		.to_string();

	pathext
		.split(';')
		.filter(|extension| !extension.is_empty())
		.map(|extension| format!("{program}{extension}"))
		.collect()
}

#[cfg(not(windows))]
fn executable_candidates(program: &str) -> Vec<String> {
	vec![program.to_string()]
}

#[cfg(test)]
mod tests {
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
}
