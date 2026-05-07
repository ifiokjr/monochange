use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command as ProcessCommand;

use glob::Pattern;
use monochange_config::load_change_signals;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_core::ChangesetAffectedSettings;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::SourceConfiguration;
use monochange_core::git::git_current_branch;

use crate::discover_workspace;

/// Evaluate pull-request changeset coverage for the supplied changed paths.
///
/// This is the library entry point behind `mc step:affected-packages` and the
/// GitHub changeset-policy workflow. It loads the workspace configuration, resolves
/// changed files against configured packages, reads any attached changesets, and
/// returns a structured pass/skip/fail report.
pub fn affected_packages(
	root: &Path,
	changed_paths: &[String],
	labels: &[String],
) -> MonochangeResult<ChangesetPolicyEvaluation> {
	// Load and validate configuration
	let configuration = load_workspace_configuration(root)?;
	let verify = &configuration.changesets.affected;

	if !verify.enabled {
		return Err(MonochangeError::Config(
			"changeset verification requires `[changesets.affected].enabled = true`".to_string(),
		));
	}

	// Discover workspace and normalize inputs
	let discovery = discover_workspace(root)?;
	// Normalize labels and changed paths
	let labels = labels
		.iter()
		.map(|label| label.trim().to_string())
		.filter(|label| !label.is_empty())
		.collect::<Vec<_>>();
	let changed_paths = changed_paths
		.iter()
		.map(|path| normalize_changed_path(path))
		.filter(|path| !path.is_empty())
		.collect::<Vec<_>>();

	if let Some((current_branch, branch_prefix)) =
		current_branch_matches_pull_request_branch_prefix(root, configuration.source.as_ref())
	{
		return Ok(skipped_pull_request_branch_evaluation(
			labels,
			changed_paths,
			&current_branch,
			&branch_prefix,
		));
	}

	// Identify skip labels and changeset paths
	let matched_skip_labels = labels
		.iter()
		.filter(|label| {
			verify
				.skip_labels
				.iter()
				.any(|candidate| candidate == *label)
		})
		.cloned()
		.collect::<Vec<_>>();
	let changeset_paths = changed_paths
		.iter()
		.filter(|path| is_changeset_markdown_path(path))
		.cloned()
		.collect::<Vec<_>>();

	// Build package ID mapping
	let config_ids_by_package_id = configuration
		.packages
		.iter()
		.map(|package| {
			resolve_package_reference(&package.id, &configuration.root_path, &discovery.packages)
				.map(|package_id| (package_id, package.id.clone()))
		})
		.collect::<MonochangeResult<BTreeMap<_, _>>>()?;

	// Classify changed paths against package definitions
	let mut matched_paths = Vec::new();
	let mut ignored_paths = Vec::new();
	let mut affected_package_ids = BTreeSet::new();
	for path in changed_paths
		.iter()
		.filter(|path| !is_changeset_markdown_path(path))
	{
		if path_matches_any_global_pattern(path, &verify.ignored_paths) {
			ignored_paths.push(path.clone());
			continue;
		}

		if path_matches_any_configured_changelog(
			path,
			&configuration.packages,
			&configuration.groups,
		) {
			ignored_paths.push(path.clone());
			continue;
		}

		if path_matches_any_global_pattern(path, &verify.changed_paths) {
			matched_paths.push(path.clone());
			affected_package_ids.extend(
				configuration
					.packages
					.iter()
					.map(|package| package.id.clone()),
			);
			continue;
		}

		let mut matched_any_package = false;
		let mut ignored_by_package = false;
		for package in &configuration.packages {
			if path_touches_package(path, package) {
				matched_any_package = true;
				affected_package_ids.insert(package.id.clone());
				continue;
			}
			if path_is_ignored_for_package(path, package) {
				ignored_by_package = true;
			}
		}
		if matched_any_package {
			matched_paths.push(path.clone());
		} else if ignored_by_package {
			ignored_paths.push(path.clone());
		}
	}

	let mut covered_package_ids = BTreeSet::new();
	let mut errors = Vec::new();
	for changeset_path in &changeset_paths {
		let absolute_path = root.join(changeset_path);
		if !absolute_path.exists() {
			errors.push(format!(
				"attached changeset `{changeset_path}` does not exist in the checked-out workspace"
			));
			continue;
		}
		match load_change_signals(&absolute_path, &configuration, &discovery.packages) {
			Ok(signals) => {
				for signal in signals {
					covered_package_ids.insert(
						config_ids_by_package_id
							.get(&signal.package_id)
							.cloned()
							.unwrap_or(signal.package_id),
					);
				}
			}
			Err(error) => errors.push(error.render()),
		}
	}

	let uncovered_package_ids = affected_package_ids
		.difference(&covered_package_ids)
		.cloned()
		.collect::<Vec<_>>();
	if matched_skip_labels.is_empty() && !uncovered_package_ids.is_empty() {
		errors.push(format!(
			"changed packages are not covered by attached changesets: {}",
			uncovered_package_ids.join(", ")
		));
	}

	let affected_package_ids = affected_package_ids.into_iter().collect::<Vec<_>>();
	let covered_package_ids = covered_package_ids.into_iter().collect::<Vec<_>>();
	let required =
		!affected_package_ids.is_empty() && verify.required && matched_skip_labels.is_empty();
	let status = match (
		errors.is_empty(),
		matched_skip_labels.is_empty(),
		affected_package_ids.is_empty(),
	) {
		(false, ..) => ChangesetPolicyStatus::Failed,
		(true, false, _) => ChangesetPolicyStatus::Skipped,
		(true, true, true) => ChangesetPolicyStatus::NotRequired,
		(true, true, false) => ChangesetPolicyStatus::Passed,
	};
	let summary = match status {
		ChangesetPolicyStatus::Failed
			if errors
				.iter()
				.any(|error| error.contains("not covered by attached changesets")) =>
		{
			format!(
				"changeset verification failed: attached changesets do not cover {} changed package{}",
				uncovered_package_ids.len(),
				if uncovered_package_ids.len() == 1 { "" } else { "s" }
			)
		}
		ChangesetPolicyStatus::Failed => {
			"changeset verification failed: one or more attached changeset files are invalid"
				.to_string()
		}
		ChangesetPolicyStatus::Skipped => format!(
			"changeset verification skipped because the change has an allowed label: {}",
			matched_skip_labels.join(", ")
		),
		ChangesetPolicyStatus::NotRequired => {
			"changeset verification passed: no configured packages were affected by the changed files"
				.to_string()
		}
		ChangesetPolicyStatus::Passed => format!(
			"changeset verification passed: attached changesets cover {} changed package{}",
			affected_package_ids.len(),
			if affected_package_ids.len() == 1 { "" } else { "s" }
		),
	};

	let mut evaluation = ChangesetPolicyEvaluation {
		status,
		required,
		enforce: false,
		summary,
		comment: None,
		labels,
		matched_skip_labels,
		changed_paths,
		matched_paths,
		ignored_paths,
		changeset_paths,
		affected_package_ids,
		covered_package_ids,
		uncovered_package_ids,
		errors,
	};
	if evaluation.status == ChangesetPolicyStatus::Failed && verify.comment_on_failure {
		evaluation.comment = Some(render_changeset_verification_comment(verify, &evaluation));
	}

	Ok(evaluation)
}

