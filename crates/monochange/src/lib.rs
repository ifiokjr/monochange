#![deny(clippy::all)]

doc_comment::doctest!("../readme.md");

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

use clap::Arg;
use clap::ArgAction;
use clap::ArgMatches;
use clap::Command;
use clap::ValueEnum;
use monochange_cargo::discover_cargo_packages;
use monochange_cargo::RustSemverProvider;
use monochange_config::apply_version_groups;
use monochange_config::load_change_signals;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_core::materialize_dependency_edges;
use monochange_core::BumpSeverity;
use monochange_core::DiscoveryReport;
use monochange_core::Ecosystem;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::ReleasePlan;
use monochange_dart::discover_dart_packages;
use monochange_deno::discover_deno_packages;
use monochange_graph::build_release_plan;
use monochange_npm::discover_npm_packages;
use monochange_semver::collect_assessments;
use monochange_semver::CompatibilityProvider;
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
	Text,
	Json,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub enum ChangeBump {
	Patch,
	Minor,
	Major,
}

impl From<ChangeBump> for BumpSeverity {
	fn from(value: ChangeBump) -> Self {
		match value {
			ChangeBump::Patch => Self::Patch,
			ChangeBump::Minor => Self::Minor,
			ChangeBump::Major => Self::Major,
		}
	}
}

#[derive(Debug, Serialize)]
struct ChangeFile {
	changes: Vec<ChangeEntry>,
}

#[derive(Debug, Serialize)]
struct ChangeEntry {
	package: String,
	bump: String,
	reason: String,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	evidence: Vec<String>,
}

pub fn build_command(bin_name: &'static str) -> Command {
	Command::new(bin_name)
		.about("Manage versions and releases for your multiplatform, multilanguage monorepo")
		.subcommand_required(true)
		.arg_required_else_help(true)
		.subcommand(
			Command::new("workspace")
				.subcommand_required(true)
				.subcommand(
					Command::new("discover")
						.about("Discover packages across supported ecosystems")
						.arg(root_arg())
						.arg(format_arg()),
				),
		)
		.subcommand(
			Command::new("plan").subcommand_required(true).subcommand(
				Command::new("release")
					.about("Plan a release from explicit change input")
					.arg(root_arg())
					.arg(
						Arg::new("changes")
							.long("changes")
							.value_name("PATH")
							.required(true),
					)
					.arg(format_arg()),
			),
		)
		.subcommand(
			Command::new("changes")
				.subcommand_required(true)
				.subcommand(
					Command::new("add")
						.about("Create a change file for one or more packages")
						.arg(root_arg())
						.arg(
							Arg::new("package")
								.long("package")
								.value_name("PACKAGE")
								.action(ArgAction::Append)
								.required(true),
						)
						.arg(
							Arg::new("bump")
								.long("bump")
								.value_name("BUMP")
								.default_value("patch")
								.value_parser(clap::builder::EnumValueParser::<ChangeBump>::new()),
						)
						.arg(
							Arg::new("reason")
								.long("reason")
								.value_name("TEXT")
								.required(true),
						)
						.arg(
							Arg::new("evidence")
								.long("evidence")
								.value_name("TEXT")
								.action(ArgAction::Append),
						)
						.arg(Arg::new("output").long("output").value_name("PATH")),
				),
		)
}

fn root_arg() -> Arg {
	Arg::new("root")
		.long("root")
		.value_name("PATH")
		.default_value(".")
}

fn format_arg() -> Arg {
	Arg::new("format")
		.long("format")
		.value_name("FORMAT")
		.default_value("text")
		.value_parser(clap::builder::EnumValueParser::<OutputFormat>::new())
}

pub fn run_from_env(bin_name: &'static str) -> MonochangeResult<()> {
	let args = std::env::args_os();
	let output = run_with_args(bin_name, args)?;
	println!("{output}");
	Ok(())
}

