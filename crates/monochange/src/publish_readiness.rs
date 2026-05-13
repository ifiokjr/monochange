use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageType;
use monochange_core::ReleaseRecordDiscovery;
use monochange_core::WorkspaceConfiguration;
use serde::Deserialize;
use serde::Serialize;

use crate::OutputFormat;
use crate::PreparedRelease;
use crate::discover_release_record;
use crate::package_publish;

const PUBLISH_READINESS_KIND: &str = "monochange.publishReadiness";
const PUBLISH_READINESS_SCHEMA_VERSION: u64 = 2;
const FNV_OFFSET_BASIS: u64 = 14_695_981_039_346_656_037;
const FNV_PRIME: u64 = 1_099_511_628_211;

#[derive(Debug, Clone, Eq, PartialEq)]
pub(crate) struct PublishReadinessOptions {
	pub from: String,
	pub selected_packages: BTreeSet<String>,
	pub format: OutputFormat,
	pub output: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishReadinessGlobalStatus {
	Ready,
	Blocked,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublishReadinessPackageStatus {
	Ready,
	AlreadyPublished,
	Unsupported,
	Blocked,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishReadinessPackage {
	pub package: String,
	pub ecosystem: Ecosystem,
	pub registry: String,
	pub version: String,
	pub status: PublishReadinessPackageStatus,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublishReadinessReport {
	#[serde(default = "publish_readiness_schema_version")]
	pub schema_version: u64,
	#[serde(default = "default_publish_readiness_kind")]
	pub kind: String,
	pub status: PublishReadinessGlobalStatus,
	pub from: String,
	pub resolved_commit: String,
	pub record_commit: String,
	pub package_set_fingerprint: String,
	#[serde(default = "default_publish_readiness_input_fingerprint")]
	pub input_fingerprint: String,
	pub packages: Vec<PublishReadinessPackage>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
struct PublishReadinessSource<'a> {
	from: &'a str,
	resolved_commit: &'a str,
	record_commit: &'a str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Ord, PartialOrd)]
struct PackageIdentity<'a> {
	package: &'a str,
	ecosystem: Ecosystem,
	registry: &'a str,
	version: &'a str,
}

pub(crate) async fn run_publish_readiness(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	options: &PublishReadinessOptions,
) -> MonochangeResult<String> {
	let report = build_publish_readiness_report(
		root,
		configuration,
		&options.from,
		&options.selected_packages,
	)
	.await?;
	options
		.output
		.as_deref()
		.map(|output| write_report_artifact(output, &report))
		.transpose()?;
	render_report(&report, options.format)
}

#[cfg(test)]
pub(crate) async fn validate_publish_readiness_artifact(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	artifact_path: &Path,
) -> MonochangeResult<()> {
	let artifact = read_report_artifact(artifact_path)?;
	let current_report = build_publish_readiness_report_for_publish(
		root,
		configuration,
		prepared_release,
		selected_packages,
	)
	.await?;
	validate_publish_readiness_report(&artifact, &current_report)
}

pub(crate) async fn publish_plan_package_filter_from_readiness_artifact(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
	artifact_path: &Path,
) -> MonochangeResult<BTreeSet<String>> {
	let artifact = read_report_artifact(artifact_path)?;
	let current_report = build_publish_readiness_report_for_publish(
		root,
		configuration,
		prepared_release,
		selected_packages,
	)
	.await?;
	validate_publish_readiness_plan_artifact(&artifact, &current_report)?;

	let artifact_ready_packages = publish_plan_ready_package_ids(&artifact);
	let current_ready_packages = publish_plan_ready_package_ids(&current_report);
	Ok(current_ready_packages
		.intersection(&artifact_ready_packages)
		.cloned()
		.collect())
}

async fn build_publish_readiness_report(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	from: &str,
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<PublishReadinessReport> {
	let discovery = discover_release_record(root, from).await?;
	let input_fingerprint = publish_readiness_input_fingerprint(root, configuration)?;
	let publish_report = package_publish::run_publish_packages_with_publications(
		root,
		configuration,
		&discovery.record.package_publications,
		selected_packages,
		true,
	)
	.await?;
	Ok(build_report_from_publish_report(
		source_from_discovery(&discovery),
		&publish_report,
		input_fingerprint,
	))
}

async fn build_publish_readiness_report_for_publish(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	prepared_release: Option<&PreparedRelease>,
	selected_packages: &BTreeSet<String>,
) -> MonochangeResult<PublishReadinessReport> {
	if let Some(prepared_release) = prepared_release {
		let input_fingerprint = publish_readiness_input_fingerprint(root, configuration)?;
		let publish_report = package_publish::run_publish_packages(
			root,
			configuration,
			Some(prepared_release),
			selected_packages,
			true,
		)
		.await?;
		let source = PublishReadinessSource {
			from: "prepared-release",
			resolved_commit: "prepared-release",
			record_commit: "prepared-release",
		};
		return Ok(build_report_from_publish_report(
			source,
			&publish_report,
			input_fingerprint,
		));
	}
	build_publish_readiness_report(root, configuration, "HEAD", selected_packages).await
}

fn build_report_from_publish_report(
	source: PublishReadinessSource<'_>,
	report: &package_publish::PackagePublishReport,
	input_fingerprint: String,
) -> PublishReadinessReport {
	let packages = report
		.packages
		.iter()
		.map(|package| {
			PublishReadinessPackage {
				package: package.package.clone(),
				ecosystem: package.ecosystem,
				registry: package.registry.clone(),
				version: package.version.clone(),
				status: readiness_status_from_publish_status(package.status),
				message: package.message.clone(),
			}
		})
		.collect::<Vec<_>>();
	let status = if packages.iter().any(|package| {
		matches!(
			package.status,
			PublishReadinessPackageStatus::Unsupported | PublishReadinessPackageStatus::Blocked
		)
	}) {
		PublishReadinessGlobalStatus::Blocked
	} else {
		PublishReadinessGlobalStatus::Ready
	};
	let package_set_fingerprint = package_set_fingerprint(&packages);
	PublishReadinessReport {
		schema_version: PUBLISH_READINESS_SCHEMA_VERSION,
		kind: PUBLISH_READINESS_KIND.to_string(),
		status,
		from: source.from.to_string(),
		resolved_commit: source.resolved_commit.to_string(),
		record_commit: source.record_commit.to_string(),
		package_set_fingerprint,
		input_fingerprint,
		packages,
	}
}

fn source_from_discovery(discovery: &ReleaseRecordDiscovery) -> PublishReadinessSource<'_> {
	PublishReadinessSource {
		from: &discovery.input_ref,
		resolved_commit: &discovery.resolved_commit,
		record_commit: &discovery.record_commit,
	}
}

fn publish_readiness_input_fingerprint(
	root: &Path,
	configuration: &WorkspaceConfiguration,
) -> MonochangeResult<String> {
	let paths = publish_readiness_input_paths(root, configuration);
	let mut hash = FNV_OFFSET_BASIS;
	hash = update_input_fingerprint_hash(hash, b"monochange.publishReadiness.inputs.v1");

	for path in paths {
		let relative = readiness_relative_path(root, &path);
		let contents = fs::read(&path).map_err(|error| {
			MonochangeError::Io(format!(
				"failed to read publish readiness input {}: {error}",
				path.display()
			))
		})?;
		hash = update_input_fingerprint_hash(hash, relative.as_bytes());
		hash = update_input_fingerprint_hash(hash, b"\0");
		hash = update_input_fingerprint_hash(hash, contents.len().to_string().as_bytes());
		hash = update_input_fingerprint_hash(hash, b"\0");
		hash = update_input_fingerprint_hash(hash, &contents);
		hash = update_input_fingerprint_hash(hash, b"\0");
	}

	Ok(format!("fnv1a64:{hash:016x}"))
}

fn publish_readiness_input_paths(
	root: &Path,
	configuration: &WorkspaceConfiguration,
) -> BTreeSet<PathBuf> {
	let mut paths = BTreeSet::new();
	insert_existing_path(&mut paths, root.join("monochange.toml"));

	for package in &configuration.packages {
		let package_root = root.join(&package.path);
		for manifest in package_manifest_names(package.package_type) {
			insert_existing_path(&mut paths, package_root.join(manifest));
		}
		insert_lockfile_paths(&mut paths, &package_root);
	}

	insert_publish_tooling_paths(&mut paths, root);
	insert_lockfile_paths(&mut paths, root);
	paths
}

fn insert_publish_tooling_paths(paths: &mut BTreeSet<PathBuf>, root: &Path) {
	for path in [
		"Cargo.toml",
		"package.json",
		"pnpm-workspace.yaml",
		".npmrc",
		".yarnrc",
		".yarnrc.yml",
		".cargo/config.toml",
		".cargo/config",
		"rust-toolchain.toml",
		"rust-toolchain",
		"pyproject.toml",
		"setup.cfg",
		"setup.py",
		"uv.toml",
		"poetry.toml",
		"pdm.toml",
		"deno.json",
		"deno.jsonc",
		"pubspec.yaml",
	] {
		insert_existing_path(paths, root.join(path));
	}
}

fn insert_lockfile_paths(paths: &mut BTreeSet<PathBuf>, root: &Path) {
	for path in [
		"Cargo.lock",
		"package-lock.json",
		"npm-shrinkwrap.json",
		"pnpm-lock.yaml",
		"yarn.lock",
		"bun.lock",
		"bun.lockb",
		"deno.lock",
		"pubspec.lock",
		"uv.lock",
		"poetry.lock",
		"pdm.lock",
	] {
		insert_existing_path(paths, root.join(path));
	}
}

fn package_manifest_names(package_type: PackageType) -> &'static [&'static str] {
	package_manifest_names_for_type(package_type.as_str())
}

fn package_manifest_names_for_type(package_type: &str) -> &'static [&'static str] {
	match package_type {
		"cargo" => &["Cargo.toml"],
		"npm" => &["package.json"],
		"deno" => &["deno.json", "deno.jsonc"],
		"dart" | "flutter" => &["pubspec.yaml"],
		"python" => &["pyproject.toml"],
		_ => &[],
	}
}

fn insert_existing_path(paths: &mut BTreeSet<PathBuf>, path: PathBuf) {
	if path.is_file() {
		paths.insert(path);
	}
}

fn readiness_relative_path(root: &Path, path: &Path) -> String {
	path.strip_prefix(root)
		.unwrap_or(path)
		.to_string_lossy()
		.replace('\\', "/")
}

fn update_input_fingerprint_hash(mut hash: u64, bytes: &[u8]) -> u64 {
	for byte in bytes {
		hash ^= u64::from(*byte);
		hash = hash.wrapping_mul(FNV_PRIME);
	}
	hash
}

fn validate_readiness_input_fingerprint(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	if artifact.input_fingerprint == current.input_fingerprint {
		return Ok(());
	}

	Err(MonochangeError::Config(
		"publish readiness artifact inputs are stale; workspace config, manifests, lockfiles, or publish tooling inputs changed, so rerun `mc step:publish-readiness --from HEAD --output <PATH>`".to_string(),
	))
}

fn readiness_status_from_publish_status(
	status: package_publish::PackagePublishStatus,
) -> PublishReadinessPackageStatus {
	match status {
		package_publish::PackagePublishStatus::Planned
		| package_publish::PackagePublishStatus::Published => PublishReadinessPackageStatus::Ready,
		package_publish::PackagePublishStatus::SkippedExisting => {
			PublishReadinessPackageStatus::AlreadyPublished
		}
		package_publish::PackagePublishStatus::SkippedExternal => {
			PublishReadinessPackageStatus::Unsupported
		}
		package_publish::PackagePublishStatus::Blocked
		| package_publish::PackagePublishStatus::Failed => PublishReadinessPackageStatus::Blocked,
	}
}

fn publish_plan_ready_package_ids(report: &PublishReadinessReport) -> BTreeSet<String> {
	let mut package_readiness = BTreeMap::<String, bool>::new();

	for package in &report.packages {
		let is_ready = matches!(
			package.status,
			PublishReadinessPackageStatus::Ready | PublishReadinessPackageStatus::AlreadyPublished
		);
		package_readiness
			.entry(package.package.clone())
			.and_modify(|ready| *ready &= is_ready)
			.or_insert(is_ready);
	}

	package_readiness
		.into_iter()
		.filter_map(|(package, ready)| ready.then_some(package))
		.collect()
}

fn write_report_artifact(output: &Path, report: &PublishReadinessReport) -> MonochangeResult<()> {
	output
		.parent()
		.filter(|parent| !parent.as_os_str().is_empty())
		.map(create_report_artifact_directory)
		.transpose()?;
	let body = serde_json::to_string_pretty(report).map_err(publish_readiness_json_error)?;
	fs::write(output, format!("{body}\n")).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write publish readiness output {}: {error}",
			output.display()
		))
	})
}

