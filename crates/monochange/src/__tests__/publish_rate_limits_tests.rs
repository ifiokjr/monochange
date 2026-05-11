pub(crate) fn plan_unbatched_publish_order_for_dependency_ordered_requests(
	requests: &[package_publish::PublishRequest],
	packages: &[monochange_core::PackageRecord],
	operation: RateLimitOperation,
	dry_run: bool,
) -> PublishRateLimitReport {
	let mut requests = requests.to_vec();
	sort_requests_by_dependencies(&mut requests, packages);
	plan_unbatched_publish_order_for_requests(&requests, operation, dry_run)
}

fn plan_unbatched_publish_order_for_requests(
	requests: &[package_publish::PublishRequest],
	operation: RateLimitOperation,
	dry_run: bool,
) -> PublishRateLimitReport {
	let policies = policies_for_operation(operation)
		.into_iter()
		.map(|policy| (policy.registry, policy))
		.collect::<BTreeMap<_, _>>();
	let mut requests_by_registry =
		BTreeMap::<RegistryKind, Vec<&package_publish::PublishRequest>>::new();
	for request in requests {
		if request.mode == PublishMode::External {
			continue;
		}
		requests_by_registry
			.entry(request.registry)
			.or_default()
			.push(request);
	}

	let mut batches = Vec::new();
	let mut windows = Vec::new();
	for (registry, requests) in requests_by_registry {
		let policy = policies
			.get(&registry)
			.unwrap_or_else(|| panic!("missing rate-limit policy for {registry}"));
		let pending = requests.len();
		windows.push(RegistryRateLimitWindowPlan {
			registry,
			operation,
			limit: None,
			window_seconds: None,
			pending,
			batches_required: 1,
			fits_single_window: true,
			confidence: policy.confidence,
			notes: "rate-limit batching disabled for this publish order".to_string(),
			evidence: policy.evidence.clone(),
		});
		batches.push(PublishRateLimitBatch {
			registry,
			operation,
			batch_index: 1,
			total_batches: 1,
			packages: requests
				.iter()
				.map(|request| request.package_id.clone())
				.collect(),
			recommended_wait_seconds: None,
		});
	}

	PublishRateLimitReport {
		dry_run,
		windows,
		batches,
		warnings: Vec::new(),
	}
}

use monochange_publish::RegistryEndpoints;
use monochange_publish::filter_pending_publish_requests_with_transport;

fn build_placeholder_plan_requests_with_transport(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	packages: &[monochange_core::PackageRecord],
	selected_packages: &BTreeSet<String>,
	client: &reqwest::blocking::Client,
	endpoints: &RegistryEndpoints,
) -> MonochangeResult<Vec<package_publish::PublishRequest>> {
	let requests = package_publish::build_placeholder_requests(
		root,
		configuration,
		packages,
		selected_packages,
	)?;
	filter_pending_publish_requests_with_transport(&requests, client, endpoints)
}

fn build_release_plan_requests_with_transport(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	packages: &[monochange_core::PackageRecord],
	selected_packages: &BTreeSet<String>,
	client: &reqwest::blocking::Client,
	endpoints: &RegistryEndpoints,
) -> MonochangeResult<Vec<package_publish::PublishRequest>> {
	let publications = package_publish::release_record_package_publications_from_prepared_or_head(
		root,
		prepared_release,
	)?;
	let requests = package_publish::build_release_requests(
		configuration,
		packages,
		&publications,
		selected_packages,
	)?;
	filter_pending_publish_requests_with_transport(&requests, client, endpoints)
}

use std::fs;

use httpmock::Method::GET;
use httpmock::MockServer;
use monochange_core::DependencyKind;
use monochange_core::PackagePublicationTarget;
use monochange_core::PublishAttestationSettings;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::TrustedPublishingSettings;
use semver::Version;
use tempfile::tempdir;

use super::*;

fn copy_fixture_dir(source: &Path, destination: &Path) {
	copy_fixture_entry(source, destination, source);
}

fn copy_fixture_entry(source: &Path, destination: &Path, current: &Path) {
	let metadata = fs::metadata(current)
		.unwrap_or_else(|error| panic!("fixture metadata {}: {error}", current.display()));
	let relative = current
		.strip_prefix(source)
		.unwrap_or_else(|error| panic!("strip fixture prefix: {error}"));
	let target = destination.join(relative);

	if metadata.is_dir() {
		fs::create_dir_all(&target)
			.unwrap_or_else(|error| panic!("create fixture dir {}: {error}", target.display()));
		for entry in fs::read_dir(current)
			.unwrap_or_else(|error| panic!("read fixture dir {}: {error}", current.display()))
		{
			let entry = entry.unwrap_or_else(|error| panic!("fixture dir entry: {error}"));
			copy_fixture_entry(source, destination, &entry.path());
		}
		return;
	}

	if let Some(parent) = target.parent() {
		fs::create_dir_all(parent)
			.unwrap_or_else(|error| panic!("create fixture parent {}: {error}", parent.display()));
	}
	fs::copy(current, &target)
		.unwrap_or_else(|error| panic!("copy fixture {}: {error}", current.display()));
}

fn sample_publish_request(
	package_id: &str,
	ecosystem: monochange_core::Ecosystem,
	registry: RegistryKind,
	mode: PublishMode,
) -> package_publish::PublishRequest {
	let package_name = package_id.strip_prefix("monochange__").map_or_else(
		|| package_id.to_string(),
		|name| format!("@monochange/{name}"),
	);
	package_publish::PublishRequest {
		package_id: package_id.to_string(),
		package_name,
		ecosystem,
		manifest_path: Path::new("workspace/package.json").to_path_buf(),
		package_root: Path::new("workspace").to_path_buf(),
		registry,
		package_manager: Some("pnpm".to_string()),
		package_metadata: BTreeMap::new(),
		mode,
		version: Version::new(1, 0, 0).to_string(),
		placeholder: false,
		trusted_publishing: TrustedPublishingSettings::default(),
		attestations: PublishAttestationSettings::default(),
		placeholder_readme: String::new(),
	}
}

