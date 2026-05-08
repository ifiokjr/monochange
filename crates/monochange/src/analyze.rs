use std::path::Path;
use std::path::PathBuf;

use monochange_analysis::AnalysisConfig;
use monochange_analysis::ChangeAnalysis;
use monochange_analysis::ChangeFrame;
use monochange_analysis::DetectionLevel;
use monochange_analysis::ReleaseTrajectoryRefs;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_core::EffectiveReleaseIdentity;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::ReleaseOwnerKind;
use monochange_core::VersionFormat;
use serde::Serialize;

use crate::OutputFormat;
use crate::discover_workspace;
use crate::git_support::resolve_git_commit_ref;
use crate::git_support::run_git_capture;
use crate::release_artifacts::parse_tag_prefix_and_version;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzePackageSelection {
	requested_reference: String,
	package_id: String,
	package_record_id: String,
	package_name: String,
	ecosystem: monochange_core::Ecosystem,
	manifest_path: PathBuf,
	version_group_id: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeRefs {
	release: Option<String>,
	main: String,
	head: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeFrames {
	release_to_main: Option<ChangeAnalysis>,
	main_to_head: ChangeAnalysis,
	release_to_head: Option<ChangeAnalysis>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AnalyzeReport {
	package: AnalyzePackageSelection,
	release_identity: Option<EffectiveReleaseIdentity>,
	first_release: bool,
	refs: AnalyzeRefs,
	frames: AnalyzeFrames,
	warnings: Vec<String>,
}

pub(crate) fn render_analyze_report(
	root: &Path,
	package_reference: &str,
	release_ref: Option<&str>,
	main_ref: Option<&str>,
	head_ref: Option<&str>,
	detection_level: &str,
	format: OutputFormat,
) -> MonochangeResult<String> {
	let detection_level = parse_detection_level(detection_level)?;
	let refs = (release_ref, main_ref, head_ref);
	let report = build_report(root, package_reference, refs, detection_level)?;

	match format {
		OutputFormat::Json => {
			let rendered = serde_json::to_string_pretty(&report);
			debug_assert!(
				rendered.is_ok(),
				"analyze report serialization should succeed"
			);
			Ok(rendered.unwrap_or_default())
		}
		OutputFormat::Markdown | OutputFormat::Text => Ok(render_text_report(&report)),
	}
}

fn build_report(
	root: &Path,
	package_reference: &str,
	refs: (Option<&str>, Option<&str>, Option<&str>),
	detection_level: DetectionLevel,
) -> MonochangeResult<AnalyzeReport> {
	let (release_ref, main_ref, head_ref) = refs;
	let configuration = load_workspace_configuration(root)?;
	let discovery = discover_workspace(root)?;
	let selected_record_id =
		resolve_package_reference(package_reference, root, &discovery.packages)?;
	let selected_package = discovery
		.packages
		.iter()
		.find(|package| package.id == selected_record_id)
		.cloned()
		.expect("resolved package should exist in discovery packages");
	let selected_package_id = preferred_package_id(&selected_package);
	let selected_manifest_path = selected_package
		.relative_manifest_path(root)
		.unwrap_or_else(|| selected_package.manifest_path.clone());
	let release_identity = configuration.effective_release_identity(&selected_package_id);
	let main_ref = match main_ref {
		Some(main_ref) => main_ref.to_string(),
		None => default_branch_name(root)?,
	};
	let head_ref = head_ref.map_or_else(|| "HEAD".to_string(), ToString::to_string);
	let resolved_release_ref = match release_ref {
		Some(release_ref) => Some(release_ref.to_string()),
		None => latest_release_tag_for_identity(root, release_identity.as_ref())?,
	};
	let config = AnalysisConfig {
		detection_level,
		..AnalysisConfig::default()
	};
	let mut warnings = Vec::new();
	let (first_release, frames) = if let Some(release_ref) = &resolved_release_ref {
		let refs = ReleaseTrajectoryRefs {
			release_ref: release_ref.clone(),
			main_ref: main_ref.clone(),
			head_ref: head_ref.clone(),
		};
		let trajectory =
			monochange_analysis::analyze_release_trajectory_for_refs(root, &refs, &config)?;
		warnings.extend(trajectory.warnings);
		(
			false,
			AnalyzeFrames {
				release_to_main: Some(filter_change_analysis(
					trajectory.frames.release_to_main,
					&selected_package_id,
				)),
				main_to_head: filter_change_analysis(
					trajectory.frames.main_to_head,
					&selected_package_id,
				),
				release_to_head: Some(filter_change_analysis(
					trajectory.frames.release_to_head,
					&selected_package_id,
				)),
			},
		)
	} else {
		warnings.push(first_release_warning(
			&selected_package_id,
			&main_ref,
			&head_ref,
			release_identity.as_ref(),
		));
		let main_to_head =
			analyze_package_range(root, &main_ref, &head_ref, &config, &selected_package_id)?;
		(
			true,
			AnalyzeFrames {
				release_to_main: None,
				main_to_head,
				release_to_head: None,
			},
		)
	};

	Ok(AnalyzeReport {
		package: AnalyzePackageSelection {
			requested_reference: package_reference.to_string(),
			package_id: selected_package_id,
			package_record_id: selected_package.id,
			package_name: selected_package.name,
			ecosystem: selected_package.ecosystem,
			manifest_path: selected_manifest_path,
			version_group_id: selected_package.version_group_id,
		},
		release_identity,
		first_release,
		refs: AnalyzeRefs {
			release: resolved_release_ref,
			main: main_ref,
			head: head_ref,
		},
		frames,
		warnings,
	})
}

fn analyze_package_range(
	root: &Path,
	base: &str,
	head: &str,
	config: &AnalysisConfig,
	selected_package_id: &str,
) -> MonochangeResult<ChangeAnalysis> {
	let frame = ChangeFrame::CustomRange {
		base: base.to_string(),
		head: head.to_string(),
	};
	let analysis = monochange_analysis::analyze_changes(root, &frame, config)?;

	Ok(filter_change_analysis(analysis, selected_package_id))
}

fn parse_detection_level(value: &str) -> MonochangeResult<DetectionLevel> {
	match value {
		"basic" => Ok(DetectionLevel::Basic),
		"signature" => Ok(DetectionLevel::Signature),
		"semantic" => Ok(DetectionLevel::Semantic),
		_ => {
			Err(MonochangeError::Config(format!(
				"unknown detection level `{value}`; expected one of: basic, signature, semantic"
			)))
		}
	}
}

fn preferred_package_id(package: &PackageRecord) -> String {
	package
		.metadata
		.get("config_id")
		.cloned()
		.unwrap_or_else(|| package.id.clone())
}

fn parse_origin_head_branch(symbolic_ref: &str) -> Option<String> {
	let branch = symbolic_ref.trim();
	let branch = branch.strip_prefix("origin/").unwrap_or(branch);

	match branch {
		"" | "HEAD" => None,
		_ => Some(branch.to_string()),
	}
}

fn default_branch_name(root: &Path) -> MonochangeResult<String> {
	run_git_capture(
		root,
		&["rev-parse", "--abbrev-ref", "origin/HEAD"],
		"failed to determine default branch from origin/HEAD",
	)
	.ok()
	.and_then(|symbolic_ref| parse_origin_head_branch(&symbolic_ref))
	.map_or_else(|| fallback_default_branch(root), Ok)
}

fn fallback_default_branch(root: &Path) -> MonochangeResult<String> {
	for branch in ["main", "master"] {
		if resolve_git_commit_ref(root, branch).is_ok() {
			return Ok(branch.to_string());
		}
	}

	Err(MonochangeError::Discovery(
		"could not determine default branch".to_string(),
	))
}

fn latest_release_tag_for_identity(
	root: &Path,
	release_identity: Option<&EffectiveReleaseIdentity>,
) -> MonochangeResult<Option<String>> {
	let Some(release_identity) = release_identity else {
		return Ok(None);
	};
	if !release_identity.tag {
		return Ok(None);
	}

	let tag_prefix = tag_prefix_for_identity(release_identity);
	let tag_output = run_git_capture(
		root,
		&["tag", "--list", "--sort=-v:refname"],
		"failed to list git tags for release baseline resolution",
	)?;
	let latest = tag_output
		.lines()
		.map(str::trim)
		.filter(|tag| !tag.is_empty())
		.find_map(|tag| {
			let (prefix, _) = parse_tag_prefix_and_version(tag)?;
			(prefix == tag_prefix).then(|| tag.to_string())
		});

	Ok(latest)
}

fn tag_prefix_for_identity(release_identity: &EffectiveReleaseIdentity) -> String {
	match release_identity.version_format {
		VersionFormat::Namespaced => format!("{}/v", release_identity.owner_id),
		_ => "v".to_string(),
	}
}

fn first_release_warning(
	selected_package_id: &str,
	main_ref: &str,
	head_ref: &str,
	release_identity: Option<&EffectiveReleaseIdentity>,
) -> String {
	let Some(release_identity) = release_identity else {
		return format!(
			"package `{selected_package_id}` has no configured release identity; analyzing only `{main_ref} -> {head_ref}`"
		);
	};

	format!(
		"no prior release tag found for {} `{}`; treating `{selected_package_id}` as a first release and analyzing only `{main_ref} -> {head_ref}`",
		release_owner_label(release_identity.owner_kind),
		release_identity.owner_id,
	)
}

fn release_owner_label(owner_kind: ReleaseOwnerKind) -> &'static str {
	if owner_kind == ReleaseOwnerKind::Group {
		"group"
	} else {
		"package"
	}
}

fn filter_change_analysis(
	mut analysis: ChangeAnalysis,
	selected_package_id: &str,
) -> ChangeAnalysis {
	analysis
		.package_analyses
		.retain(|package_id, _| package_id == selected_package_id);
	analysis
}

fn render_text_report(report: &AnalyzeReport) -> String {
	let mut lines = vec!["analyze:".to_string()];
	lines.push(format!(
		"  package: {} ({})",
		report.package.package_id, report.package.package_name
	));
	lines.push(format!(
		"  package record: {}",
		report.package.package_record_id
	));
	lines.push(format!(
		"  ecosystem: {}",
		report.package.ecosystem.as_str()
	));
	lines.push(format!(
		"  manifest: {}",
		report.package.manifest_path.display()
	));
	if let Some(version_group_id) = &report.package.version_group_id {
		lines.push(format!("  version group: {version_group_id}"));
	}
	if let Some(release_identity) = &report.release_identity {
		lines.push(format!(
			"  release identity: {} {}",
			release_owner_label(release_identity.owner_kind),
			release_identity.owner_id
		));
	}
	lines.push("  refs:".to_string());
	if let Some(release_ref) = &report.refs.release {
		lines.push(format!("    release: {release_ref}"));
	} else {
		lines.push("    release: none".to_string());
	}
	lines.push(format!("    main: {}", report.refs.main));
	lines.push(format!("    head: {}", report.refs.head));
	lines.push(format!("  first release: {}", yes_no(report.first_release)));

	if !report.warnings.is_empty() {
		lines.push(String::new());
		lines.push("warnings:".to_string());
		for warning in &report.warnings {
			lines.push(format!("- {warning}"));
		}
	}

	render_frame_section(
		&mut lines,
		"main -> head",
		&report.frames.main_to_head,
		&report.package.package_id,
	);

	if let Some(release_to_main) = &report.frames.release_to_main {
		render_frame_section(
			&mut lines,
			"release -> main",
			release_to_main,
			&report.package.package_id,
		);
	}

	if let Some(release_to_head) = &report.frames.release_to_head {
		render_frame_section(
			&mut lines,
			"release -> head",
			release_to_head,
			&report.package.package_id,
		);
	}

	lines.join("\n")
}

fn render_frame_section(
	lines: &mut Vec<String>,
	label: &str,
	analysis: &ChangeAnalysis,
	selected_package_id: &str,
) {
	lines.push(String::new());
	lines.push(format!("{label}:"));
	lines.push(format!("  frame: {}", analysis.frame));
	let Some(package_analysis) = analysis.package_analyses.get(selected_package_id) else {
		let no_change_message = [
			"  no semantic changes detected for `",
			selected_package_id,
			"` in this frame",
		]
		.concat();
		lines.push(no_change_message);
		push_warning_lines_if_any(lines, &analysis.warnings);
		return;
	};

	let semantic_change_count = package_analysis.semantic_changes.len();
	lines.push(format!("  semantic changes: {semantic_change_count}"));
	push_changed_file_lines_if_any(lines, &package_analysis.changed_files);
	push_semantic_change_lines_if_any(lines, &package_analysis.semantic_changes);
	push_section_warnings_if_any(lines, &package_analysis.warnings, &analysis.warnings);
}

fn push_changed_file_lines_if_any(lines: &mut Vec<String>, changed_files: &[PathBuf]) {
	if changed_files.is_empty() {
		return;
	}

	push_changed_file_lines(lines, changed_files);
}

fn push_changed_file_lines(lines: &mut Vec<String>, changed_files: &[PathBuf]) {
	lines.push("  changed files:".to_string());
	lines.extend(
		changed_files
			.iter()
			.map(|changed_file| format!("  - {}", changed_file.display())),
	);
}

fn push_semantic_change_lines_if_any(
	lines: &mut Vec<String>,
	semantic_changes: &[monochange_analysis::SemanticChange],
) {
	if semantic_changes.is_empty() {
		return;
	}

	push_semantic_change_lines(lines, semantic_changes);
}

fn push_semantic_change_lines(
	lines: &mut Vec<String>,
	semantic_changes: &[monochange_analysis::SemanticChange],
) {
	lines.push("  changes:".to_string());
	lines.extend(
		semantic_changes
			.iter()
			.map(|change| format!("  - {} ({})", change.summary, change.file_path.display())),
	);
}

fn push_warning_lines_if_any(lines: &mut Vec<String>, warnings: &[String]) {
	if warnings.is_empty() {
		return;
	}

	push_warning_lines(lines, warnings);
}

fn push_section_warnings_if_any(
	lines: &mut Vec<String>,
	package_warnings: &[String],
	analysis_warnings: &[String],
) {
	if package_warnings.is_empty() && analysis_warnings.is_empty() {
		return;
	}

	lines.push("  warnings:".to_string());
	push_warning_items(lines, package_warnings);
	push_warning_items(lines, analysis_warnings);
}

fn push_warning_lines(lines: &mut Vec<String>, warnings: &[String]) {
	lines.push("  warnings:".to_string());
	push_warning_items(lines, warnings);
}

fn push_warning_items(lines: &mut Vec<String>, warnings: &[String]) {
	lines.extend(warnings.iter().map(|warning| format!("  - {warning}")));
}

fn yes_no(value: bool) -> &'static str {
	if value { "yes" } else { "no" }
}

#[cfg(test)]
#[path = "__tests/analyze.rs"]
mod tests;