fn read_report_artifact(input: &Path) -> MonochangeResult<PublishReadinessReport> {
	let body = fs::read_to_string(input).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to read publish readiness artifact {}: {error}",
			input.display()
		))
	})?;
	serde_json::from_str(&body).map_err(publish_readiness_json_error)
}

#[cfg(test)]
fn validate_publish_readiness_report(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	validate_readiness_artifact_header(artifact)?;
	validate_readiness_artifact_status(artifact)?;
	validate_readiness_current_status(current)?;
	validate_readiness_release_commit(artifact, current)?;
	validate_readiness_input_fingerprint(artifact, current)?;
	validate_readiness_packages(artifact, current)
}

fn validate_publish_readiness_plan_artifact(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	validate_readiness_artifact_header(artifact)?;
	validate_readiness_release_commit(artifact, current)?;
	validate_readiness_input_fingerprint(artifact, current)?;
	validate_readiness_package_subset(artifact, current)
}

fn validate_readiness_artifact_header(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.kind != PUBLISH_READINESS_KIND {
		return Err(MonochangeError::Config(format!(
			"publish readiness artifact has kind `{}`, expected `{PUBLISH_READINESS_KIND}`",
			report.kind
		)));
	}
	if report.schema_version != PUBLISH_READINESS_SCHEMA_VERSION {
		return Err(MonochangeError::Config(format!(
			"publish readiness artifact schema version {} is not supported; expected {PUBLISH_READINESS_SCHEMA_VERSION}",
			report.schema_version
		)));
	}
	Ok(())
}

