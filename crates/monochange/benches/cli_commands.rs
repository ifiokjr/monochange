use std::fs;
use std::path::Path;

use criterion::criterion_group;
use criterion::criterion_main;
use criterion::BenchmarkId;
use criterion::Criterion;

/// Generate a workspace fixture with N cargo packages and M changesets.
fn generate_fixture(root: &Path, num_packages: usize, num_changesets: usize) {
	use std::fmt::Write;
	let mut workspace_members = String::from("[workspace]\nmembers = [\n");
	for i in 0..num_packages {
		let _ = writeln!(workspace_members, "  \"crates/pkg-{i}\",");
	}
	workspace_members.push_str("]\nresolver = \"2\"\n");
	fs::write(root.join("Cargo.toml"), &workspace_members).unwrap();

	let mut config = String::from("[defaults]\npackage_type = \"cargo\"\n\n");
	for i in 0..num_packages {
		let _ = write!(config, "[package.pkg-{i}]\npath = \"crates/pkg-{i}\"\n\n");
	}
	config.push_str("[ecosystems.cargo]\nenabled = true\n\n");
	config.push_str("[cli.validate]\n[[cli.validate.steps]]\ntype = \"Validate\"\n\n");
	config.push_str("[cli.discover]\n[[cli.discover.steps]]\ntype = \"Discover\"\n\n");
	config.push_str("[cli.release]\n[[cli.release.steps]]\ntype = \"PrepareRelease\"\n");
	fs::write(root.join("monochange.toml"), &config).unwrap();

	for i in 0..num_packages {
		let pkg_dir = root.join(format!("crates/pkg-{i}"));
		fs::create_dir_all(&pkg_dir).unwrap();
		fs::write(
			pkg_dir.join("Cargo.toml"),
			format!("[package]\nname = \"pkg-{i}\"\nversion = \"1.0.0\"\nedition = \"2021\"\n"),
		)
		.unwrap();
	}

	let changeset_dir = root.join(".changeset");
	fs::create_dir_all(&changeset_dir).unwrap();
	for i in 0..num_changesets {
		let target_pkg = i % num_packages;
		fs::write(
			changeset_dir.join(format!("change-{i:04}.md")),
			format!("---\npkg-{target_pkg}: patch\n---\n\nFix issue #{i}.\n"),
		)
		.unwrap();
	}
}

const SCALES: &[(usize, usize)] = &[(5, 10), (20, 50), (50, 200)];

fn bench_config_load(c: &mut Criterion) {
	let mut group = c.benchmark_group("config_load");
	group.sample_size(10);

	for &(packages, changesets) in SCALES {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("load_workspace_configuration", &label),
			&(packages, changesets),
			|b, &(packages, changesets)| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture(tempdir.path(), packages, changesets);
				b.iter(|| monochange_config::load_workspace_configuration(tempdir.path()).unwrap());
			},
		);
	}
	group.finish();
}

fn bench_discover(c: &mut Criterion) {
	let mut group = c.benchmark_group("discover");
	group.sample_size(10);

	for &(packages, changesets) in SCALES {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("discover_workspace", &label),
			&(packages, changesets),
			|b, &(packages, changesets)| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture(tempdir.path(), packages, changesets);
				b.iter(|| monochange::discover_workspace(tempdir.path()).unwrap());
			},
		);
	}
	group.finish();
}

fn bench_validate(c: &mut Criterion) {
	let mut group = c.benchmark_group("validate");
	group.sample_size(10);

	for &(packages, changesets) in SCALES {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("validate_workspace", &label),
			&(packages, changesets),
			|b, &(packages, changesets)| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture(tempdir.path(), packages, changesets);
				b.iter(|| monochange_config::validate_workspace(tempdir.path()).unwrap());
			},
		);
	}
	group.finish();
}

fn bench_changeset_loading(c: &mut Criterion) {
	let mut group = c.benchmark_group("changeset_loading");
	group.sample_size(10);

	for &(packages, changesets) in SCALES {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("load_changesets", &label),
			&(packages, changesets),
			|b, &(packages, changesets)| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture(tempdir.path(), packages, changesets);
				let configuration =
					monochange_config::load_workspace_configuration(tempdir.path()).unwrap();
				let discovery = monochange::discover_workspace(tempdir.path()).unwrap();
				let changeset_dir = tempdir.path().join(".changeset");
				let changeset_paths: Vec<_> = fs::read_dir(&changeset_dir)
					.unwrap()
					.filter_map(Result::ok)
					.map(|e| e.path())
					.filter(|p| p.extension().is_some_and(|ext| ext == "md"))
					.collect();
				b.iter(|| {
					changeset_paths
						.iter()
						.map(|path| {
							monochange_config::load_changeset_file(
								path,
								&configuration,
								&discovery.packages,
							)
							.unwrap()
						})
						.collect::<Vec<_>>()
				});
			},
		);
	}
	group.finish();
}

