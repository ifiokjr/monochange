use std::fs;
use std::path::Path;
use std::path::PathBuf;

#[test]
fn checked_in_snapshots_do_not_embed_escaped_newlines() {
	let workspace_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let mut unreadable_snapshots = Vec::new();

	collect_unreadable_snapshots(&workspace_root, &workspace_root, &mut unreadable_snapshots);

	assert!(
		unreadable_snapshots.is_empty(),
		"snapshots must not embed escaped newline sequences in their body; redact multiline JSON fields and add separate string snapshots instead:\n{}",
		unreadable_snapshots
			.iter()
			.map(|path| format!("- {}", path.display()))
			.collect::<Vec<_>>()
			.join("\n")
	);
}

fn collect_unreadable_snapshots(
	root: &Path,
	directory: &Path,
	unreadable_snapshots: &mut Vec<PathBuf>,
) {
	for entry in fs::read_dir(directory)
		.unwrap_or_else(|error| panic!("read directory {}: {error}", directory.display()))
	{
		let entry =
			entry.unwrap_or_else(|error| panic!("read entry in {}: {error}", directory.display()));
		let path = entry.path();

		if path.is_dir() {
			if should_skip_directory(&path) {
				continue;
			}
			collect_unreadable_snapshots(root, &path, unreadable_snapshots);
			continue;
		}

		if path
			.extension()
			.is_some_and(|extension| extension == "snap")
			&& snapshot_body_contains_escaped_newline(&path)
		{
			unreadable_snapshots.push(
				path.strip_prefix(root)
					.unwrap_or_else(|error| {
						panic!(
							"strip root {} from {}: {error}",
							root.display(),
							path.display()
						)
					})
					.to_path_buf(),
			);
		}
	}
}

fn should_skip_directory(path: &Path) -> bool {
	path.file_name().is_some_and(|name| {
		matches!(
			name.to_string_lossy().as_ref(),
			".devenv" | ".direnv" | ".git" | "target" | "worktrees"
		)
	})
}

fn snapshot_body_contains_escaped_newline(path: &Path) -> bool {
	let contents = fs::read_to_string(path)
		.unwrap_or_else(|error| panic!("read snapshot {}: {error}", path.display()));
	let body = contents
		.splitn(3, "---\n")
		.nth(2)
		.unwrap_or(contents.as_str());
	body.contains(r"\n")
}
