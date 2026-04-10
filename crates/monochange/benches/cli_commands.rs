use std::fs;
use std::path::Path;

use criterion::criterion_group;
use criterion::criterion_main;
use criterion::BenchmarkId;
use criterion::Criterion;

/// Generate a workspace fixture with N cargo packages and M changesets.
fn generate_fixture(root: &Path, num_packages: usize, num_changesets: usize) {
	// Create a Cargo workspace so discovery finds packages correctly.
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

fn bench_discover(c: &mut Criterion) {
	let mut group = c.benchmark_group("discover");
	group.sample_size(10);

	for &(packages, changesets) in &[(5, 10), (20, 50), (50, 200)] {
		let label = format!("{packages}pkg_{changesets}cs");
		group.bench_with_input(
			BenchmarkId::new("discover", &label),
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

fn bench_prepare_release_dry_run(c: &mut Criterion) {
	let mut group = c.benchmark_group("prepare_release_dry_run");
	group.sample_size(10);

	for &(packages, changesets) in &[(5, 10), (20, 50), (50, 200)] {
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

criterion_group!(benches, bench_discover, bench_prepare_release_dry_run);
criterion_main!(benches);
