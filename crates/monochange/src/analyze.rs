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
mod tests {
	use std::path::Path;

	use monochange_core::VersionFormat;
	use monochange_test_helpers::copy_directory;
	use monochange_test_helpers::fs::fixture_path_from;
	use monochange_test_helpers::git::git;
	use tempfile::TempDir;

	use super::*;

	fn setup_analyze_repo(tag_release: bool, with_origin_head: bool) -> TempDir {
		let scenario_root = fixture_path_from(
			env!("CARGO_MANIFEST_DIR"),
			"cli-output/analyze-group-release-trajectory",
		);
		let release = scenario_root.join("release");
		let main = scenario_root.join("main");
		let head = scenario_root.join("head");
		let tempdir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let root = tempdir.path();

		copy_directory(&release, root);
		git(root, &["init"]);
		git(root, &["config", "user.name", "monochange-tests"]);
		git(
			root,
			&["config", "user.email", "monochange-tests@example.com"],
		);
		git(root, &["add", "."]);
		git(root, &["commit", "-m", "release"]);
		git(root, &["branch", "-M", "main"]);
		if tag_release {
			git(root, &["tag", "v1.0.0"]);
		}
		if with_origin_head {
			let remote_dir = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
			git(remote_dir.path(), &["init", "--bare"]);
			let remote = remote_dir.keep();
			git(
				root,
				&["remote", "add", "origin", remote.to_string_lossy().as_ref()],
			);
			git(root, &["push", "-u", "origin", "main"]);
			git(root, &["remote", "set-head", "origin", "main"]);
		}

		copy_directory(&main, root);
		git(root, &["add", "."]);
		git(root, &["commit", "-m", "main evolution"]);
		git(root, &["checkout", "-b", "feature"]);
		copy_directory(&head, root);
		git(root, &["add", "."]);
		git(root, &["commit", "-m", "head evolution"]);

		tempdir
	}

	#[test]
	fn filter_change_analysis_keeps_only_selected_package() {
		let mut package_analyses = std::collections::BTreeMap::new();
		package_analyses.insert(
			"core".to_string(),
			monochange_analysis::PackageChangeAnalysis {
				package_id: "core".to_string(),
				package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
				package_name: "core".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				analyzer_id: Some("cargo/public-api".to_string()),
				changed_files: vec![PathBuf::from("src/lib.rs")],
				semantic_changes: Vec::new(),
				warnings: Vec::new(),
			},
		);
		package_analyses.insert(
			"app".to_string(),
			monochange_analysis::PackageChangeAnalysis {
				package_id: "app".to_string(),
				package_record_id: "cargo:crates/app/Cargo.toml".to_string(),
				package_name: "app".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				analyzer_id: Some("cargo/public-api".to_string()),
				changed_files: Vec::new(),
				semantic_changes: Vec::new(),
				warnings: Vec::new(),
			},
		);
		let filtered = filter_change_analysis(
			ChangeAnalysis {
				frame: ChangeFrame::WorkingDirectory,
				detection_level: DetectionLevel::Signature,
				package_analyses,
				warnings: vec!["warning".to_string()],
			},
			"core",
		);

		assert_eq!(filtered.package_analyses.len(), 1);
		assert!(filtered.package_analyses.contains_key("core"));
		assert_eq!(filtered.warnings, vec!["warning"]);
	}

	#[test]
	fn tag_prefix_for_identity_matches_primary_and_namespaced_tags() {
		let namespaced = EffectiveReleaseIdentity {
			owner_id: "core".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			group_id: None,
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
			members: vec!["core".to_string()],
		};
		let primary = EffectiveReleaseIdentity {
			owner_id: "sdk".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			group_id: Some("sdk".to_string()),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			members: vec!["core".to_string(), "app".to_string()],
		};

		assert_eq!(tag_prefix_for_identity(&namespaced), "core/v");
		assert_eq!(tag_prefix_for_identity(&primary), "v");
	}

	#[test]
	fn render_analyze_report_supports_json_with_explicit_refs() {
		let tempdir = setup_analyze_repo(true, true);
		let rendered = render_analyze_report(
			tempdir.path(),
			"core",
			Some("v1.0.0"),
			Some("main"),
			Some("HEAD"),
			"semantic",
			OutputFormat::Json,
		)
		.unwrap_or_else(|error| panic!("render analyze json: {error}"));

		assert!(rendered.contains("\"release\": \"v1.0.0\""));
		assert!(rendered.contains("\"firstRelease\": false"));
		assert!(rendered.contains("\"releaseToHead\""));
		assert!(rendered.contains("\"itemPath\": \"shout\""));
	}