fn publish_request(package_id: &str, registry: RegistryKind) -> package_publish::PublishRequest {
	let ecosystem = match registry {
		RegistryKind::CratesIo => monochange_core::Ecosystem::Cargo,
		RegistryKind::Npm => monochange_core::Ecosystem::Npm,
		other => panic!("unsupported test registry: {other}"),
	};
	sample_publish_request(package_id, ecosystem, registry, PublishMode::Builtin)
}

fn dependency_levels(dependencies: &BTreeMap<&str, Vec<&str>>) -> BTreeMap<usize, Vec<String>> {
	fn rank<'a>(
		package: &'a str,
		dependencies: &BTreeMap<&'a str, Vec<&'a str>>,
		ranks: &mut BTreeMap<&'a str, usize>,
	) -> usize {
		if let Some(rank) = ranks.get(package) {
			return *rank;
		}
		let package_rank = dependencies.get(package).map_or(0, |package_dependencies| {
			package_dependencies
				.iter()
				.map(|dependency| rank(dependency, dependencies, ranks) + 1)
				.max()
				.unwrap_or(0)
		});
		ranks.insert(package, package_rank);
		package_rank
	}

	let mut ranks = BTreeMap::new();
	for package in dependencies.keys() {
		rank(package, dependencies, &mut ranks);
	}

	let mut levels = BTreeMap::<usize, Vec<String>>::new();
	for (package, rank) in ranks {
		levels.entry(rank).or_default().push(package.to_string());
	}
	levels
}

fn dependency_packages(
	dependencies: &BTreeMap<&str, Vec<&str>>,
	ecosystem: monochange_core::Ecosystem,
) -> Vec<monochange_core::PackageRecord> {
	dependencies
		.iter()
		.map(|(package, package_dependencies)| {
			let manifest_name = match ecosystem {
				monochange_core::Ecosystem::Cargo => "Cargo.toml",
				monochange_core::Ecosystem::Npm => "package.json",
				_ => "manifest",
			};
			let mut record = monochange_core::PackageRecord::new(
				ecosystem,
				*package,
				Path::new("/workspace").join(package).join(manifest_name),
				Path::new("/workspace").to_path_buf(),
				None,
				monochange_core::PublishState::Public,
			);
			record.id = (*package).to_string();
			record.declared_dependencies = package_dependencies
				.iter()
				.map(|dependency| {
					monochange_core::PackageDependency {
						name: (*dependency).to_string(),
						kind: DependencyKind::Runtime,
						version_constraint: None,
						optional: false,
						source_field: None,
					}
				})
				.collect();
			record
		})
		.collect()
}

fn dependency_ordered_publish_report(
	requests: &[package_publish::PublishRequest],
	crate_dependencies: &BTreeMap<&str, Vec<&str>>,
	npm_dependencies: &BTreeMap<&str, Vec<&str>>,
) -> PublishRateLimitReport {
	let packages = dependency_ordered_packages(crate_dependencies, npm_dependencies);
	plan_publish_rate_limits_for_dependency_ordered_requests(
		requests,
		&packages,
		RateLimitOperation::Publish,
		false,
	)
}

fn dependency_ordered_unbatched_publish_report(
	requests: &[package_publish::PublishRequest],
	crate_dependencies: &BTreeMap<&str, Vec<&str>>,
	npm_dependencies: &BTreeMap<&str, Vec<&str>>,
) -> PublishRateLimitReport {
	let packages = dependency_ordered_packages(crate_dependencies, npm_dependencies);
	plan_unbatched_publish_order_for_dependency_ordered_requests(
		requests,
		&packages,
		RateLimitOperation::Publish,
		false,
	)
}

fn dependency_ordered_packages(
	crate_dependencies: &BTreeMap<&str, Vec<&str>>,
	npm_dependencies: &BTreeMap<&str, Vec<&str>>,
) -> Vec<monochange_core::PackageRecord> {
	let mut packages = dependency_packages(crate_dependencies, monochange_core::Ecosystem::Cargo);
	packages.extend(dependency_packages(
		npm_dependencies,
		monochange_core::Ecosystem::Npm,
	));
	packages
}

fn render_publish_dependency_snapshot(
	crate_dependencies: &BTreeMap<&str, Vec<&str>>,
	npm_dependencies: &BTreeMap<&str, Vec<&str>>,
	report: &PublishRateLimitReport,
) -> String {
	let crate_levels = dependency_levels(crate_dependencies);
	let npm_levels = dependency_levels(npm_dependencies);
	let mut lines = vec!["dependency ranks:".to_string(), "  crates_io:".to_string()];
	for (rank, packages) in crate_levels {
		lines.push(format!("    rank {rank}: {}", packages.join(", ")));
	}
	lines.push("  npm:".to_string());
	for (rank, packages) in npm_levels {
		lines.push(format!("    rank {rank}: {}", packages.join(", ")));
	}
	lines.push("planned batches:".to_string());
	for batch in &report.batches {
		lines.push(format!(
			"  {} batch {}/{}: {}",
			batch.registry,
			batch.batch_index,
			batch.total_batches,
			batch.packages.join(", ")
		));
	}
	lines.join("\n")
}