#[cfg(test)]
fn validate_readiness_artifact_status(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.status == PublishReadinessGlobalStatus::Ready {
		return Ok(());
	}
	Err(MonochangeError::Config(
		"publish readiness artifact is blocked; rerun `mc publish-readiness` and resolve blockers before `mc publish`".to_string(),
	))
}

#[cfg(test)]
fn validate_readiness_current_status(report: &PublishReadinessReport) -> MonochangeResult<()> {
	if report.status == PublishReadinessGlobalStatus::Ready {
		return Ok(());
	}
	Err(MonochangeError::Config(
		"current publish readiness is blocked; rerun `mc publish-readiness` and resolve blockers before `mc publish`".to_string(),
	))
}

fn validate_readiness_release_commit(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	if artifact.record_commit == current.record_commit {
		return Ok(());
	}
	Err(MonochangeError::Config(format!(
		"publish readiness artifact was generated for release record {}, but `mc publish` selected {}; rerun `mc step:publish-readiness --from HEAD --output <PATH>`",
		artifact.record_commit, current.record_commit
	)))
}

#[cfg(test)]
fn validate_readiness_packages(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	let artifact_packages = package_identities(&artifact.packages)?;
	let current_packages = package_identities(&current.packages)?;
	if artifact.package_set_fingerprint != package_set_fingerprint(&artifact.packages) {
		return Err(MonochangeError::Config(
			"publish readiness artifact package fingerprint does not match its package list"
				.to_string(),
		));
	}
	if artifact_packages == current_packages {
		return Ok(());
	}
	let missing = current_packages
		.difference(&artifact_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();
	let stale = artifact_packages
		.difference(&current_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();
	Err(MonochangeError::Config(format!(
		"publish readiness artifact package set is stale or does not match selected packages (missing: {}; stale: {})",
		render_package_identity_list(&missing),
		render_package_identity_list(&stale)
	)))
}

fn validate_readiness_package_subset(
	artifact: &PublishReadinessReport,
	current: &PublishReadinessReport,
) -> MonochangeResult<()> {
	let artifact_packages = package_identities(&artifact.packages)?;
	let current_packages = package_identities(&current.packages)?;

	if artifact.package_set_fingerprint != package_set_fingerprint(&artifact.packages) {
		return Err(MonochangeError::Config(
			"publish readiness artifact package fingerprint does not match its package list"
				.to_string(),
		));
	}

	let missing = current_packages
		.difference(&artifact_packages)
		.map(render_package_identity)
		.collect::<Vec<_>>();

	if missing.is_empty() {
		return Ok(());
	}

	Err(MonochangeError::Config(format!(
		"publish readiness artifact does not cover selected packages: {}",
		render_package_identity_list(&missing)
	)))
}

fn package_identities(
	packages: &[PublishReadinessPackage],
) -> MonochangeResult<BTreeSet<PackageIdentity<'_>>> {
	let mut identities = BTreeSet::new();
	for package in packages {
		let identity = package_identity(package);
		if !identities.insert(identity) {
			return Err(MonochangeError::Config(format!(
				"publish readiness artifact contains duplicate package entry {}",
				render_package_identity(&identity)
			)));
		}
	}
	Ok(identities)
}

fn package_set_fingerprint(packages: &[PublishReadinessPackage]) -> String {
	let identities = packages
		.iter()
		.map(package_identity)
		.collect::<BTreeSet<_>>();
	let mut fingerprint = String::new();
	for identity in identities {
		if !fingerprint.is_empty() {
			fingerprint.push('\n');
		}
		write_package_identity(&mut fingerprint, &identity);
	}
	fingerprint
}

fn package_identity(package: &PublishReadinessPackage) -> PackageIdentity<'_> {
	PackageIdentity {
		package: package.package.as_str(),
		ecosystem: package.ecosystem,
		registry: package.registry.as_str(),
		version: package.version.as_str(),
	}
}