fn current_branch_matches_pull_request_branch_prefix(
	root: &Path,
	source: Option<&SourceConfiguration>,
) -> Option<(String, String)> {
	let source = source?;
	let branch_prefix = source.pull_requests.branch_prefix.trim();
	if branch_prefix.is_empty() {
		return None;
	}

	let current_branch = git_current_branch(root).ok()?;
	current_branch
		.starts_with(branch_prefix)
		.then(|| (current_branch, branch_prefix.to_string()))
}

fn skipped_pull_request_branch_evaluation(
	labels: Vec<String>,
	changed_paths: Vec<String>,
	current_branch: &str,
	branch_prefix: &str,
) -> ChangesetPolicyEvaluation {
	ChangesetPolicyEvaluation {
		status: ChangesetPolicyStatus::Skipped,
		required: false,
		enforce: false,
		summary: format!(
			"changeset verification skipped because current branch `{current_branch}` starts with release pull request branch prefix `{branch_prefix}`"
		),
		comment: None,
		labels,
		matched_skip_labels: Vec::new(),
		changed_paths,
		matched_paths: Vec::new(),
		ignored_paths: Vec::new(),
		changeset_paths: Vec::new(),
		affected_package_ids: Vec::new(),
		covered_package_ids: Vec::new(),
		uncovered_package_ids: Vec::new(),
		errors: Vec::new(),
	}
}