#[test]
fn publish_plan_batches_current_project_dependencies_in_registry_order() {
	let crate_dependencies = BTreeMap::from([
		("monochange_schema", vec![]),
		("monochange_core", vec!["monochange_schema"]),
		("monochange_changelog", vec!["monochange_core"]),
		("monochange_ecmascript", vec!["monochange_core"]),
		("monochange_hosting", vec!["monochange_core"]),
		("monochange_lint", vec!["monochange_core"]),
		("monochange_linting", vec!["monochange_core"]),
		("monochange_publish", vec!["monochange_core"]),
		("monochange_semver", vec!["monochange_core"]),
		("monochange_telemetry", vec!["monochange_core"]),
		("monochange_test_helpers", vec!["monochange_core"]),
		(
			"monochange_config",
			vec!["monochange_core", "monochange_semver"],
		),
		(
			"monochange_deno",
			vec!["monochange_ecmascript", "monochange_core"],
		),
		(
			"monochange_forgejo",
			vec!["monochange_hosting", "monochange_core"],
		),
		(
			"monochange_gitea",
			vec!["monochange_hosting", "monochange_core"],
		),
		(
			"monochange_github",
			vec!["monochange_hosting", "monochange_core"],
		),
		(
			"monochange_gitlab",
			vec!["monochange_hosting", "monochange_core"],
		),
		("monochange_go", vec!["monochange_core"]),
		(
			"monochange_graph",
			vec!["monochange_core", "monochange_semver"],
		),
		("monochange_python", vec!["monochange_core"]),
		(
			"monochange_cargo",
			vec!["monochange_core", "monochange_semver"],
		),
		(
			"monochange_dart",
			vec!["monochange_core", "monochange_semver"],
		),
		(
			"monochange_npm",
			vec!["monochange_ecmascript", "monochange_core"],
		),
		(
			"monochange_analysis",
			vec!["monochange_config", "monochange_core", "monochange_graph"],
		),
		(
			"monochange",
			vec![
				"monochange_analysis",
				"monochange_config",
				"monochange_core",
			],
		),
	]);
	let npm_dependencies = BTreeMap::from([
		("monochange__cli-darwin-arm64", vec![]),
		("monochange__cli-darwin-x64", vec![]),
		("monochange__cli-linux-arm64-gnu", vec![]),
		("monochange__cli-linux-arm64-musl", vec![]),
		("monochange__cli-linux-x64-gnu", vec![]),
		("monochange__cli-linux-x64-musl", vec![]),
		("monochange__cli-win32-arm64-msvc", vec![]),
		("monochange__cli-win32-x64-msvc", vec![]),
		("monochange__skill", vec![]),
		(
			"monochange__cli",
			vec![
				"monochange__cli-darwin-arm64",
				"monochange__cli-darwin-x64",
				"monochange__cli-linux-arm64-gnu",
				"monochange__cli-linux-arm64-musl",
				"monochange__cli-linux-x64-gnu",
				"monochange__cli-linux-x64-musl",
				"monochange__cli-win32-arm64-msvc",
				"monochange__cli-win32-x64-msvc",
			],
		),
	]);
	let crate_requests = crate_dependencies
		.keys()
		.map(|package| publish_request(package, RegistryKind::CratesIo));
	let npm_requests = [
		"monochange__cli",
		"monochange__cli-darwin-arm64",
		"monochange__cli-darwin-x64",
		"monochange__cli-linux-arm64-gnu",
		"monochange__cli-linux-arm64-musl",
		"monochange__cli-linux-x64-gnu",
		"monochange__cli-linux-x64-musl",
		"monochange__cli-win32-arm64-msvc",
		"monochange__cli-win32-x64-msvc",
		"monochange__skill",
	]
	.into_iter()
	.map(|package| publish_request(package, RegistryKind::Npm));
	let requests = crate_requests.chain(npm_requests).collect::<Vec<_>>();
	let report =
		dependency_ordered_publish_report(&requests, &crate_dependencies, &npm_dependencies);
	let rendered =
		render_publish_dependency_snapshot(&crate_dependencies, &npm_dependencies, &report);

	insta::assert_snapshot!(
		&rendered,
		@r###"
dependency ranks:
  crates_io:
    rank 0: monochange_schema
    rank 1: monochange_core
    rank 2: monochange_changelog, monochange_ecmascript, monochange_go, monochange_hosting, monochange_lint, monochange_linting, monochange_publish, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers
    rank 3: monochange_cargo, monochange_config, monochange_dart, monochange_deno, monochange_forgejo, monochange_gitea, monochange_github, monochange_gitlab, monochange_graph, monochange_npm
    rank 4: monochange_analysis
    rank 5: monochange
  npm:
    rank 0: monochange__cli-darwin-arm64, monochange__cli-darwin-x64, monochange__cli-linux-arm64-gnu, monochange__cli-linux-arm64-musl, monochange__cli-linux-x64-gnu, monochange__cli-linux-x64-musl, monochange__cli-win32-arm64-msvc, monochange__cli-win32-x64-msvc, monochange__skill
    rank 1: monochange__cli
planned batches:
  crates_io batch 1/3: monochange_schema, monochange_core, monochange_changelog, monochange_ecmascript, monochange_go, monochange_hosting, monochange_lint, monochange_linting, monochange_publish, monochange_python
  crates_io batch 2/3: monochange_semver, monochange_telemetry, monochange_test_helpers, monochange_deno, monochange_npm, monochange_forgejo, monochange_gitea, monochange_github, monochange_gitlab, monochange_cargo
  crates_io batch 3/3: monochange_config, monochange_dart, monochange_graph, monochange_analysis, monochange
  npm batch 1/1: monochange__cli-darwin-arm64, monochange__cli-darwin-x64, monochange__cli-linux-arm64-gnu, monochange__cli-linux-arm64-musl, monochange__cli-linux-x64-gnu, monochange__cli-linux-x64-musl, monochange__cli-win32-arm64-msvc, monochange__cli-win32-x64-msvc, monochange__skill, monochange__cli
"###
	);
}