	#[test]
	fn render_analyze_report_supports_first_release_fallback_and_text_warnings() {
		let tempdir = setup_analyze_repo(false, false);
		let rendered = render_analyze_report(
			tempdir.path(),
			"core",
			None,
			None,
			None,
			"signature",
			OutputFormat::Text,
		)
		.unwrap_or_else(|error| panic!("render analyze text: {error}"));

		assert!(rendered.contains("release: none"));
		assert!(rendered.contains("first release: yes"));
		assert!(rendered.contains("no prior release tag found for group `sdk`"));
		assert!(rendered.contains("main -> head:"));
	}

	#[test]
	fn parse_detection_level_and_default_branch_helpers_cover_error_paths() {
		let invalid_level = parse_detection_level("deep").unwrap_err().render();
		assert!(invalid_level.contains("unknown detection level"));

		let missing_repo = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let default_branch_error = default_branch_name(missing_repo.path())
			.unwrap_err()
			.render();
		assert!(default_branch_error.contains("could not determine default branch"));
	}

	#[test]
	fn parse_origin_head_branch_covers_attached_and_unset_values() {
		assert_eq!(
			parse_origin_head_branch("origin/main"),
			Some("main".to_string())
		);
		assert_eq!(
			parse_origin_head_branch(" origin/master\n"),
			Some("master".to_string())
		);
		assert_eq!(parse_origin_head_branch("origin/HEAD"), None);
		assert_eq!(parse_origin_head_branch(""), None);
	}

	#[test]
	fn default_branch_and_release_tag_resolution_cover_origin_head_and_missing_identity_paths() {
		let tempdir = setup_analyze_repo(true, true);
		assert_eq!(
			default_branch_name(tempdir.path())
				.unwrap_or_else(|error| panic!("default branch: {error}")),
			"main"
		);
		assert_eq!(
			latest_release_tag_for_identity(tempdir.path(), None)
				.unwrap_or_else(|error| panic!("no identity: {error}")),
			None
		);

		let no_tag_identity = EffectiveReleaseIdentity {
			owner_id: "core".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			group_id: None,
			tag: false,
			release: false,
			version_format: VersionFormat::Namespaced,
			members: vec!["core".to_string()],
		};
		assert_eq!(
			latest_release_tag_for_identity(tempdir.path(), Some(&no_tag_identity))
				.unwrap_or_else(|error| panic!("no tag identity: {error}")),
			None
		);
	}

	#[test]
	fn release_owner_label_covers_package_and_group_variants() {
		assert_eq!(release_owner_label(ReleaseOwnerKind::Package), "package");
		assert_eq!(release_owner_label(ReleaseOwnerKind::Group), "group");
	}

	#[test]
	fn render_frame_section_covers_change_and_warning_paths() {
		let mut lines = Vec::new();
		let mut package_analyses = std::collections::BTreeMap::new();
		package_analyses.insert(
			"core".to_string(),
			monochange_analysis::PackageChangeAnalysis {
				package_id: "core".to_string(),
				package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
				package_name: "core".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				analyzer_id: Some("cargo/public-api".to_string()),
				changed_files: vec![Path::new("src/lib.rs").to_path_buf()],
				semantic_changes: vec![monochange_analysis::SemanticChange {
					category: monochange_analysis::SemanticChangeCategory::PublicApi,
					kind: monochange_analysis::SemanticChangeKind::Added,
					item_kind: "function".to_string(),
					item_path: "shout".to_string(),
					summary: "function `shout` added".to_string(),
					file_path: Path::new("src/lib.rs").to_path_buf(),
					before_signature: None,
					after_signature: Some("fn shout (name : & str) -> String".to_string()),
				}],
				warnings: vec!["package warning".to_string()],
			},
		);
		render_frame_section(
			&mut lines,
			"main -> head",
			&ChangeAnalysis {
				frame: ChangeFrame::CustomRange {
					base: "main".to_string(),
					head: "HEAD".to_string(),
				},
				detection_level: DetectionLevel::Signature,
				package_analyses,
				warnings: vec!["frame warning".to_string()],
			},
			"core",
		);
		let rendered = lines.join("\n");

		assert!(rendered.contains("changed files:"));
		assert!(rendered.contains("function `shout` added"));
		assert!(rendered.contains("package warning"));
		assert!(rendered.contains("frame warning"));
	}