fn render_package_identity(identity: &PackageIdentity<'_>) -> String {
	let mut rendered = String::new();
	write_package_identity(&mut rendered, identity);
	rendered
}

fn write_package_identity(output: &mut String, identity: &PackageIdentity<'_>) {
	let _ = write!(
		output,
		"{} {:?} {} {}",
		identity.package, identity.ecosystem, identity.registry, identity.version
	);
}

fn render_package_identity_list(identities: &[String]) -> String {
	if identities.is_empty() {
		return "none".to_string();
	}
	identities.join(", ")
}

fn publish_readiness_json_error(error: impl std::fmt::Display) -> MonochangeError {
	MonochangeError::Config(format!("publish readiness JSON: {error}"))
}

fn create_report_artifact_directory(parent: &Path) -> MonochangeResult<()> {
	fs::create_dir_all(parent).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to create publish readiness output directory {}: {error}",
			parent.display()
		))
	})
}

fn render_report(
	report: &PublishReadinessReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => {
			serde_json::to_string_pretty(report)
				.map(|mut body| {
					body.push('\n');
					body
				})
				.map_err(publish_readiness_json_error)
		}
		OutputFormat::Markdown => Ok(render_markdown_report(report)),
		OutputFormat::Text => Ok(render_text_report(report)),
	}
}
fn render_text_report(report: &PublishReadinessReport) -> String {
	let mut output = String::new();
	let _ = writeln!(
		output,
		"publish readiness: {}",
		readiness_global_status_label(report.status)
	);
	let _ = writeln!(output, "release ref: {}", report.from);
	let _ = writeln!(output, "release record: {}", report.record_commit);
	if report.packages.is_empty() {
		output.push_str("packages: none\n");
	} else {
		output.push_str("packages:\n");
		for package in &report.packages {
			let _ = writeln!(
				output,
				"- {} {} {} [{}]: {}",
				package.package,
				package.version,
				package.registry,
				readiness_package_status_label(package.status),
				package.message
			);
		}
	}
	output
}