#[test]
fn publish_plan_orders_current_project_dependencies_without_batching() {
	let crate_dependencies = BTreeMap::from([
		("monochange_schema", vec![]),
		("monochange_core", vec!["monochange_schema"]),
		("monochange_changelog", vec!["monochange_core"]),
		("monochange_ecmascript", vec!["monochange_core"]),
		("monochange_hosting", vec!["monochange_core"]),
		("monochange_lint", vec!["monochange_core"]),
		("monochange_linting", vec!["monochange_core"]),
		("monochange_publish", vec!["monochange_core"]),
		("monochange_semver", vec!["monochange_core"]),
		("monochange_telemetry", vec!["monochange_core"]),
		("monochange_test_helpers", vec!["monochange_core"]),
		(
			"monochange_config",
			vec!["monochange_core", "monochange_semver"],
		),
		(
			"monochange_deno",
			vec!["monochange_ecmascript", "monochange_core"],
		),
		(
			"monochange_forgejo",
			vec!["monochange_hosting", "monochange_core"],
		),
		(
			"monochange_gitea",
			vec!["monochange_hosting", "monochange_core"],
		),
		(
			"monochange_github",
			vec!["monochange_hosting", "monochange_core"],
		),
		(
			"monochange_gitlab",
			vec!["monochange_hosting", "monochange_core"],
		),
		("monochange_go", vec!["monochange_core"]),
		(
			"monochange_graph",
			vec!["monochange_core", "monochange_semver"],
		),
		("monochange_python", vec!["monochange_core"]),
		(
			"monochange_cargo",
			vec!["monochange_core", "monochange_semver"],
		),
		(
			"monochange_dart",
			vec!["monochange_core", "monochange_semver"],
		),
		(
			"monochange_npm",
			vec!["monochange_ecmascript", "monochange_core"],
		),
		(
			"monochange_analysis",
			vec!["monochange_config", "monochange_core", "monochange_graph"],
		),
		(
			"monochange",
			vec![
				"monochange_analysis",
				"monochange_config",
				"monochange_core",
			],
		),
	]);
	let npm_dependencies = BTreeMap::from([
		("monochange__cli-darwin-arm64", vec![]),
		("monochange__cli-linux-x64-gnu", vec![]),
		("monochange__skill", vec![]),
		(
			"monochange__cli",
			vec![
				"monochange__cli-darwin-arm64",
				"monochange__cli-linux-x64-gnu",
			],
		),
	]);
	let crate_requests = crate_dependencies
		.keys()
		.map(|package| publish_request(package, RegistryKind::CratesIo));
	let npm_requests = [
		"monochange__cli",
		"monochange__cli-darwin-arm64",
		"monochange__cli-linux-x64-gnu",
		"monochange__skill",
	]
	.into_iter()
	.map(|package| publish_request(package, RegistryKind::Npm));
	let requests = crate_requests.chain(npm_requests).collect::<Vec<_>>();
	let report = dependency_ordered_unbatched_publish_report(
		&requests,
		&crate_dependencies,
		&npm_dependencies,
	);
	let rendered =
		render_publish_dependency_snapshot(&crate_dependencies, &npm_dependencies, &report);

	insta::assert_snapshot!(
		&rendered,
		@r###"
dependency ranks:
  crates_io:
    rank 0: monochange_schema
    rank 1: monochange_core
    rank 2: monochange_changelog, monochange_ecmascript, monochange_go, monochange_hosting, monochange_lint, monochange_linting, monochange_publish, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers
    rank 3: monochange_cargo, monochange_config, monochange_dart, monochange_deno, monochange_forgejo, monochange_gitea, monochange_github, monochange_gitlab, monochange_graph, monochange_npm
    rank 4: monochange_analysis
    rank 5: monochange
  npm:
    rank 0: monochange__cli-darwin-arm64, monochange__cli-linux-x64-gnu, monochange__skill
    rank 1: monochange__cli
planned batches:
  crates_io batch 1/1: monochange_schema, monochange_core, monochange_changelog, monochange_ecmascript, monochange_go, monochange_hosting, monochange_lint, monochange_linting, monochange_publish, monochange_python, monochange_semver, monochange_telemetry, monochange_test_helpers, monochange_deno, monochange_npm, monochange_forgejo, monochange_gitea, monochange_github, monochange_gitlab, monochange_cargo, monochange_config, monochange_dart, monochange_graph, monochange_analysis, monochange
  npm batch 1/1: monochange__cli-darwin-arm64, monochange__cli-linux-x64-gnu, monochange__skill, monochange__cli
"###
	);
}

#[test]
fn publish_rate_limit_mode_helpers_cover_placeholder_descriptions_and_windows() {
	assert_eq!(
		PublishRateLimitMode::Placeholder.description(),
		"placeholder publish"
	);
	assert_eq!(render_window(Some(60)), "60s");
	assert_eq!(render_window(None), "unknown window");
}

#[test]
fn registry_policies_include_pypi_without_a_fixed_quota() {
	let pypi = registry_policies()
		.into_iter()
		.find(|policy| policy.registry == RegistryKind::Pypi)
		.expect("PyPI policy should exist");

	assert_eq!(pypi.limit, None);
	assert_eq!(pypi.window_seconds, None);
	assert_eq!(pypi.confidence, RateLimitConfidence::Low);
	assert!(pypi.notes.contains("PyPI does not publish"));
	let evidence = pypi.evidence.first().expect("PyPI policy evidence");
	assert_eq!(evidence.url, PYPI_TRUSTED_PUBLISHERS_DOCS);
}

#[test]
fn plan_publish_rate_limits_summarizes_pending_publications_and_batches() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/publish-rate-limits/single-window/workspace");
	copy_fixture_dir(&fixture, tempdir.path());
	let configuration = crate::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("load config: {error}"));
	let prepared_release = PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: tempdir.path().to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string(), "docs".to_string(), "web".to_string()],
		package_publications: vec![
			PackagePublicationTarget {
				package: "core".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: Version::new(1, 0, 0).to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			},
			PackagePublicationTarget {
				package: "docs".to_string(),
				ecosystem: monochange_core::Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: Version::new(1, 0, 0).to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			},
			PackagePublicationTarget {
				package: "web".to_string(),
				ecosystem: monochange_core::Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: Version::new(1, 0, 0).to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			},
		],
		version: None,
		group_version: None,
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
	};

	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/core");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/docs");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/web");
		then.status(404);
	});

	let client = reqwest::blocking::Client::builder()
		.build()
		.unwrap_or_else(|error| panic!("http client: {error}"));
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};
	let requests = build_release_plan_requests_with_transport(
		tempdir.path(),
		&configuration,
		Some(&prepared_release),
		&discover_workspace(tempdir.path())
			.unwrap_or_else(|error| panic!("discover workspace: {error}"))
			.packages,
		&BTreeSet::new(),
		&client,
		&endpoints,
	)
	.unwrap_or_else(|error| panic!("build release plan requests: {error}"));
	let report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

	assert_eq!(report.windows.len(), 2);
	assert!(report.warnings.is_empty());
	assert!(report.batches.iter().any(|batch| {
		batch.registry == RegistryKind::Npm
			&& batch.packages == vec!["docs".to_string(), "web".to_string()]
	}));
}