fn bench_prepare_release_dry_run(c: &mut Criterion) {
	let mut group = c.benchmark_group("prepare_release_dry_run");
	group.sample_size(10);

	for &(packages, changesets) in SCALES {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("prepare_release", &label),
			&(packages, changesets),
			|b, &(packages, changesets)| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture(tempdir.path(), packages, changesets);
				b.iter(|| monochange::prepare_release(tempdir.path(), true).unwrap());
			},
		);
	}
	group.finish();
}

/// Generate a workspace with a real git history.
///
/// Creates `num_packages` packages, then builds git history by:
/// 1. Initial commit with all packages
/// 2. `num_history_commits` filler commits (modifying source files)
/// 3. Changesets added at evenly spaced intervals throughout the history
///
/// This tests whether `git log --follow --diff-filter=A` scales with
/// the depth of git history.
fn generate_fixture_with_git_history(
	root: &Path,
	num_packages: usize,
	num_changesets: usize,
	num_history_commits: usize,
) {
	use std::process::Command;

	// Create workspace structure first (no git yet).
	generate_fixture(root, num_packages, num_changesets);
	// Remove changesets — we'll add them interleaved with history.
	let changeset_dir = root.join(".changeset");
	if changeset_dir.exists() {
		fs::remove_dir_all(&changeset_dir).unwrap();
	}
	fs::create_dir_all(&changeset_dir).unwrap();

	// Init git repo.
	let git = |args: &[&str]| {
		let output = Command::new("git")
			.args(args)
			.current_dir(root)
			.env("GIT_AUTHOR_NAME", "bench")
			.env("GIT_AUTHOR_EMAIL", "bench@test")
			.env("GIT_COMMITTER_NAME", "bench")
			.env("GIT_COMMITTER_EMAIL", "bench@test")
			.output()
			.unwrap_or_else(|e| panic!("git {}: {e}", args.join(" ")));
		assert!(
			output.status.success(),
			"git {} failed: {}",
			args.join(" "),
			String::from_utf8_lossy(&output.stderr)
		);
	};

	git(&["init", "-b", "main"]);
	git(&["add", "."]);
	git(&["commit", "-m", "initial"]);

	// Calculate where to insert changesets in the history.
	let changeset_interval = if num_changesets > 0 {
		num_history_commits.max(1) / num_changesets.max(1)
	} else {
		usize::MAX
	};
	let mut changesets_added = 0;

	for commit_idx in 0..num_history_commits {
		// Add a filler commit (modify a source file).
		let pkg_idx = commit_idx % num_packages;
		let src_file = root.join(format!("crates/pkg-{pkg_idx}/src.rs"));
		fs::write(&src_file, format!("// commit {commit_idx}\n")).unwrap();
		git(&["add", "."]);
		git(&["commit", "-m", &format!("commit {commit_idx}")]);

		// Interleave changeset additions at regular intervals.
		if changesets_added < num_changesets
			&& (changeset_interval == 0 || commit_idx % changeset_interval.max(1) == 0)
		{
			let target_pkg = changesets_added % num_packages;
			let cs_path = changeset_dir.join(format!("change-{changesets_added:04}.md"));
			fs::write(
				&cs_path,
				format!("---\npkg-{target_pkg}: patch\n---\n\nFix issue #{changesets_added}.\n"),
			)
			.unwrap();
			git(&["add", "."]);
			git(&["commit", "-m", &format!("changeset {changesets_added}")]);
			changesets_added += 1;
		}
	}

	// Add any remaining changesets.
	while changesets_added < num_changesets {
		let target_pkg = changesets_added % num_packages;
		let cs_path = changeset_dir.join(format!("change-{changesets_added:04}.md"));
		fs::write(
			&cs_path,
			format!("---\npkg-{target_pkg}: patch\n---\n\nFix issue #{changesets_added}.\n"),
		)
		.unwrap();
		git(&["add", "."]);
		git(&["commit", "-m", &format!("changeset {changesets_added}")]);
		changesets_added += 1;
	}
}

fn bench_prepare_release_with_git_history(c: &mut Criterion) {
	let mut group = c.benchmark_group("prepare_release_git_history");
	group.sample_size(10);

	// Test with 10 packages, 20 changesets, varying history depth.
	for &history_commits in &[10, 100, 200] {
		let label = format!("10pkg_20cs_{history_commits}commits");
		group.bench_with_input(
			BenchmarkId::new("prepare_release", &label),
			&history_commits,
			|b, &history_commits| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture_with_git_history(tempdir.path(), 10, 20, history_commits);
				b.iter(|| monochange::prepare_release(tempdir.path(), true).unwrap());
			},
		);
	}
	group.finish();
}

criterion_group!(
	benches,
	bench_config_load,
	bench_discover,
	bench_validate,
	bench_changeset_loading,
	bench_prepare_release_dry_run,
	bench_prepare_release_with_git_history,
);
criterion_main!(benches);
