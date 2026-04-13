use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use monochange_core::LockfileCommandExecution;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;

fn root_relative(root: &Path, path: &Path) -> PathBuf {
	let relative =
		monochange_core::relative_to_root(root, path).unwrap_or_else(|| path.to_path_buf());
	if relative.as_os_str().is_empty() {
		PathBuf::from(".")
	} else {
		relative
	}
}

pub fn remap_workspace_path(
	root: &Path,
	temp_root: &Path,
	path: &Path,
) -> MonochangeResult<PathBuf> {
	let normalized_root = monochange_core::normalize_path(root);
	let normalized_path = monochange_core::normalize_path(path);
	let relative = normalized_path
		.strip_prefix(&normalized_root)
		.map_err(|error| {
			MonochangeError::Config(format!(
				"path `{}` was outside workspace root `{}`: {error}",
				path.display(),
				root.display(),
			))
		})?;
	Ok(temp_root.join(relative))
}

pub fn run_lockfile_command(
	root: &Path,
	temp_root: &Path,
	command: &LockfileCommandExecution,
) -> MonochangeResult<()> {
	let cwd = remap_workspace_path(root, temp_root, &command.cwd)?;
	let output = if let Some(shell_binary) = command.shell.shell_binary() {
		Command::new(shell_binary)
			.arg("-c")
			.arg(&command.command)
			.current_dir(&cwd)
			.output()
	} else {
		let parts = shlex::split(&command.command).ok_or_else(|| {
			MonochangeError::Config(format!("failed to parse command `{}`", command.command))
		})?;
		let Some((program, args)) = parts.split_first() else {
			return Err(MonochangeError::Config(
				"lockfile command must not be empty".to_string(),
			));
		};
		Command::new(program).args(args).current_dir(&cwd).output()
	};
	let output = output.map_err(|error| {
		MonochangeError::Io(format!(
			"failed to run lockfile command `{}` in {}: {error}",
			command.command,
			root_relative(root, &command.cwd).display(),
		))
	})?;
	if output.status.success() {
		return Ok(());
	}
	let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
	let details = if stderr.is_empty() {
		format!("exit status {}", output.status)
	} else {
		stderr
	};
	Err(MonochangeError::Discovery(format!(
		"lockfile command `{}` failed in {}: {details}",
		command.command,
		root_relative(root, &command.cwd).display(),
	)))
}

pub fn read_optional_file(path: &Path) -> MonochangeResult<Option<Vec<u8>>> {
	match fs::read(path) {
		Ok(contents) => Ok(Some(contents)),
		Err(error)
			if matches!(
				error.kind(),
				std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
			) =>
		{
			Ok(None)
		}
		Err(error) => {
			Err(MonochangeError::Io(format!(
				"failed to read {}: {error}",
				path.display()
			)))
		}
	}
}

pub fn entry_file_type(entry: &fs::DirEntry, path: &Path) -> MonochangeResult<fs::FileType> {
	entry
		.file_type()
		.map_err(|error| MonochangeError::Io(format!("failed to stat {}: {error}", path.display())))
}

pub fn strip_workspace_prefix<'a>(path: &'a Path, root: &Path) -> MonochangeResult<&'a Path> {
	path.strip_prefix(root).map_err(|error| {
		MonochangeError::Config(format!(
			"path `{}` was outside workspace root `{}`: {error}",
			path.display(),
			root.display()
		))
	})
}

pub fn ensure_parent_directory(path: &Path) -> MonochangeResult<()> {
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}
	Ok(())
}

pub fn copy_workspace_file(source: &Path, destination: &Path) -> MonochangeResult<()> {
	fs::copy(source, destination).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to copy {} to {}: {error}",
			source.display(),
			destination.display()
		))
	})?;
	Ok(())
}

pub fn collect_workspace_files(
	root: &Path,
	current: &Path,
	relative_paths: &mut BTreeSet<PathBuf>,
) -> MonochangeResult<()> {
	for entry in fs::read_dir(current).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", current.display()))
	})? {
		let entry = entry
			.map_err(|error| MonochangeError::Io(format!("directory entry error: {error}")))?;
		let path = entry.path();
		if path.file_name().is_some_and(|name| name == ".git") {
			continue;
		}
		let file_type = entry_file_type(&entry, &path)?;
		if file_type.is_dir() {
			collect_workspace_files(root, &path, relative_paths)?;
			continue;
		}
		if file_type.is_file() {
			relative_paths.insert(strip_workspace_prefix(&path, root)?.to_path_buf());
		}
	}
	Ok(())
}

pub fn copy_workspace_tree(source: &Path, destination: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(destination).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create {}: {error}",
			destination.display()
		))
	})?;
	for entry in fs::read_dir(source).map_err(|error| {
		MonochangeError::Io(format!("failed to read {}: {error}", source.display()))
	})? {
		let entry = entry
			.map_err(|error| MonochangeError::Io(format!("directory entry error: {error}")))?;
		let source_path = entry.path();
		if source_path.file_name().is_some_and(|name| name == ".git") {
			continue;
		}
		let destination_path = destination.join(entry.file_name());
		let file_type = entry_file_type(&entry, &source_path)?;
		if file_type.is_dir() {
			copy_workspace_tree(&source_path, &destination_path)?;
			continue;
		}
		if file_type.is_file() {
			ensure_parent_directory(&destination_path)?;
			copy_workspace_file(&source_path, &destination_path)?;
		}
	}
	Ok(())
}