pub fn run_with_args<I>(bin_name: &'static str, args: I) -> MonochangeResult<String>
where
	I: IntoIterator<Item = OsString>,
{
	let matches = build_command(bin_name)
		.try_get_matches_from(args)
		.map_err(|error| MonochangeError::Config(error.to_string()))?;
	execute_matches(&matches)
}

pub fn execute_matches(matches: &ArgMatches) -> MonochangeResult<String> {
	match matches.subcommand() {
		Some(("workspace", workspace_matches)) => match workspace_matches.subcommand() {
			Some(("discover", discover_matches)) => {
				let root = required_path(discover_matches, "root")?;
				let format = required_format(discover_matches, "format")?;
				render_discovery_report(&discover_workspace(&root)?, format)
			}
			_ => Err(MonochangeError::Config(
				"unknown workspace command".to_string(),
			)),
		},
		Some(("plan", plan_matches)) => match plan_matches.subcommand() {
			Some(("release", release_matches)) => {
				let root = required_path(release_matches, "root")?;
				let changes = required_path(release_matches, "changes")?;
				let format = required_format(release_matches, "format")?;
				render_release_plan(&plan_release(&root, &changes)?, format)
			}
			_ => Err(MonochangeError::Config("unknown plan command".to_string())),
		},
		Some(("changes", changes_matches)) => match changes_matches.subcommand() {
			Some(("add", add_matches)) => {
				let root = required_path(add_matches, "root")?;
				let package_refs = required_strings(add_matches, "package")?;
				let bump = required_bump(add_matches, "bump")?;
				let reason = required_string(add_matches, "reason")?;
				let evidence = optional_strings(add_matches, "evidence");
				let output = optional_path(add_matches, "output");
				let path = add_change_file(
					&root,
					&package_refs,
					bump.into(),
					&reason,
					&evidence,
					output.as_deref(),
				)?;
				Ok(format!("wrote change file {}", path.display()))
			}
			_ => Err(MonochangeError::Config(
				"unknown changes command".to_string(),
			)),
		},
		_ => Err(MonochangeError::Config("unknown command".to_string())),
	}
}

pub fn discover_workspace(root: &Path) -> MonochangeResult<DiscoveryReport> {
	let configuration = load_workspace_configuration(root)?;
	let mut warnings = Vec::new();
	let mut packages = Vec::new();

	for discovery in [
		discover_cargo_packages(root)?,
		discover_npm_packages(root)?,
		discover_deno_packages(root)?,
		discover_dart_packages(root)?,
	] {
		warnings.extend(discovery.warnings);
		packages.extend(discovery.packages);
	}

	packages.sort_by(|left, right| left.id.cmp(&right.id));
	packages.dedup_by(|left, right| left.id == right.id);

	let (version_groups, version_group_warnings) =
		apply_version_groups(&mut packages, &configuration)?;
	warnings.extend(version_group_warnings);
	let dependencies = materialize_dependency_edges(&packages);

	Ok(DiscoveryReport {
		workspace_root: root.to_path_buf(),
		packages,
		dependencies,
		version_groups,
		warnings,
	})
}

pub fn add_change_file(
	root: &Path,
	package_refs: &[String],
	bump: BumpSeverity,
	reason: &str,
	evidence: &[String],
	output: Option<&Path>,
) -> MonochangeResult<PathBuf> {
	let discovery = discover_workspace(root)?;
	for package_ref in package_refs {
		resolve_package_reference(package_ref, root, &discovery.packages)?;
	}

	let output_path = output.map_or_else(
		|| default_change_path(root, package_refs),
		Path::to_path_buf,
	);
	if let Some(parent) = output_path.parent() {
		fs::create_dir_all(parent).map_err(|error| {
			MonochangeError::Io(format!("failed to create {}: {error}", parent.display()))
		})?;
	}

	let change_file = ChangeFile {
		changes: package_refs
			.iter()
			.map(|package| ChangeEntry {
				package: package.clone(),
				bump: bump.to_string(),
				reason: reason.to_string(),
				evidence: evidence.to_vec(),
			})
			.collect(),
	};
	let content = toml::to_string(&change_file).map_err(|error| {
		MonochangeError::Config(format!("failed to serialize change file: {error}"))
	})?;
	fs::write(&output_path, content).map_err(|error| {
		MonochangeError::Io(format!(
			"failed to write {}: {error}",
			output_path.display()
		))
	})?;
	Ok(output_path)
}

pub fn plan_release(root: &Path, changes_path: &Path) -> MonochangeResult<ReleasePlan> {
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let change_signals = load_change_signals(changes_path, root, &discovery.packages)?;
	let rust_provider = RustSemverProvider;
	let providers: [&dyn CompatibilityProvider; 1] = [&rust_provider];
	let compatibility_evidence =
		collect_assessments(&providers, &discovery.packages, &change_signals);

	Ok(build_release_plan(
		root,
		&discovery.packages,
		&discovery.dependencies,
		&discovery.version_groups,
		&change_signals,
		&compatibility_evidence,
		configuration.defaults.parent_bump,
	))
}

fn render_discovery_report(
	report: &DiscoveryReport,
	format: OutputFormat,
) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => serde_json::to_string_pretty(&json_discovery_report(report))
			.map_err(|error| MonochangeError::Discovery(error.to_string())),
		OutputFormat::Text => Ok(text_discovery_report(report)),
	}
}