/// Backwards-compatible alias for [`affected_packages`].
pub fn verify_changesets(
	root: &Path,
	changed_paths: &[String],
	labels: &[String],
) -> MonochangeResult<ChangesetPolicyEvaluation> {
	affected_packages(root, changed_paths, labels)
}

/// Backwards-compatible alias for [`affected_packages`].
pub fn evaluate_changeset_policy(
	root: &Path,
	changed_paths: &[String],
	labels: &[String],
) -> MonochangeResult<ChangesetPolicyEvaluation> {
	affected_packages(root, changed_paths, labels)
}

pub(crate) fn compute_changed_paths_since(
	root: &Path,
	since_rev: &str,
) -> MonochangeResult<Vec<String>> {
	let mut diff_command = ProcessCommand::new("git");
	diff_command
		.args(["diff", "--name-only", since_rev])
		.current_dir(root);
	clear_git_env(&mut diff_command);
	let diff_output = diff_command.output().map_err(|error| {
		MonochangeError::Config(format!(
			"failed to run git diff --name-only {since_rev}: {error}"
		))
	})?;
	if !diff_output.status.success() {
		let stderr = String::from_utf8_lossy(&diff_output.stderr);
		return Err(MonochangeError::Config(format!(
			"git diff --name-only {since_rev} failed: {stderr}"
		)));
	}
	let mut paths: Vec<String> = String::from_utf8_lossy(&diff_output.stdout)
		.lines()
		.map(|line| line.trim().to_string())
		.filter(|line| !line.is_empty())
		.collect();

	let mut untracked_command = ProcessCommand::new("git");
	untracked_command
		.args(["ls-files", "--others", "--exclude-standard"])
		.current_dir(root);
	clear_git_env(&mut untracked_command);
	let untracked_output = untracked_command
		.output()
		.map_err(|error| MonochangeError::Config(format!("failed to run git ls-files: {error}")))?;
	if untracked_output.status.success() {
		for line in String::from_utf8_lossy(&untracked_output.stdout).lines() {
			let path = line.trim().to_string();
			if !path.is_empty() && !paths.contains(&path) {
				paths.push(path);
			}
		}
	}

	paths.sort();
	Ok(paths)
}

pub(crate) fn normalize_changed_path(path: &str) -> String {
	let normalized = path.trim().replace('\\', "/");
	let normalized = normalized.trim_start_matches("./");
	normalized.trim_matches('/').to_string()
}

pub(crate) fn is_changeset_markdown_path(path: &str) -> bool {
	path.starts_with(".changeset/")
		&& Path::new(path)
			.extension()
			.is_some_and(|extension| extension.eq_ignore_ascii_case("md"))
}

fn path_matches_any_global_pattern(path: &str, patterns: &[String]) -> bool {
	patterns.iter().any(|pattern| {
		Pattern::new(pattern)
			.ok()
			.is_some_and(|compiled| compiled.matches(path))
	})
}

fn path_touches_package(path: &str, package: &monochange_core::PackageDefinition) -> bool {
	if matches_any_package_pattern(path, package, &package.additional_paths) {
		return true;
	}
	if !path_is_within_package(path, package) {
		return false;
	}
	!path_is_ignored_for_package(path, package)
}

fn path_is_ignored_for_package(path: &str, package: &monochange_core::PackageDefinition) -> bool {
	path_is_within_package(path, package)
		&& matches_any_package_pattern(path, package, &package.ignored_paths)
}

fn path_matches_any_configured_changelog(
	path: &str,
	packages: &[monochange_core::PackageDefinition],
	groups: &[monochange_core::GroupDefinition],
) -> bool {
	packages.iter().any(|package| {
		package
			.changelog
			.as_ref()
			.is_some_and(|target| path_matches_changelog_target(path, &target.path))
	}) || groups.iter().any(|group| {
		group
			.changelog
			.as_ref()
			.is_some_and(|target| path_matches_changelog_target(path, &target.path))
	})
}

fn path_matches_changelog_target(path: &str, changelog_path: &Path) -> bool {
	normalize_changed_path(&changelog_path.to_string_lossy()) == path
}