#[test]
fn plan_publish_rate_limits_publish_all_respects_selected_packages() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/publish-rate-limits/single-window/workspace");
	copy_fixture_dir(&fixture, tempdir.path());
	let mut configuration = crate::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("load config: {error}"));
	for package in &mut configuration.packages {
		package.publish.mode = PublishMode::External;
	}
	let selected_packages = BTreeSet::from(["docs".to_string()]);
	let report = plan_publish_rate_limits_with_selection(
		tempdir.path(),
		&configuration,
		None,
		&selected_packages,
		PublishRateLimitMode::Publish,
		true,
		true,
	)
	.unwrap_or_else(|error| panic!("plan publish all rate limits: {error}"));

	assert!(report.batches.is_empty());
	assert!(report.warnings.is_empty());
}

#[test]
fn plan_publish_rate_limits_skips_private_and_disabled_packages_from_release_batches() {
	let configuration = WorkspaceConfiguration {
		root_path: std::path::PathBuf::from("/workspace"),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: vec![
			monochange_core::PackageDefinition {
				id: "core".to_string(),
				path: std::path::PathBuf::from("crates/core"),
				package_type: monochange_core::PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				version_format: monochange_core::VersionFormat::Primary,
				publish: monochange_core::PublishSettings::default(),
			},
			monochange_core::PackageDefinition {
				id: "private".to_string(),
				path: std::path::PathBuf::from("crates/private"),
				package_type: monochange_core::PackageType::Cargo,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				version_format: monochange_core::VersionFormat::Primary,
				publish: monochange_core::PublishSettings::default(),
			},
			monochange_core::PackageDefinition {
				id: "docs".to_string(),
				path: std::path::PathBuf::from("packages/docs"),
				package_type: monochange_core::PackageType::Npm,
				changelog: None,
				excluded_changelog_types: Vec::new(),
				empty_update_message: None,
				release_title: None,
				changelog_version_title: None,
				versioned_files: Vec::new(),
				ignore_ecosystem_versioned_files: false,
				ignored_paths: Vec::new(),
				additional_paths: Vec::new(),
				tag: true,
				release: true,
				version_format: monochange_core::VersionFormat::Primary,
				publish: monochange_core::PublishSettings {
					enabled: false,
					..monochange_core::PublishSettings::default()
				},
			},
		],
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	};
	let packages = vec![
		monochange_core::PackageRecord {
			id: "cargo:crates/core/Cargo.toml".to_string(),
			name: "core".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: std::path::PathBuf::from("/workspace/crates/core/Cargo.toml"),
			workspace_root: std::path::PathBuf::from("/workspace"),
			current_version: Some(Version::new(1, 0, 0)),
			publish_state: monochange_core::PublishState::Public,
			version_group_id: None,
			metadata: BTreeMap::from([("config_id".to_string(), "core".to_string())]),
			declared_dependencies: Vec::new(),
		},
		monochange_core::PackageRecord {
			id: "cargo:crates/private/Cargo.toml".to_string(),
			name: "private".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: std::path::PathBuf::from("/workspace/crates/private/Cargo.toml"),
			workspace_root: std::path::PathBuf::from("/workspace"),
			current_version: Some(Version::new(1, 0, 0)),
			publish_state: monochange_core::PublishState::Private,
			version_group_id: None,
			metadata: BTreeMap::from([("config_id".to_string(), "private".to_string())]),
			declared_dependencies: Vec::new(),
		},
		monochange_core::PackageRecord {
			id: "npm:packages/docs/package.json".to_string(),
			name: "docs".to_string(),
			ecosystem: monochange_core::Ecosystem::Npm,
			manifest_path: std::path::PathBuf::from("/workspace/packages/docs/package.json"),
			workspace_root: std::path::PathBuf::from("/workspace"),
			current_version: Some(Version::new(1, 0, 0)),
			publish_state: monochange_core::PublishState::Public,
			version_group_id: None,
			metadata: BTreeMap::from([("config_id".to_string(), "docs".to_string())]),
			declared_dependencies: Vec::new(),
		},
	];
	let publications = vec![
		PackagePublicationTarget {
			package: "core".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			version: Version::new(1, 0, 1).to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
		PackagePublicationTarget {
			package: "private".to_string(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			version: Version::new(1, 0, 1).to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
		PackagePublicationTarget {
			package: "docs".to_string(),
			ecosystem: monochange_core::Ecosystem::Npm,
			registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
			version: Version::new(1, 0, 1).to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
		},
	];
	let requests = package_publish::build_release_requests(
		&configuration,
		&packages,
		&publications,
		&BTreeSet::new(),
	)
	.unwrap_or_else(|error| panic!("build release requests: {error}"));
	let report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

	assert_eq!(requests.len(), 1);
	assert_eq!(requests[0].package_id, "core");
	assert_eq!(report.windows.len(), 1);
	assert_eq!(report.windows[0].registry, RegistryKind::CratesIo);
	assert_eq!(report.windows[0].pending, 1);
	assert_eq!(report.batches.len(), 1);
	assert_eq!(report.batches[0].packages, vec!["core".to_string()]);
}

#[test]
fn plan_publish_rate_limits_skips_versions_that_are_already_published() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/publish-rate-limits/single-window/workspace");
	copy_fixture_dir(&fixture, tempdir.path());
	let configuration = crate::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("load config: {error}"));
	let prepared_release = PreparedRelease {
		plan: monochange_core::ReleasePlan {
			workspace_root: tempdir.path().to_path_buf(),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string(), "docs".to_string(), "web".to_string()],
		package_publications: vec![
			PackagePublicationTarget {
				package: "core".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: Version::new(1, 0, 0).to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			},
			PackagePublicationTarget {
				package: "docs".to_string(),
				ecosystem: monochange_core::Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: Version::new(1, 0, 0).to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			},
			PackagePublicationTarget {
				package: "web".to_string(),
				ecosystem: monochange_core::Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: Version::new(1, 0, 0).to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
			},
		],
		version: None,
		group_version: None,
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		dry_run: true,
	};
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/core");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "num": "1.0.0" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/docs");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/web");
		then.status(404);
	});

	let client = reqwest::blocking::Client::builder()
		.build()
		.unwrap_or_else(|error| panic!("http client: {error}"));
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};
	let requests = build_release_plan_requests_with_transport(
		tempdir.path(),
		&configuration,
		Some(&prepared_release),
		&discover_workspace(tempdir.path())
			.unwrap_or_else(|error| panic!("discover workspace: {error}"))
			.packages,
		&BTreeSet::new(),
		&client,
		&endpoints,
	)
	.unwrap_or_else(|error| panic!("build release plan requests: {error}"));
	let report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

	assert_eq!(report.windows.len(), 1);
	assert_eq!(report.windows[0].registry, RegistryKind::Npm);
	assert_eq!(report.windows[0].pending, 2);
	assert!(
		report
			.batches
			.iter()
			.all(|batch| batch.registry == RegistryKind::Npm)
	);
	assert!(
		report
			.batches
			.iter()
			.flat_map(|batch| batch.packages.iter())
			.all(|package| package != "core")
	);
}

