use super::*;
use crate::PackageRecord;
use crate::PublishState;

#[test]
fn package_snapshot_file_lookup_finds_matching_paths() {
	let snapshot = PackageSnapshot {
		label: "after".to_string(),
		files: vec![PackageSnapshotFile {
			path: PathBuf::from("src/lib.rs"),
			contents: "pub fn greet() {}".to_string(),
		}],
	};

	let file = snapshot
		.file(Path::new("src/lib.rs"))
		.unwrap_or_else(|| panic!("expected file in snapshot"));
	assert_eq!(file.contents, "pub fn greet() {}");
}

#[test]
fn package_analysis_context_exposes_package_root() {
	let package = PackageRecord::new(
		Ecosystem::Cargo,
		"core",
		PathBuf::from("/repo/crates/core/Cargo.toml"),
		PathBuf::from("/repo"),
		None,
		PublishState::Public,
	);
	let context = PackageAnalysisContext {
		repo_root: Path::new("/repo"),
		package: &package,
		detection_level: DetectionLevel::Signature,
		changed_files: &[],
		before_snapshot: None,
		after_snapshot: None,
	};

	assert_eq!(context.package_root(), Path::new("/repo/crates/core"));
}