fn path_is_within_package(path: &str, package: &monochange_core::PackageDefinition) -> bool {
	let package_root = normalize_changed_path(&package.path.to_string_lossy());
	path == package_root || path.starts_with(&format!("{package_root}/"))
}

fn matches_any_package_pattern(
	path: &str,
	package: &monochange_core::PackageDefinition,
	patterns: &[String],
) -> bool {
	let package_root = normalize_changed_path(&package.path.to_string_lossy());
	let relative_path = path
		.strip_prefix(&format!("{package_root}/"))
		.or_else(|| (path == package_root).then_some(""));
	patterns.iter().any(|pattern| {
		Pattern::new(pattern).ok().is_some_and(|compiled| {
			compiled.matches(path)
				|| relative_path.is_some_and(|relative_path| compiled.matches(relative_path))
		})
	})
}

fn clear_git_env(command: &mut ProcessCommand) {
	for variable in [
		"GIT_DIR",
		"GIT_WORK_TREE",
		"GIT_COMMON_DIR",
		"GIT_INDEX_FILE",
		"GIT_OBJECT_DIRECTORY",
		"GIT_ALTERNATE_OBJECT_DIRECTORIES",
	] {
		command.env_remove(variable);
	}
}

fn render_changeset_verification_comment(
	verify: &ChangesetAffectedSettings,
	evaluation: &ChangesetPolicyEvaluation,
) -> String {
	let mut lines = vec![
		"### monochange changeset verification failed".to_string(),
		String::new(),
		evaluation.summary.clone(),
	];
	if !evaluation.matched_paths.is_empty() {
		lines.push(String::new());
		lines.push("Changed package paths:".to_string());
		for path in &evaluation.matched_paths {
			lines.push(format!("- `{path}`"));
		}
	}
	if !evaluation.affected_package_ids.is_empty() {
		lines.push(String::new());
		lines.push("Affected packages:".to_string());
		for package_id in &evaluation.affected_package_ids {
			lines.push(format!("- `{package_id}`"));
		}
	}
	if !evaluation.changeset_paths.is_empty() {
		lines.push(String::new());
		lines.push("Attached changeset files:".to_string());
		for path in &evaluation.changeset_paths {
			lines.push(format!("- `{path}`"));
		}
	}
	if !evaluation.errors.is_empty() {
		lines.push(String::new());
		lines.push("Errors:".to_string());
		for error in &evaluation.errors {
			lines.push(format!("- {error}"));
		}
	}
	if !verify.skip_labels.is_empty() {
		lines.push(String::new());
		lines.push("Allowed skip labels:".to_string());
		for label in &verify.skip_labels {
			lines.push(format!("- `{label}`"));
		}
	}
	lines.push(String::new());
	lines.push("How to fix:".to_string());
	lines.push("- add or update a `.changeset/*.md` file so it references every changed package or owning group".to_string());
	lines.push(
		"- for example: `mc change --package <id> --bump patch --reason \"describe the change\"`"
			.to_string(),
	);
	if !verify.skip_labels.is_empty() {
		lines.push(
			"- or apply one of the configured skip labels when no release note is required"
				.to_string(),
		);
	}
	lines.join("\n")
}

#[cfg(test)]
mod tests {
	use std::fs;
	use std::process::Command;

	use monochange_core::PackageDefinition;
	use monochange_core::PackageType;
	use monochange_core::SourceProvider as ProviderKind;
	use monochange_core::VersionFormat;

	use super::*;

	fn setup_fixture(relative: &str) -> tempfile::TempDir {
		monochange_test_helpers::fs::setup_fixture_from(env!("CARGO_MANIFEST_DIR"), relative)
	}

	fn sample_package() -> PackageDefinition {
		PackageDefinition {
			id: "core".to_string(),
			path: Path::new("crates/core").to_path_buf(),
			package_type: PackageType::Cargo,
			changelog: None,
			excluded_changelog_types: Vec::new(),
			empty_update_message: None,
			release_title: None,
			changelog_version_title: None,
			versioned_files: Vec::new(),
			ignore_ecosystem_versioned_files: false,
			ignored_paths: vec!["README.md".to_string(), "docs/**".to_string()],
			additional_paths: vec!["shared/**".to_string()],
			tag: true,
			release: true,
			publish: monochange_core::PublishSettings::default(),
			version_format: VersionFormat::Primary,
		}
	}