#[test]
fn build_placeholder_plan_requests_skips_packages_when_any_registry_version_exists() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/publish-rate-limits/single-window/workspace");
	copy_fixture_dir(&fixture, tempdir.path());
	let configuration = crate::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("load config: {error}"));
	let discovery = discover_workspace(tempdir.path())
		.unwrap_or_else(|error| panic!("discover workspace: {error}"));
	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/core");
		then.status(200).json_body_obj(&serde_json::json!({
			"versions": [{ "num": "1.2.3" }]
		}));
	});
	server.mock(|when, then| {
		when.method(GET).path("/docs");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/web");
		then.status(404);
	});

	let client = reqwest::blocking::Client::builder()
		.build()
		.unwrap_or_else(|error| panic!("http client: {error}"));
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};
	let requests = build_placeholder_plan_requests_with_transport(
		tempdir.path(),
		&configuration,
		&discovery.packages,
		&BTreeSet::new(),
		&client,
		&endpoints,
	)
	.unwrap_or_else(|error| panic!("build placeholder plan requests: {error}"));

	assert_eq!(
		requests
			.iter()
			.map(|request| request.package_id.as_str())
			.collect::<Vec<_>>(),
		vec!["docs", "web"]
	);
}

#[test]
fn plan_publish_rate_limits_for_requests_groups_multiple_packages_into_one_batch_when_limit_is_unbounded()
 {
	let requests = vec![
		sample_publish_request(
			"docs",
			monochange_core::Ecosystem::Npm,
			RegistryKind::Npm,
			PublishMode::Builtin,
		),
		sample_publish_request(
			"web",
			monochange_core::Ecosystem::Npm,
			RegistryKind::Npm,
			PublishMode::Builtin,
		),
	];

	let report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

	assert_eq!(report.batches.len(), 1);
	assert_eq!(
		report.batches[0].packages,
		vec!["docs".to_string(), "web".to_string()]
	);
}

#[test]
fn plan_publish_rate_limits_preserves_large_dependency_chain_across_limited_batches() {
	let package_ids = (1..=50)
		.map(|index| format!("crate-{index:02}"))
		.collect::<Vec<_>>();
	let requests = package_ids
		.iter()
		.map(|package_id| {
			sample_publish_request(
				package_id,
				monochange_core::Ecosystem::Cargo,
				RegistryKind::CratesIo,
				PublishMode::Builtin,
			)
		})
		.collect::<Vec<_>>();

	let report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

	assert_eq!(report.batches.len(), 5);
	for (batch_index, batch) in report.batches.iter().enumerate() {
		let expected_start = batch_index * 10;
		let expected_end = expected_start + 10;
		assert_eq!(batch.packages, package_ids[expected_start..expected_end]);
	}
}

#[test]
fn plan_publish_rate_limits_supports_placeholder_mode_and_skips_external_requests() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests/publish-rate-limits/single-window/workspace");
	copy_fixture_dir(&fixture, tempdir.path());
	let configuration = crate::load_workspace_configuration(tempdir.path())
		.unwrap_or_else(|error| panic!("load config: {error}"));

	let server = MockServer::start();
	server.mock(|when, then| {
		when.method(GET).path("/crates/core");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/docs");
		then.status(404);
	});
	server.mock(|when, then| {
		when.method(GET).path("/web");
		then.status(404);
	});

	let client = reqwest::blocking::Client::builder()
		.build()
		.unwrap_or_else(|error| panic!("http client: {error}"));
	let endpoints = RegistryEndpoints {
		npm_registry: server.base_url(),
		crates_io_api: server.base_url(),
		crates_io_index: server.base_url(),
		pub_dev_api: server.base_url(),
		jsr_base: server.base_url(),
		pypi_api: server.base_url(),
		go_proxy: server.base_url(),
	};
	let requests = build_placeholder_plan_requests_with_transport(
		tempdir.path(),
		&configuration,
		&discover_workspace(tempdir.path())
			.unwrap_or_else(|error| panic!("discover workspace: {error}"))
			.packages,
		&BTreeSet::new(),
		&client,
		&endpoints,
	)
	.unwrap_or_else(|error| panic!("build placeholder plan requests: {error}"));
	let report = plan_publish_rate_limits_for_requests(
		&requests,
		RateLimitOperation::PlaceholderPublish,
		true,
	);

	assert!(
		report
			.windows
			.iter()
			.all(|window| { window.operation == RateLimitOperation::PlaceholderPublish })
	);

	let filtered = plan_publish_rate_limits_for_requests(
		&[
			sample_publish_request(
				"docs",
				monochange_core::Ecosystem::Npm,
				RegistryKind::Npm,
				PublishMode::Builtin,
			),
			sample_publish_request(
				"external",
				monochange_core::Ecosystem::Npm,
				RegistryKind::Npm,
				PublishMode::External,
			),
		],
		RateLimitOperation::Publish,
		true,
	);
	assert_eq!(filtered.windows.len(), 1);
	assert_eq!(filtered.windows[0].pending, 1);
	assert_eq!(filtered.batches[0].packages, vec!["docs".to_string()]);
}