fn render_release_plan(plan: &ReleasePlan, format: OutputFormat) -> MonochangeResult<String> {
	match format {
		OutputFormat::Json => serde_json::to_string_pretty(&json_release_plan(plan))
			.map_err(|error| MonochangeError::Discovery(error.to_string())),
		OutputFormat::Text => Ok(text_release_plan(plan)),
	}
}

fn json_discovery_report(report: &DiscoveryReport) -> serde_json::Value {
	json!({
		"workspaceRoot": report.workspace_root,
		"packages": report.packages.iter().map(|package| {
			json!({
				"id": package.id,
				"name": package.name,
				"ecosystem": package.ecosystem.as_str(),
				"manifestPath": package.manifest_path,
				"workspaceRoot": package.workspace_root,
				"version": package.current_version.as_ref().map(ToString::to_string),
				"versionGroup": package.version_group_id,
				"publishState": format_publish_state(package.publish_state),
			})
		}).collect::<Vec<_>>(),
		"dependencies": report.dependencies.iter().map(|edge| {
			json!({
				"from": edge.from_package_id,
				"to": edge.to_package_id,
				"kind": edge.dependency_kind.to_string(),
				"direct": edge.is_direct,
			})
		}).collect::<Vec<_>>(),
		"versionGroups": report.version_groups.iter().map(|group| {
			json!({
				"id": group.group_id,
				"members": group.members,
				"mismatchDetected": group.mismatch_detected,
			})
		}).collect::<Vec<_>>(),
		"warnings": report.warnings,
	})
}

fn json_release_plan(plan: &ReleasePlan) -> serde_json::Value {
	json!({
		"workspaceRoot": plan.workspace_root,
		"decisions": plan.decisions.iter().map(|decision| {
			json!({
				"package": decision.package_id,
				"bump": decision.recommended_bump.to_string(),
				"trigger": decision.trigger_type,
				"plannedVersion": decision.planned_version.as_ref().map(ToString::to_string),
				"reasons": decision.reasons,
				"upstreamSources": decision.upstream_sources,
			})
		}).collect::<Vec<_>>(),
		"groups": plan.groups.iter().map(|group| {
			json!({
				"id": group.group_id,
				"plannedVersion": group.planned_version.as_ref().map(ToString::to_string),
				"members": group.members,
				"bump": group.recommended_bump.to_string(),
			})
		}).collect::<Vec<_>>(),
		"warnings": plan.warnings,
		"unresolvedItems": plan.unresolved_items,
		"compatibilityEvidence": plan.compatibility_evidence.iter().map(|assessment| {
			json!({
				"package": assessment.package_id,
				"provider": assessment.provider_id,
				"severity": assessment.severity.to_string(),
				"summary": assessment.summary,
				"confidence": assessment.confidence,
				"evidenceLocation": assessment.evidence_location,
			})
		}).collect::<Vec<_>>(),
	})
}

fn text_discovery_report(report: &DiscoveryReport) -> String {
	let mut counts = BTreeMap::<Ecosystem, usize>::new();
	for package in &report.packages {
		*counts.entry(package.ecosystem).or_default() += 1;
	}

	let mut lines = vec![format!(
		"Workspace discovery for {}",
		report.workspace_root.display()
	)];
	lines.push(format!("Packages: {}", report.packages.len()));
	for (ecosystem, count) in counts {
		lines.push(format!("- {ecosystem}: {count}"));
	}
	lines.push(format!("Dependencies: {}", report.dependencies.len()));
	if !report.version_groups.is_empty() {
		lines.push("Version groups:".to_string());
		for group in &report.version_groups {
			lines.push(format!("- {} ({})", group.group_id, group.members.len()));
		}
	}
	if !report.warnings.is_empty() {
		lines.push("Warnings:".to_string());
		for warning in &report.warnings {
			lines.push(format!("- {warning}"));
		}
	}
	lines.join("\n")
}