	fn git(root: &Path, args: &[&str]) {
		let mut command = Command::new("git");
		command.current_dir(root);
		for variable in [
			"GIT_DIR",
			"GIT_WORK_TREE",
			"GIT_COMMON_DIR",
			"GIT_INDEX_FILE",
			"GIT_OBJECT_DIRECTORY",
			"GIT_ALTERNATE_OBJECT_DIRECTORIES",
		] {
			command.env_remove(variable);
		}
		let output = command
			.args(args)
			.output()
			.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
		assert!(
			output.status.success(),
			"git {args:?} failed: {}{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
	}

	#[test]
	fn affected_packages_requires_changeset_verification_to_be_enabled() {
		let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		fs::create_dir_all(tempdir.path().join("crates/core/src"))
			.unwrap_or_else(|error| panic!("create source tree: {error}"));
		fs::write(
			tempdir.path().join("Cargo.toml"),
			"[workspace]\nmembers = [\"crates/core\"]\n",
		)
		.unwrap_or_else(|error| panic!("write workspace Cargo.toml: {error}"));
		fs::write(
			tempdir.path().join("crates/core/Cargo.toml"),
			"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
		)
		.unwrap_or_else(|error| panic!("write package Cargo.toml: {error}"));
		fs::write(
			tempdir.path().join("crates/core/src/lib.rs"),
			"pub fn core() {}\n",
		)
		.unwrap_or_else(|error| panic!("write lib.rs: {error}"));
		fs::write(
			tempdir.path().join("monochange.toml"),
			"[defaults]\npackage_type = \"cargo\"\n\n[changesets.affected]\nenabled = false\nrequired = true\ncomment_on_failure = true\n\n[package.core]\npath = \"crates/core\"\n",
		)
		.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

		let error = affected_packages(
			tempdir.path(),
			&["crates/core/src/lib.rs".to_string()],
			&Vec::new(),
		)
		.err()
		.unwrap_or_else(|| panic!("expected disabled verification error"));
		assert!(matches!(error, MonochangeError::Config(_)));
		assert_eq!(
			error.to_string(),
			"config error: changeset verification requires `[changesets.affected].enabled = true`"
		);
	}

	#[test]
	fn affected_packages_skips_release_pull_request_branches() {
		let fixture = setup_fixture("monochange/changeset-policy-base");
		let config_path = fixture.path().join("monochange.toml");
		let mut config = fs::read_to_string(&config_path)
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
		config.push_str(
			"\n[source]\nprovider = \"github\"\nowner = \"monochange\"\nrepo = \"monochange\"\n\n[source.pull_requests]\nbranch_prefix = \"monochange/release\"\n",
		);
		fs::write(&config_path, config)
			.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

		git(fixture.path(), &["init", "-b", "main"]);
		git(fixture.path(), &["config", "user.name", "monochange"]);
		git(
			fixture.path(),
			&["config", "user.email", "monochange@example.com"],
		);
		git(fixture.path(), &["add", "."]);
		git(fixture.path(), &["commit", "-m", "initial"]);
		git(
			fixture.path(),
			&["checkout", "-b", "monochange/release/affected"],
		);

		let evaluation = affected_packages(
			fixture.path(),
			&["crates/core/src/lib.rs".to_string()],
			&Vec::new(),
		)
		.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

		assert_eq!(evaluation.status, ChangesetPolicyStatus::Skipped);
		assert!(!evaluation.required);
		assert!(evaluation.affected_package_ids.is_empty());
		assert_eq!(
			evaluation.changed_paths,
			vec!["crates/core/src/lib.rs".to_string()]
		);
		assert!(
			evaluation
				.summary
				.contains("current branch `monochange/release/affected` starts")
		);
	}

	#[test]
	fn affected_packages_uses_global_affected_path_filters() {
		let fixture = setup_fixture("monochange/changeset-policy-base");
		let config_path = fixture.path().join("monochange.toml");
		let mut config = fs::read_to_string(&config_path)
			.unwrap_or_else(|error| panic!("read monochange.toml: {error}"));
		config = config.replace(
			"comment_on_failure = true",
			"comment_on_failure = true\nchanged_paths = [\"infra/**\"]\nignored_paths = [\"docs/**\"]",
		);
		config = config.replace(
			"path = \"crates/core\"\nignored_paths",
			"path = \"crates/core\"\nchangelog = \"crates/core/CHANGELOG.md\"\nignored_paths",
		);
		fs::write(&config_path, config)
			.unwrap_or_else(|error| panic!("write monochange.toml: {error}"));

		let evaluation = affected_packages(
			fixture.path(),
			&[
				"docs/readme.md".to_string(),
				"crates/core/CHANGELOG.md".to_string(),
				"infra/config.yml".to_string(),
			],
			&Vec::new(),
		)
		.unwrap_or_else(|error| panic!("affected packages for global paths: {error}"));

		assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
		assert_eq!(
			evaluation.ignored_paths,
			vec![
				"docs/readme.md".to_string(),
				"crates/core/CHANGELOG.md".to_string()
			]
		);
		assert_eq!(
			evaluation.matched_paths,
			vec!["infra/config.yml".to_string()]
		);
		assert_eq!(evaluation.affected_package_ids, vec!["core".to_string()]);
	}

	#[test]
	fn current_branch_prefix_matching_requires_non_empty_prefix() {
		let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		git(repo.path(), &["init", "-b", "main"]);
		git(repo.path(), &["config", "user.name", "monochange"]);
		git(
			repo.path(),
			&["config", "user.email", "monochange@example.com"],
		);
		fs::write(repo.path().join("tracked.txt"), "initial\n")
			.unwrap_or_else(|error| panic!("write tracked file: {error}"));
		git(repo.path(), &["add", "tracked.txt"]);
		git(repo.path(), &["commit", "-m", "initial"]);

		let mut source = SourceConfiguration {
			provider: ProviderKind::default(),
			owner: "monochange".to_string(),
			repo: "monochange".to_string(),
			host: None,
			api_url: None,
			releases: monochange_core::ProviderReleaseSettings::default(),
			pull_requests: monochange_core::ProviderMergeRequestSettings::default(),
		};

		source.pull_requests.branch_prefix = "   ".to_string();
		assert_eq!(
			current_branch_matches_pull_request_branch_prefix(repo.path(), Some(&source)),
			None
		);

		source.pull_requests.branch_prefix = "monochange/release/".to_string();
		assert_eq!(
			current_branch_matches_pull_request_branch_prefix(repo.path(), Some(&source)),
			None
		);

		git(repo.path(), &["checkout", "-b", "monochange/release/core"]);
		assert_eq!(
			current_branch_matches_pull_request_branch_prefix(repo.path(), Some(&source)),
			Some((
				"monochange/release/core".to_string(),
				"monochange/release/".to_string()
			))
		);
	}

	#[test]
	fn affected_packages_reports_missing_and_invalid_changeset_inputs() {
		let fixture = setup_fixture("monochange/changeset-policy-base");
		fs::create_dir_all(fixture.path().join(".changeset"))
			.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
		fs::write(fixture.path().join(".changeset/invalid.md"), "---\ncore:\n")
			.unwrap_or_else(|error| panic!("write invalid changeset: {error}"));

		let evaluation = affected_packages(
			fixture.path(),
			&[
				"crates/core/src/lib.rs".to_string(),
				".changeset/missing.md".to_string(),
				".changeset/invalid.md".to_string(),
			],
			&Vec::new(),
		)
		.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

		assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
		assert!(
			evaluation
				.errors
				.iter()
				.any(|error| error.contains("does not exist in the checked-out workspace"))
		);
		assert!(
			evaluation
				.errors
				.iter()
				.any(|error| error.contains("failed to parse") || error.contains("invalid"))
		);
		assert!(evaluation.summary.contains("changeset verification failed"));
		assert_eq!(
			evaluation.summary,
			"changeset verification failed: attached changesets do not cover 1 changed package"
		);
		assert!(evaluation.comment.is_some());
	}

	#[test]
	fn affected_packages_marks_ignored_paths_and_omits_failure_comments_when_disabled() {
		let fixture = setup_fixture("monochange/changeset-policy-base");
		let evaluation = affected_packages(
			fixture.path(),
			&["crates/core/tests/policy.rs".to_string()],
			&Vec::new(),
		)
		.unwrap_or_else(|error| panic!("affected packages for ignored path: {error}"));

		assert_eq!(evaluation.status, ChangesetPolicyStatus::NotRequired);
		assert_eq!(
			evaluation.ignored_paths,
			vec!["crates/core/tests/policy.rs".to_string()]
		);
		assert!(evaluation.affected_package_ids.is_empty());
		assert!(evaluation.comment.is_none());
	}

	#[test]
	fn compute_changed_paths_since_reports_git_failures_and_includes_untracked_files() {
		let failing_repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		git(failing_repo.path(), &["init", "-b", "main"]);
		let error = compute_changed_paths_since(failing_repo.path(), "definitely-missing-rev")
			.err()
			.unwrap_or_else(|| panic!("expected git diff failure"));
		assert!(
			error
				.to_string()
				.contains("git diff --name-only definitely-missing-rev failed")
		);

		let repo = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
		git(repo.path(), &["init", "-b", "main"]);
		git(repo.path(), &["config", "user.name", "monochange"]);
		git(
			repo.path(),
			&["config", "user.email", "monochange@example.com"],
		);
		fs::write(repo.path().join("tracked.txt"), "one\n")
			.unwrap_or_else(|error| panic!("write tracked file: {error}"));
		git(repo.path(), &["add", "tracked.txt"]);
		git(repo.path(), &["commit", "-m", "initial"]);
		fs::write(repo.path().join("tracked.txt"), "two\n")
			.unwrap_or_else(|error| panic!("update tracked file: {error}"));
		fs::write(repo.path().join("new.txt"), "new\n")
			.unwrap_or_else(|error| panic!("write untracked file: {error}"));

		let changed = compute_changed_paths_since(repo.path(), "HEAD")
			.unwrap_or_else(|error| panic!("compute changed paths: {error}"));
		assert_eq!(
			changed,
			vec!["new.txt".to_string(), "tracked.txt".to_string()]
		);

		let removed_root = {
			let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
			tempdir.path().to_path_buf()
		};
		let spawn_error = compute_changed_paths_since(&removed_root, "HEAD")
			.err()
			.unwrap_or_else(|| panic!("expected git spawn failure"));
		assert!(
			spawn_error
				.to_string()
				.contains("failed to run git diff --name-only HEAD")
		);
	}

	#[test]
	fn path_helpers_cover_normalization_matching_and_comment_rendering() {
		let package = sample_package();
		assert_eq!(
			normalize_changed_path("./crates\\core//src/lib.rs"),
			"crates/core//src/lib.rs"
		);
		assert!(is_changeset_markdown_path(".changeset/test.md"));
		assert!(!is_changeset_markdown_path(".changeset/test.txt"));
		assert!(path_touches_package("crates/core/src/lib.rs", &package));
		assert!(path_touches_package("shared/config.json", &package));
		assert!(path_is_ignored_for_package(
			"crates/core/README.md",
			&package
		));
		assert!(matches_any_package_pattern(
			"crates/core/docs/guide.md",
			&package,
			&package.ignored_paths
		));

		let verify = ChangesetAffectedSettings {
			enabled: true,
			required: true,
			comment_on_failure: true,
			skip_labels: vec!["release-notes-not-needed".to_string()],
			changed_paths: Vec::new(),
			ignored_paths: Vec::new(),
		};
		let evaluation = ChangesetPolicyEvaluation {
			status: ChangesetPolicyStatus::Failed,
			required: true,
			enforce: true,
			summary: "missing changesets".to_string(),
			comment: None,
			labels: vec!["release-notes-not-needed".to_string()],
			matched_skip_labels: Vec::new(),
			changed_paths: vec!["crates/core/src/lib.rs".to_string()],
			matched_paths: vec!["crates/core/src/lib.rs".to_string()],
			ignored_paths: vec!["crates/core/README.md".to_string()],
			changeset_paths: vec![".changeset/core.md".to_string()],
			affected_package_ids: vec!["core".to_string()],
			covered_package_ids: Vec::new(),
			uncovered_package_ids: vec!["core".to_string()],
			errors: vec!["core is uncovered".to_string()],
		};
		let comment = render_changeset_verification_comment(&verify, &evaluation);
		assert!(comment.contains("Changed package paths:"));
		assert!(comment.contains("Affected packages:"));
		assert!(comment.contains("Attached changeset files:"));
		assert!(comment.contains("Errors:"));
		assert!(comment.contains("Allowed skip labels:"));
		assert!(comment.contains("How to fix:"));
	}

	#[test]
	fn evaluate_and_verify_changesets_delegate_to_affected_packages() {
		let fixture = setup_fixture("monochange/changeset-policy-base");
		let changed_paths = vec!["crates/core/src/lib.rs".to_string()];
		let labels = vec!["no-changeset-required".to_string()];

		let affected = affected_packages(fixture.path(), &changed_paths, &labels)
			.unwrap_or_else(|error| panic!("affected packages: {error}"));
		let verified = verify_changesets(fixture.path(), &changed_paths, &labels)
			.unwrap_or_else(|error| panic!("verify changesets: {error}"));
		let evaluated = evaluate_changeset_policy(fixture.path(), &changed_paths, &labels)
			.unwrap_or_else(|error| panic!("evaluate changeset policy: {error}"));

		assert_eq!(verified, affected);
		assert_eq!(evaluated, affected);
	}

	#[test]
	fn render_comment_includes_related_skip_guidance() {
		let verify = ChangesetAffectedSettings {
			enabled: true,
			required: false,
			comment_on_failure: true,
			skip_labels: vec!["docs-only".to_string()],
			changed_paths: Vec::new(),
			ignored_paths: Vec::new(),
		};
		let evaluation = ChangesetPolicyEvaluation {
			status: ChangesetPolicyStatus::Failed,
			required: false,
			enforce: false,
			summary: "no coverage".to_string(),
			comment: None,
			labels: Vec::new(),
			matched_skip_labels: Vec::new(),
			changed_paths: Vec::new(),
			matched_paths: Vec::new(),
			ignored_paths: Vec::new(),
			changeset_paths: Vec::new(),
			affected_package_ids: Vec::new(),
			covered_package_ids: Vec::new(),
			uncovered_package_ids: Vec::new(),
			errors: vec!["problem".to_string()],
		};
		let comment = render_changeset_verification_comment(&verify, &evaluation);
		assert!(comment.contains("docs-only"));
		assert!(comment.contains("apply one of the configured skip labels"));
	}

	#[test]
	fn package_pattern_helpers_cover_root_relative_and_invalid_patterns() {
		let package = sample_package();

		assert!(path_is_within_package("crates/core", &package));
		assert!(!path_is_within_package("docs/readme.md", &package));
		assert!(path_is_ignored_for_package(
			"crates/core/docs/guide.md",
			&package
		));
		assert!(matches_any_package_pattern(
			"crates/core/README.md",
			&package,
			&package.ignored_paths
		));
		assert!(!matches_any_package_pattern(
			"crates/core/src/lib.rs",
			&package,
			&["[".to_string()]
		));
		assert!(path_matches_any_global_pattern(
			"docs/readme.md",
			&["docs/**".to_string()]
		));
		assert!(!path_matches_any_global_pattern(
			"docs/readme.md",
			&["[".to_string()]
		));
		assert!(!path_touches_package("docs/readme.md", &package));
	}

	#[test]
	fn affected_packages_uses_invalid_changeset_summary_when_only_changeset_inputs_fail() {
		let fixture = setup_fixture("monochange/changeset-policy-base");
		fs::create_dir_all(fixture.path().join(".changeset"))
			.unwrap_or_else(|error| panic!("create .changeset dir: {error}"));
		fs::write(fixture.path().join(".changeset/invalid.md"), "---\ncore:\n")
			.unwrap_or_else(|error| panic!("write invalid changeset: {error}"));

		let evaluation = affected_packages(
			fixture.path(),
			&[".changeset/invalid.md".to_string()],
			&Vec::new(),
		)
		.unwrap_or_else(|error| panic!("evaluate affected packages: {error}"));

		assert_eq!(evaluation.status, ChangesetPolicyStatus::Failed);
		assert_eq!(
			evaluation.summary,
			"changeset verification failed: one or more attached changeset files are invalid"
		);
		assert!(
			evaluation
				.errors
				.iter()
				.any(|error| error.contains("failed to parse"))
		);
	}
}