#[test]
fn plan_unbatched_publish_order_skips_external_requests() {
	let report = plan_unbatched_publish_order_for_requests(
		&[
			sample_publish_request(
				"docs",
				monochange_core::Ecosystem::Npm,
				RegistryKind::Npm,
				PublishMode::Builtin,
			),
			sample_publish_request(
				"external",
				monochange_core::Ecosystem::Npm,
				RegistryKind::Npm,
				PublishMode::External,
			),
		],
		RateLimitOperation::Publish,
		true,
	);

	assert_eq!(report.windows.len(), 1);
	assert_eq!(report.windows[0].pending, 1);
	assert_eq!(report.batches.len(), 1);
	assert_eq!(report.batches[0].packages, vec!["docs".to_string()]);
}

#[test]
fn plan_window_flags_multiple_batches_when_limit_is_exceeded() {
	let policy = RegistryRateLimitPolicy {
		registry: RegistryKind::PubDev,
		operation: RateLimitOperation::Publish,
		limit: Some(12),
		window_seconds: Some(86_400),
		confidence: RateLimitConfidence::Medium,
		notes: "pub.dev limit".to_string(),
		evidence: Vec::new(),
	};

	let window = plan_window(&policy, 25);

	assert_eq!(window.batches_required, 3);
	assert!(!window.fits_single_window);
}

#[test]
fn enforce_publish_rate_limits_returns_ok_when_enforcement_is_not_triggered() {
	let configuration = WorkspaceConfiguration {
		root_path: Path::new(".").to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: vec![monochange_core::PackageDefinition {
			id: "pkg-a".to_string(),
			path: Path::new("pkg-a").to_path_buf(),
			package_type: monochange_core::PackageType::Dart,
			changelog: None,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: Vec::new(),
			additional_paths: Vec::new(),
			tag: false,
			release: false,
			version_format: monochange_core::VersionFormat::default(),
			publish: monochange_core::PublishSettings::default(),
		}],
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	};
	let unenforced = PublishRateLimitReport {
		dry_run: true,
		windows: vec![RegistryRateLimitWindowPlan {
			registry: RegistryKind::PubDev,
			operation: RateLimitOperation::Publish,
			limit: Some(12),
			window_seconds: Some(86_400),
			pending: 13,
			batches_required: 2,
			fits_single_window: false,
			confidence: RateLimitConfidence::Medium,
			notes: "limit".to_string(),
			evidence: Vec::new(),
		}],
		batches: vec![PublishRateLimitBatch {
			registry: RegistryKind::PubDev,
			operation: RateLimitOperation::Publish,
			batch_index: 1,
			total_batches: 2,
			packages: vec!["pkg-a".to_string()],
			recommended_wait_seconds: None,
		}],
		warnings: vec!["warning".to_string()],
	};
	enforce_publish_rate_limits(&configuration, &unenforced, PublishRateLimitMode::Publish)
		.unwrap_or_else(|error| panic!("unenforced rate limits should pass: {error}"));

	let mut enforced = configuration.clone();
	enforced.packages[0].publish.rate_limits.enforce = true;
	let single_window = PublishRateLimitReport {
		dry_run: true,
		windows: vec![RegistryRateLimitWindowPlan {
			fits_single_window: true,
			batches_required: 1,
			pending: 1,
			..unenforced.windows[0].clone()
		}],
		batches: vec![PublishRateLimitBatch {
			total_batches: 1,
			..unenforced.batches[0].clone()
		}],
		warnings: Vec::new(),
	};
	enforce_publish_rate_limits(&enforced, &single_window, PublishRateLimitMode::Placeholder)
		.unwrap_or_else(|error| panic!("single-window rate limits should pass: {error}"));
}