fn text_release_plan(plan: &ReleasePlan) -> String {
	let mut lines = vec![format!(
		"Release plan for {}",
		plan.workspace_root.display()
	)];
	for decision in plan
		.decisions
		.iter()
		.filter(|decision| decision.recommended_bump.is_release())
	{
		let planned_version = decision
			.planned_version
			.as_ref()
			.map_or_else(|| "unversioned".to_string(), ToString::to_string);
		lines.push(format!(
			"- {}: {} ({}) -> {}",
			decision.package_id, decision.recommended_bump, decision.trigger_type, planned_version,
		));
		for reason in &decision.reasons {
			lines.push(format!("  - {reason}"));
		}
	}
	if !plan.groups.is_empty() {
		lines.push("Version groups:".to_string());
		for group in &plan.groups {
			lines.push(format!(
				"- {}: {} -> {}",
				group.group_id,
				group.recommended_bump,
				group
					.planned_version
					.as_ref()
					.map_or_else(|| "unversioned".to_string(), ToString::to_string),
			));
		}
	}
	if !plan.compatibility_evidence.is_empty() {
		lines.push("Compatibility evidence:".to_string());
		for assessment in &plan.compatibility_evidence {
			lines.push(format!(
				"- {}: {} ({})",
				assessment.package_id, assessment.severity, assessment.summary
			));
		}
	}
	if !plan.warnings.is_empty() {
		lines.push("Warnings:".to_string());
		for warning in &plan.warnings {
			lines.push(format!("- {warning}"));
		}
	}
	lines.join("\n")
}

fn default_change_path(root: &Path, package_refs: &[String]) -> PathBuf {
	let timestamp = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.map_or(0, |duration| duration.as_secs());
	let slug_source = package_refs.first().map_or("change", String::as_str);
	let slug = slug_source
		.chars()
		.map(|character| {
			if character.is_ascii_alphanumeric() {
				character.to_ascii_lowercase()
			} else {
				'-'
			}
		})
		.collect::<String>()
		.trim_matches('-')
		.to_string();
	let slug = if slug.is_empty() {
		"change".to_string()
	} else {
		slug
	};
	root.join("changes")
		.join(format!("{timestamp}-{slug}.toml"))
}

fn format_publish_state(publish_state: monochange_core::PublishState) -> &'static str {
	match publish_state {
		monochange_core::PublishState::Public => "public",
		monochange_core::PublishState::Private => "private",
		monochange_core::PublishState::Unpublished => "unpublished",
		monochange_core::PublishState::Excluded => "excluded",
	}
}

fn required_string(matches: &ArgMatches, key: &str) -> MonochangeResult<String> {
	matches
		.get_one::<String>(key)
		.cloned()
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

fn required_strings(matches: &ArgMatches, key: &str) -> MonochangeResult<Vec<String>> {
	let values = optional_strings(matches, key);
	if values.is_empty() {
		Err(MonochangeError::Config(format!("missing `{key}`")))
	} else {
		Ok(values)
	}
}

fn optional_strings(matches: &ArgMatches, key: &str) -> Vec<String> {
	matches
		.get_many::<String>(key)
		.map(|values| values.cloned().collect())
		.unwrap_or_default()
}

fn optional_path(matches: &ArgMatches, key: &str) -> Option<PathBuf> {
	matches.get_one::<String>(key).map(PathBuf::from)
}

fn required_bump(matches: &ArgMatches, key: &str) -> MonochangeResult<ChangeBump> {
	matches
		.get_one::<ChangeBump>(key)
		.copied()
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

fn required_path(matches: &ArgMatches, key: &str) -> MonochangeResult<PathBuf> {
	matches
		.get_one::<String>(key)
		.map(PathBuf::from)
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

fn required_format(matches: &ArgMatches, key: &str) -> MonochangeResult<OutputFormat> {
	matches
		.get_one::<OutputFormat>(key)
		.copied()
		.ok_or_else(|| MonochangeError::Config(format!("missing `{key}`")))
}

#[cfg(test)]
mod __tests;
