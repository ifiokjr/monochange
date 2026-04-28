use std::fmt::Write as _;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use clap::ArgMatches;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use serde::Serialize;

use crate::OutputFormat;
use crate::parse_output_format;

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MigrationAuditStatus {
	Ready,
	MigrationNeeded,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationAuditReport {
	pub status: MigrationAuditStatus,
	pub root: String,
	pub signals: Vec<MigrationAuditSignal>,
	pub recommendations: Vec<MigrationAuditRecommendation>,
	pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationAuditSignal {
	pub kind: String,
	pub tool: String,
	pub path: String,
	pub message: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MigrationAuditRecommendation {
	pub id: String,
	pub title: String,
	pub detail: String,
}

pub(crate) fn run_migration_command(
	root: &Path,
	quiet: bool,
	migrate_matches: &ArgMatches,
) -> MonochangeResult<String> {
	if quiet {
		return Ok(String::new());
	}
	let audit_matches = migrate_matches
		.subcommand_matches("audit")
		.unwrap_or(migrate_matches);
	let format = audit_matches
		.get_one::<String>("format")
		.map_or(Ok(OutputFormat::Text), |value| parse_output_format(value))?;
	run_migration_audit(root, format)
}

pub(crate) fn run_migration_audit(root: &Path, format: OutputFormat) -> MonochangeResult<String> {
	let report = audit_migration(root)?;
	Ok(render_migration_audit_report(&report, format))
}

pub(crate) fn audit_migration(root: &Path) -> MonochangeResult<MigrationAuditReport> {
	let mut signals = Vec::new();

	add_path_signal(
		root,
		&mut signals,
		"monochange-config",
		"monochange",
		"monochange.toml",
		"monochange configuration already exists",
	);
	add_path_signal(
		root,
		&mut signals,
		"legacy-release-tool",
		"knope",
		"knope.toml",
		"knope configuration detected",
	);
	add_path_signal(
		root,
		&mut signals,
		"legacy-release-tool",
		"knope",
		".knope.toml",
		"knope configuration detected",
	);
	add_path_signal(
		root,
		&mut signals,
		"legacy-changeset-tool",
		"changesets",
		".changeset/config.json",
		"Changesets configuration detected; audit frontmatter and changelog settings before reusing files with monochange",
	);
	add_path_signal(
		root,
		&mut signals,
		"legacy-release-tool",
		"release-please",
		"release-please-config.json",
		"release-please configuration detected",
	);
	add_path_signal(
		root,
		&mut signals,
		"legacy-release-tool",
		"release-please",
		".release-please-manifest.json",
		"release-please manifest detected",
	);
	for path in [
		".releaserc",
		".releaserc.json",
		".releaserc.yml",
		".releaserc.yaml",
		".releaserc.js",
		"release.config.js",
	] {
		add_path_signal(
			root,
			&mut signals,
			"legacy-release-tool",
			"semantic-release",
			path,
			"semantic-release configuration detected",
		);
	}
	add_path_signal(
		root,
		&mut signals,
		"legacy-release-tool",
		"cargo-release",
		"release.toml",
		"cargo-release configuration detected",
	);
	add_path_signal(
		root,
		&mut signals,
		"changelog-provider",
		"mdt",
		"mdt.toml",
		"mdt changelog provider detected; compare generated changelogs with monochange changelog sections",
	);
	for path in ["CHANGELOG.md", "changelog.md"] {
		add_path_signal(
			root,
			&mut signals,
			"changelog-file",
			"markdown-changelog",
			path,
			"existing changelog detected",
		);
	}

	detect_package_json_release_tools(root, &mut signals)?;
	detect_workflow_release_tools(root, &mut signals)?;
	signals.sort_by(|left, right| {
		left.path
			.cmp(&right.path)
			.then_with(|| left.tool.cmp(&right.tool))
			.then_with(|| left.kind.cmp(&right.kind))
	});
	signals.dedup();

	let recommendations = build_recommendations(&signals);
	let status = if migration_signals(&signals).next().is_some() {
		MigrationAuditStatus::MigrationNeeded
	} else {
		MigrationAuditStatus::Ready
	};

	Ok(MigrationAuditReport {
		status,
		root: root.display().to_string(),
		signals,
		recommendations,
		next_steps: next_steps(),
	})
}

fn add_path_signal(
	root: &Path,
	signals: &mut Vec<MigrationAuditSignal>,
	kind: &str,
	tool: &str,
	relative_path: &str,
	message: &str,
) {
	if root.join(relative_path).exists() {
		push_signal(signals, kind, tool, relative_path, message);
	}
}

fn detect_package_json_release_tools(
	root: &Path,
	signals: &mut Vec<MigrationAuditSignal>,
) -> MonochangeResult<()> {
	let path = root.join("package.json");
	let Some(contents) = optional_read_to_string(&path)? else {
		return Ok(());
	};

	for (needle, tool, message) in [
		(
			"@changesets/cli",
			"changesets",
			"package.json references the Changesets CLI",
		),
		(
			"changeset",
			"changesets",
			"package.json references Changesets",
		),
		(
			"release-please",
			"release-please",
			"package.json references release-please",
		),
		(
			"semantic-release",
			"semantic-release",
			"package.json references semantic-release",
		),
		("knope", "knope", "package.json references knope"),
	] {
		if contents.contains(needle) {
			push_signal(signals, "package-script", tool, "package.json", message);
		}
	}

	Ok(())
}

fn detect_workflow_release_tools(
	root: &Path,
	signals: &mut Vec<MigrationAuditSignal>,
) -> MonochangeResult<()> {
	let workflows = root.join(".github/workflows");
	let entries = match fs::read_dir(&workflows) {
		Ok(entries) => entries,
		Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
		Err(error) => {
			return Err(MonochangeError::Io(format!(
				"failed to read {}: {error}",
				workflows.display()
			)));
		}
	};

	for entry in entries.flatten() {
		let path = entry.path();
		if !is_workflow_file(&path) {
			continue;
		}
		let contents = fs::read_to_string(&path).map_err(|error| {
			MonochangeError::Io(format!("failed to read {}: {error}", path.display()))
		})?;
		let relative_path = path
			.strip_prefix(root)
			.unwrap_or(path.as_path())
			.to_string_lossy()
			.replace('\\', "/");

		for (needle, tool, message) in [
			(
				"changesets/action",
				"changesets",
				"GitHub Actions workflow uses the Changesets action",
			),
			(
				"release-please",
				"release-please",
				"GitHub Actions workflow references release-please",
			),
			(
				"semantic-release",
				"semantic-release",
				"GitHub Actions workflow references semantic-release",
			),
			("knope", "knope", "GitHub Actions workflow references knope"),
			(
				"cargo release",
				"cargo-release",
				"GitHub Actions workflow references cargo-release",
			),
		] {
			if contents.contains(needle) {
				push_signal(signals, "ci-workflow", tool, &relative_path, message);
			}
		}
	}

	Ok(())
}

fn is_workflow_file(path: &Path) -> bool {
	path.extension()
		.and_then(|extension| extension.to_str())
		.is_some_and(|extension| matches!(extension, "yml" | "yaml"))
}

fn optional_read_to_string(path: &Path) -> MonochangeResult<Option<String>> {
	match fs::read_to_string(path) {
		Ok(contents) => Ok(Some(contents)),
		Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
		Err(error) => {
			Err(MonochangeError::Io(format!(
				"failed to read {}: {error}",
				path.display()
			)))
		}
	}
}

fn push_signal(
	signals: &mut Vec<MigrationAuditSignal>,
	kind: &str,
	tool: &str,
	path: &str,
	message: &str,
) {
	signals.push(MigrationAuditSignal {
		kind: kind.to_string(),
		tool: tool.to_string(),
		path: path.to_string(),
		message: message.to_string(),
	});
}

fn migration_signals(
	signals: &[MigrationAuditSignal],
) -> impl Iterator<Item = &MigrationAuditSignal> {
	signals
		.iter()
		.filter(|signal| signal.kind != "monochange-config" && signal.kind != "changelog-file")
}

fn build_recommendations(signals: &[MigrationAuditSignal]) -> Vec<MigrationAuditRecommendation> {
	let has_monochange = signals
		.iter()
		.any(|signal| signal.kind == "monochange-config");
	let has_legacy_release_tool = signals.iter().any(|signal| {
		matches!(
			signal.kind.as_str(),
			"legacy-release-tool" | "legacy-changeset-tool" | "package-script" | "ci-workflow"
		)
	});
	let has_changelog = signals.iter().any(|signal| {
		matches!(
			signal.kind.as_str(),
			"changelog-provider" | "changelog-file"
		)
	});
	let has_ci = signals.iter().any(|signal| signal.kind == "ci-workflow");

	let mut recommendations = Vec::new();
	if !has_monochange {
		recommendations.push(MigrationAuditRecommendation {
			id: "generate-config".to_string(),
			title: "Generate monochange configuration".to_string(),
			detail: "Run `mc init --provider github` or create `monochange.toml` manually, then review package groups, changelog sections, and publish settings.".to_string(),
		});
	}
	if has_legacy_release_tool {
		recommendations.push(MigrationAuditRecommendation {
			id: "translate-release-tooling".to_string(),
			title: "Translate existing release automation".to_string(),
			detail: "Map legacy release rules, package ordering, tag formats, and release notes into monochange release commands before removing old tooling.".to_string(),
		});
	}
	if has_changelog {
		recommendations.push(MigrationAuditRecommendation {
			id: "audit-changelogs".to_string(),
			title: "Audit changelog ownership".to_string(),
			detail: "Compare existing changelog files and providers with monochange changelog recommendations so releases do not produce duplicate or divergent notes.".to_string(),
		});
	}
	if has_ci {
		recommendations.push(MigrationAuditRecommendation {
			id: "replace-ci-workflows".to_string(),
			title: "Replace release workflows incrementally".to_string(),
			detail: "Update CI to run `mc check`, release planning, `mc publish-readiness`, and trusted publishing setup before deleting the legacy workflow.".to_string(),
		});
	}
	if has_legacy_release_tool || !has_monochange {
		recommendations.push(MigrationAuditRecommendation {
			id: "trusted-publishing-checklist".to_string(),
			title: "Plan trusted publishing per package".to_string(),
			detail: "Prefer trusted publishers for registries that support them and record repository, workflow, and environment context in each package publish configuration.".to_string(),
		});
	}

	recommendations
}

fn next_steps() -> Vec<String> {
	vec![
		"Run `mc discover --format json` and compare discovered packages with existing release tooling.".to_string(),
		"Draft or update `monochange.toml` with package groups, changelog sections, and publish settings.".to_string(),
		"Dry-run release planning and publishing before removing legacy automation.".to_string(),
	]
}

fn render_migration_audit_report(report: &MigrationAuditReport, format: OutputFormat) -> String {
	match format {
		OutputFormat::Json => serde_json::to_string_pretty(report).unwrap_or_default(),
		OutputFormat::Markdown | OutputFormat::Text => render_text_report(report),
	}
}

fn render_text_report(report: &MigrationAuditReport) -> String {
	let mut output = String::new();
	let _ = writeln!(output, "migration audit: {}", status_label(&report.status));
	let _ = writeln!(output, "root: {}", report.root);
	output.push('\n');
	output.push_str("signals:\n");
	if report.signals.is_empty() {
		output.push_str("- none detected\n");
	} else {
		for signal in &report.signals {
			let _ = writeln!(
				output,
				"- {} {} at {}: {}",
				signal.kind, signal.tool, signal.path, signal.message
			);
		}
	}

	output.push('\n');
	output.push_str("recommendations:\n");
	if report.recommendations.is_empty() {
		output.push_str("- no migration-specific recommendations detected\n");
	} else {
		for recommendation in &report.recommendations {
			let _ = writeln!(
				output,
				"- {}: {}",
				recommendation.title, recommendation.detail
			);
		}
	}

	output.push('\n');
	output.push_str("next steps:\n");
	for step in &report.next_steps {
		let _ = writeln!(output, "- {step}");
	}
	output
}

fn status_label(status: &MigrationAuditStatus) -> &'static str {
	match status {
		MigrationAuditStatus::Ready => "ready",
		MigrationAuditStatus::MigrationNeeded => "migration-needed",
	}
}