#[test]
fn enforce_publish_rate_limits_blocks_multi_batch_runs_when_enabled() {
	let requests = (0..13)
		.map(|index| {
			package_publish::PublishRequest {
				package_id: format!("pkg-{index}"),
				package_name: format!("pkg-{index}"),
				ecosystem: monochange_core::Ecosystem::Dart,
				manifest_path: Path::new("pkg-a/pubspec.yaml").to_path_buf(),
				package_root: Path::new("pkg-a").to_path_buf(),
				registry: RegistryKind::PubDev,
				package_manager: None,
				package_metadata: BTreeMap::new(),
				mode: PublishMode::Builtin,
				version: Version::new(1, 0, 0).to_string(),
				placeholder: false,
				trusted_publishing: TrustedPublishingSettings::default(),
				attestations: PublishAttestationSettings::default(),
				placeholder_readme: String::new(),
			}
		})
		.collect::<Vec<_>>();
	let report =
		plan_publish_rate_limits_for_requests(&requests, RateLimitOperation::Publish, true);

	let configuration = WorkspaceConfiguration {
		root_path: Path::new(".").to_path_buf(),
		defaults: monochange_core::WorkspaceDefaults::default(),
		changelog: monochange_core::ChangelogSettings::default(),
		packages: (0..13)
			.map(|index| {
				monochange_core::PackageDefinition {
					id: format!("pkg-{index}"),
					path: Path::new("pkg-a").to_path_buf(),
					package_type: monochange_core::PackageType::Dart,
					changelog: None,
					excluded_changelog_types: Vec::new(),
					empty_update_message: None,
					release_title: None,
					changelog_version_title: None,
					versioned_files: Vec::new(),
					ignore_ecosystem_versioned_files: false,
					ignored_paths: Vec::new(),
					additional_paths: Vec::new(),
					tag: false,
					release: false,
					version_format: monochange_core::VersionFormat::default(),
					publish: monochange_core::PublishSettings {
						rate_limits: monochange_core::PublishRateLimitSettings { enforce: true },
						..monochange_core::PublishSettings::default()
					},
				}
			})
			.collect(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	};
	let error = enforce_publish_rate_limits(&configuration, &report, PublishRateLimitMode::Publish)
		.unwrap_err();
	assert!(error.to_string().contains("blocked this run"));
}

#[test]
fn test_sort_requests_by_dependencies_orders_dependencies_first() {
	let helper = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"helper",
		Path::new("/ws/helper/Cargo.toml").to_path_buf(),
		Path::new("/ws").to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	let mut dependent = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"dependent",
		Path::new("/ws/dependent/Cargo.toml").to_path_buf(),
		Path::new("/ws").to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	dependent
		.declared_dependencies
		.push(monochange_core::PackageDependency {
			name: "helper".into(),
			kind: DependencyKind::Development,
			version_constraint: None,
			optional: false,
			source_field: None,
		});
	let mut extra = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"extra",
		Path::new("/ws/extra/Cargo.toml").to_path_buf(),
		Path::new("/ws").to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	extra
		.declared_dependencies
		.push(monochange_core::PackageDependency {
			name: "helper".into(),
			kind: DependencyKind::Development,
			version_constraint: None,
			optional: false,
			source_field: None,
		});
	let packages = vec![helper.clone(), dependent.clone(), extra.clone()];
	let mut requests = vec![
		package_publish::PublishRequest {
			package_id: dependent.id.clone(),
			package_name: "dependent".into(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: Path::new("/ws/dependent/Cargo.toml").to_path_buf(),
			package_root: Path::new("/ws/dependent").to_path_buf(),
			registry: RegistryKind::CratesIo,
			package_manager: None,
			package_metadata: BTreeMap::new(),
			mode: PublishMode::Builtin,
			version: "0.1.0".into(),
			placeholder: false,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
			placeholder_readme: String::new(),
		},
		package_publish::PublishRequest {
			package_id: helper.id.clone(),
			package_name: "helper".into(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: Path::new("/ws/helper/Cargo.toml").to_path_buf(),
			package_root: Path::new("/ws/helper").to_path_buf(),
			registry: RegistryKind::CratesIo,
			package_manager: None,
			package_metadata: BTreeMap::new(),
			mode: PublishMode::Builtin,
			version: "0.1.0".into(),
			placeholder: false,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
			placeholder_readme: String::new(),
		},
	];
	sort_requests_by_dependencies(&mut requests, &packages);
	assert_eq!(requests[0].package_id, helper.id);
	assert_eq!(requests[1].package_id, dependent.id);
}

#[test]
fn test_sort_requests_by_dependencies_keeps_original_order_on_cycle() {
	let mut a = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"crate-a",
		Path::new("/ws/a/Cargo.toml").to_path_buf(),
		Path::new("/ws").to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	let mut b = monochange_core::PackageRecord::new(
		monochange_core::Ecosystem::Cargo,
		"crate-b",
		Path::new("/ws/b/Cargo.toml").to_path_buf(),
		Path::new("/ws").to_path_buf(),
		None,
		monochange_core::PublishState::Public,
	);
	// Create a cycle: a depends on b, b depends on a
	a.declared_dependencies
		.push(monochange_core::PackageDependency {
			name: "crate-b".into(),
			kind: DependencyKind::Development,
			version_constraint: None,
			optional: false,
			source_field: None,
		});
	b.declared_dependencies
		.push(monochange_core::PackageDependency {
			name: "crate-a".into(),
			kind: DependencyKind::Development,
			version_constraint: None,
			optional: false,
			source_field: None,
		});
	let packages = vec![a.clone(), b.clone()];
	let original_ids = [a.id.clone(), b.id.clone()];
	let mut requests = vec![
		package_publish::PublishRequest {
			package_id: a.id.clone(),
			package_name: "crate-a".into(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: Path::new("/ws/a/Cargo.toml").to_path_buf(),
			package_root: Path::new("/ws/a").to_path_buf(),
			registry: RegistryKind::CratesIo,
			package_manager: None,
			package_metadata: BTreeMap::new(),
			mode: PublishMode::Builtin,
			version: "0.1.0".into(),
			placeholder: false,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
			placeholder_readme: String::new(),
		},
		package_publish::PublishRequest {
			package_id: b.id.clone(),
			package_name: "crate-b".into(),
			ecosystem: monochange_core::Ecosystem::Cargo,
			manifest_path: Path::new("/ws/b/Cargo.toml").to_path_buf(),
			package_root: Path::new("/ws/b").to_path_buf(),
			registry: RegistryKind::CratesIo,
			package_manager: None,
			package_metadata: BTreeMap::new(),
			mode: PublishMode::Builtin,
			version: "0.1.0".into(),
			placeholder: false,
			trusted_publishing: TrustedPublishingSettings::default(),
			attestations: PublishAttestationSettings::default(),
			placeholder_readme: String::new(),
		},
	];
	sort_requests_by_dependencies(&mut requests, &packages);
	// With a cycle, we expect the original order to be preserved
	assert_eq!(requests[0].package_id, original_ids[0]);
	assert_eq!(requests[1].package_id, original_ids[1]);
}
