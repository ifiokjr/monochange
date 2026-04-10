use std::fs;
use std::path::Path;

use criterion::criterion_group;
use criterion::criterion_main;
use criterion::BenchmarkId;
use criterion::Criterion;

/// Generate a workspace fixture with N cargo packages and M changesets.
fn generate_fixture(root: &Path, num_packages: usize, num_changesets: usize) {
	let mut workspace_members = String::from("[workspace]\nmembers = [\n");
	for i in 0..num_packages {
		workspace_members.push_str(&format!("  \"crates/pkg-{i}\",\n"));
	}
	workspace_members.push_str("]\nresolver = \"2\"\n");
	fs::write(root.join("Cargo.toml"), &workspace_members).unwrap();

	let mut config = String::from("[defaults]\npackage_type = \"cargo\"\n\n");
	for i in 0..num_packages {
		config.push_str(&format!("[package.pkg-{i}]\npath = \"crates/pkg-{i}\"\n\n"));
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
				let changeset_paths: Vec<_> = std::fs::read_dir(&changeset_dir)
					.unwrap()
					.filter_map(Result::ok)
					.map(|e| e.path())
					.filter(|p| p.extension().map_or(false, |ext| ext == "md"))
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

fn bench_release_planning(c: &mut Criterion) {
	let mut group = c.benchmark_group("release_planning");
	group.sample_size(10);

	for &(packages, changesets) in SCALES {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("plan_release", &label),
			&(packages, changesets),
			|b, &(packages, changesets)| {
				let tempdir = tempfile::tempdir().unwrap();
				generate_fixture(tempdir.path(), packages, changesets);
				let changes_dir = tempdir.path().join(".changeset");
				let changeset_paths: Vec<_> = std::fs::read_dir(&changes_dir)
					.unwrap()
					.filter_map(Result::ok)
					.map(|e| e.path())
					.filter(|p| p.extension().map_or(false, |ext| ext == "md"))
					.collect();
				// Load once outside the benchmark loop.
				let configuration =
					monochange_config::load_workspace_configuration(tempdir.path()).unwrap();
				let discovery = monochange::discover_workspace(tempdir.path()).unwrap();
				let change_signals: Vec<_> = changeset_paths
					.iter()
					.flat_map(|path| {
						monochange_config::load_changeset_file(
							path,
							&configuration,
							&discovery.packages,
						)
						.unwrap()
						.signals
					})
					.collect();
				b.iter(|| {
					monochange::plan_release(tempdir.path(), &changeset_paths.first().unwrap())
						.unwrap_or_else(|_| {
							// plan_release takes a single file; for benchmarking the
							// graph/planning separately we'd need a different API.
							// Fall back to plan from first changeset.
							panic!("plan_release benchmark failed")
						})
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

criterion_group!(
	benches,
	bench_config_load,
	bench_discover,
	bench_validate,
	bench_changeset_loading,
	bench_prepare_release_dry_run
);
criterion_main!(benches);