	#[test]
	fn latest_release_tag_and_text_rendering_cover_warning_branches() {
		let missing_repo = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let identity = EffectiveReleaseIdentity {
			owner_id: "sdk".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			group_id: Some("sdk".to_string()),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			members: vec!["core".to_string()],
		};
		let tag_error = latest_release_tag_for_identity(missing_repo.path(), Some(&identity))
			.unwrap_err()
			.render();
		assert!(tag_error.contains("failed to list git tags"));

		let rendered = render_text_report(&AnalyzeReport {
			package: AnalyzePackageSelection {
				requested_reference: "core".to_string(),
				package_id: "core".to_string(),
				package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
				package_name: "core".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				manifest_path: Path::new("crates/core/Cargo.toml").to_path_buf(),
				version_group_id: None,
			},
			release_identity: None,
			first_release: true,
			refs: AnalyzeRefs {
				release: None,
				main: "main".to_string(),
				head: "HEAD".to_string(),
			},
			frames: AnalyzeFrames {
				release_to_main: None,
				main_to_head: ChangeAnalysis {
					frame: ChangeFrame::CustomRange {
						base: "main".to_string(),
						head: "HEAD".to_string(),
					},
					detection_level: DetectionLevel::Signature,
					package_analyses: std::collections::BTreeMap::new(),
					warnings: vec!["frame warning".to_string()],
				},
				release_to_head: None,
			},
			warnings: vec![first_release_warning("core", "main", "HEAD", None)],
		});

		assert!(rendered.contains("has no configured release identity"));
		assert!(rendered.contains("no semantic changes detected for `core` in this frame"));
		assert!(rendered.contains("frame warning"));
	}

	#[test]
	fn render_analyze_report_propagates_workspace_errors() {
		let missing_workspace = TempDir::new().unwrap_or_else(|error| panic!("tempdir: {error}"));
		let error = render_analyze_report(
			missing_workspace.path(),
			"core",
			None,
			Some("main"),
			Some("HEAD"),
			"semantic",
			OutputFormat::Markdown,
		)
		.unwrap_err()
		.render();

		assert!(!error.is_empty());
	}

	#[test]
	fn render_frame_section_covers_empty_lists_and_missing_warning_paths() {
		let mut lines = Vec::new();
		render_frame_section(
			&mut lines,
			"release -> main",
			&ChangeAnalysis {
				frame: ChangeFrame::CustomRange {
					base: "v1.0.0".to_string(),
					head: "main".to_string(),
				},
				detection_level: DetectionLevel::Signature,
				package_analyses: std::collections::BTreeMap::new(),
				warnings: Vec::new(),
			},
			"core",
		);
		let missing_package_rendered = lines.join("\n");
		assert!(
			missing_package_rendered
				.contains("no semantic changes detected for `core` in this frame")
		);
		assert!(!missing_package_rendered.contains("  warnings:"));

		lines.clear();
		let mut package_analyses = std::collections::BTreeMap::new();
		package_analyses.insert(
			"core".to_string(),
			monochange_analysis::PackageChangeAnalysis {
				package_id: "core".to_string(),
				package_record_id: "cargo:crates/core/Cargo.toml".to_string(),
				package_name: "core".to_string(),
				ecosystem: monochange_core::Ecosystem::Cargo,
				analyzer_id: Some("cargo/public-api".to_string()),
				changed_files: Vec::new(),
				semantic_changes: Vec::new(),
				warnings: Vec::new(),
			},
		);
		render_frame_section(
			&mut lines,
			"main -> head",
			&ChangeAnalysis {
				frame: ChangeFrame::CustomRange {
					base: "main".to_string(),
					head: "HEAD".to_string(),
				},
				detection_level: DetectionLevel::Signature,
				package_analyses,
				warnings: Vec::new(),
			},
			"core",
		);
		let empty_lists_rendered = lines.join("\n");

		assert!(empty_lists_rendered.contains("semantic changes: 0"));
		assert!(!empty_lists_rendered.contains("  changed files:"));
		assert!(!empty_lists_rendered.contains("  changes:"));
		assert!(!empty_lists_rendered.contains("  warnings:"));
	}
}