fn render_markdown_report(report: &PublishReadinessReport) -> String {
	let mut output = String::new();
	let _ = writeln!(output, "## Publish readiness");
	output.push('\n');
	let _ = writeln!(
		output,
		"- Status: `{}`",
		readiness_global_status_label(report.status)
	);
	let _ = writeln!(output, "- Release ref: `{}`", report.from);
	let _ = writeln!(output, "- Release record: `{}`", report.record_commit);
	output.push('\n');
	if report.packages.is_empty() {
		output.push_str("No packages selected for publishing.\n");
	} else {
		output.push_str("| Package | Version | Registry | Status | Message |\n");
		output.push_str("| --- | --- | --- | --- | --- |\n");
		for package in &report.packages {
			let _ = writeln!(
				output,
				"| `{}` | `{}` | `{}` | `{}` | {} |",
				package.package,
				package.version,
				package.registry,
				readiness_package_status_label(package.status),
				package.message.replace('|', "\\|")
			);
		}
	}
	output
}

fn readiness_global_status_label(status: PublishReadinessGlobalStatus) -> &'static str {
	match status {
		PublishReadinessGlobalStatus::Ready => "ready",
		PublishReadinessGlobalStatus::Blocked => "blocked",
	}
}

fn readiness_package_status_label(status: PublishReadinessPackageStatus) -> &'static str {
	match status {
		PublishReadinessPackageStatus::Ready => "ready",
		PublishReadinessPackageStatus::AlreadyPublished => "already_published",
		PublishReadinessPackageStatus::Unsupported => "unsupported",
		PublishReadinessPackageStatus::Blocked => "blocked",
	}
}

fn publish_readiness_schema_version() -> u64 {
	PUBLISH_READINESS_SCHEMA_VERSION
}

fn default_publish_readiness_kind() -> String {
	PUBLISH_READINESS_KIND.to_string()
}

fn default_publish_readiness_input_fingerprint() -> String {
	String::new()
}

#[cfg(test)]
#[path = "__tests__/publish_readiness_tests.rs"]
mod tests;
