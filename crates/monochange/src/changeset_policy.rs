use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::path::Path;
use std::process::Command as ProcessCommand;

use glob::Pattern;
use monochange_config::load_workspace_configuration;
use monochange_config::resolve_package_reference;
use monochange_core::ChangesetAffectedSettings;
use monochange_core::ChangesetPolicyEvaluation;
use monochange_core::ChangesetPolicyStatus;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PackageRecord;
use monochange_core::PublishState;
use monochange_core::SourceConfiguration;
use monochange_core::WorkspaceConfiguration;
use monochange_core::git::git_current_branch;

use crate::discover_workspace;

/// Evaluate pull-request changeset coverage for the supplied changed paths.
///
/// This is the library entry point behind `mc step:affected-packages` and the
/// GitHub changeset-policy workflow. It loads the workspace configuration, resolves
/// changed files against configured packages, reads any attached changesets, and
/// returns a structured pass/skip/fail report.
pub async fn affected_packages(
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
		current_branch_matches_pull_request_branch_prefix(root, configuration.source.as_ref()).await
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

	// Classify changed paths against package definitions
	let ignored_path_patterns = compile_patterns(&verify.ignored_paths);
	let changed_path_patterns = compile_patterns(&verify.changed_paths);
	let configured_changelog_paths =
		configured_changelog_paths(&configuration.packages, &configuration.groups);
	let package_matchers = configuration
		.packages
		.iter()
		.map(PackagePathMatcher::new)
		.collect::<Vec<_>>();

	let mut matched_paths = Vec::new();
	let mut ignored_paths = Vec::new();
	let mut affected_package_ids = BTreeSet::new();
	for path in changed_paths
		.iter()
		.filter(|path| !is_changeset_markdown_path(path))
	{
		if path_matches_compiled_patterns(path, &ignored_path_patterns) {
			ignored_paths.push(path.clone());
			continue;
		}

		if configured_changelog_paths.contains(path.as_str()) {
			ignored_paths.push(path.clone());
			continue;
		}

		if path_matches_compiled_patterns(path, &changed_path_patterns) {
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
		for matcher in &package_matchers {
			match matcher.classify(path) {
				PackagePathMatch::Touched => {
					matched_any_package = true;
					affected_package_ids.insert(matcher.package.id.clone());
				}
				PackagePathMatch::Ignored => {
					ignored_by_package = true;
				}
				PackagePathMatch::Unmatched => {}
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
	if !changeset_paths.is_empty() {
		let config_packages = configuration_package_records(&configuration);
		let config_ids_by_package_id = config_packages
			.iter()
			.map(|package| (package.id.clone(), package.id.clone()))
			.collect::<BTreeMap<_, _>>();
		if let Ok(config_covered_package_ids) = covered_package_ids_from_changesets(
			root,
			&configuration,
			&changeset_paths,
			&config_packages,
			&config_ids_by_package_id,
		) {
			covered_package_ids = config_covered_package_ids;
		} else {
			let discovery = discover_workspace(root)?;
			let config_ids_by_package_id = configuration
				.packages
				.iter()
				.map(|package| {
					resolve_package_reference(
						&package.id,
						&configuration.root_path,
						&discovery.packages,
					)
					.map(|package_id| (package_id, package.id.clone()))
				})
				.collect::<MonochangeResult<BTreeMap<_, _>>>()?;
			match covered_package_ids_from_changesets(
				root,
				&configuration,
				&changeset_paths,
				&discovery.packages,
				&config_ids_by_package_id,
			) {
				Ok(discovered_covered_package_ids) => {
					covered_package_ids = discovered_covered_package_ids;
				}
				Err(discovered_errors) => {
					errors.extend(discovered_errors);
				}
			}
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

async fn current_branch_matches_pull_request_branch_prefix(
	root: &Path,
	source: Option<&SourceConfiguration>,
) -> Option<(String, String)> {
	let source = source?;
	let branch_prefix = source.pull_requests.branch_prefix.trim();
	if branch_prefix.is_empty() {
		return None;
	}

	let current_branch = git_current_branch(root).await.ok()?;
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
pub async fn verify_changesets(
	root: &Path,
	changed_paths: &[String],
	labels: &[String],
) -> MonochangeResult<ChangesetPolicyEvaluation> {
	affected_packages(root, changed_paths, labels).await
}

/// Backwards-compatible alias for [`affected_packages`].
pub async fn evaluate_changeset_policy(
	root: &Path,
	changed_paths: &[String],
	labels: &[String],
) -> MonochangeResult<ChangesetPolicyEvaluation> {
	affected_packages(root, changed_paths, labels).await
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
	let mut paths: BTreeSet<String> = String::from_utf8_lossy(&diff_output.stdout)
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
			if !path.is_empty() {
				paths.insert(path);
			}
		}
	}

	Ok(paths.into_iter().collect())
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

struct PackagePathMatcher<'a> {
	package: &'a monochange_core::PackageDefinition,
	package_root: String,
	package_root_prefix: String,
	additional_patterns: Vec<Pattern>,
	ignored_patterns: Vec<Pattern>,
}

impl<'a> PackagePathMatcher<'a> {
	fn new(package: &'a monochange_core::PackageDefinition) -> Self {
		let package_root = normalize_changed_path(&package.path.to_string_lossy());
		let package_root_prefix = format!("{package_root}/");

		Self {
			package,
			package_root,
			package_root_prefix,
			additional_patterns: compile_patterns(&package.additional_paths),
			ignored_patterns: compile_patterns(&package.ignored_paths),
		}
	}

	fn classify(&self, path: &str) -> PackagePathMatch {
		let relative_path =
			package_relative_path(path, &self.package_root, &self.package_root_prefix);
		if matches_any_compiled_package_pattern(path, relative_path, &self.additional_patterns) {
			return PackagePathMatch::Touched;
		}
		if relative_path.is_none() {
			return PackagePathMatch::Unmatched;
		}
		if self.is_ignored(path) {
			return PackagePathMatch::Ignored;
		}
		PackagePathMatch::Touched
	}

	fn is_ignored(&self, path: &str) -> bool {
		let relative_path =
			package_relative_path(path, &self.package_root, &self.package_root_prefix);
		relative_path.is_some()
			&& matches_any_compiled_package_pattern(path, relative_path, &self.ignored_patterns)
	}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PackagePathMatch {
	Touched,
	Ignored,
	Unmatched,
}

fn covered_package_ids_from_changesets(
	root: &Path,
	configuration: &WorkspaceConfiguration,
	changeset_paths: &[String],
	packages: &[PackageRecord],
	config_ids_by_package_id: &BTreeMap<String, String>,
) -> Result<BTreeSet<String>, Vec<String>> {
	let changeset_load_context =
		monochange_config::build_changeset_load_context(configuration, packages);
	let mut covered_package_ids = BTreeSet::new();
	let mut errors = Vec::new();

	for changeset_path in changeset_paths {
		let absolute_path = root.join(changeset_path);
		if !absolute_path.exists() {
			errors.push(format!(
				"attached changeset `{changeset_path}` does not exist in the checked-out workspace"
			));
			continue;
		}
		match monochange_config::load_changeset_file_with_context(
			&absolute_path,
			&changeset_load_context,
		) {
			Ok(loaded) => {
				for signal in loaded.signals {
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

	if errors.is_empty() {
		Ok(covered_package_ids)
	} else {
		Err(errors)
	}
}

fn configuration_package_records(configuration: &WorkspaceConfiguration) -> Vec<PackageRecord> {
	configuration
		.packages
		.iter()
		.map(|package| {
			let mut metadata = BTreeMap::new();
			metadata.insert("config_id".to_string(), package.id.clone());

			PackageRecord {
				id: package.id.clone(),
				name: package.id.clone(),
				ecosystem: package.package_type.into(),
				manifest_path: configuration
					.root_path
					.join(&package.path)
					.join(".monochange-config-package"),
				workspace_root: configuration.root_path.clone(),
				current_version: None,
				publish_state: PublishState::Unpublished,
				version_group_id: None,
				metadata,
				declared_dependencies: Vec::new(),
			}
		})
		.collect()
}

fn compile_patterns(patterns: &[String]) -> Vec<Pattern> {
	patterns
		.iter()
		.filter_map(|pattern| Pattern::new(pattern).ok())
		.collect()
}

fn path_matches_compiled_patterns(path: &str, patterns: &[Pattern]) -> bool {
	patterns.iter().any(|pattern| pattern.matches(path))
}

fn configured_changelog_paths(
	packages: &[monochange_core::PackageDefinition],
	groups: &[monochange_core::GroupDefinition],
) -> BTreeSet<String> {
	packages
		.iter()
		.filter_map(|package| package.changelog.as_ref())
		.map(|target| normalize_changed_path(&target.path.to_string_lossy()))
		.chain(
			groups
				.iter()
				.filter_map(|group| group.changelog.as_ref())
				.map(|target| normalize_changed_path(&target.path.to_string_lossy())),
		)
		.collect()
}

fn package_relative_path<'path>(
	path: &'path str,
	package_root: &str,
	package_root_prefix: &str,
) -> Option<&'path str> {
	path.strip_prefix(package_root_prefix)
		.or_else(|| (path == package_root).then_some(""))
}

fn matches_any_compiled_package_pattern(
	path: &str,
	relative_path: Option<&str>,
	patterns: &[Pattern],
) -> bool {
	patterns.iter().any(|pattern| {
		pattern.matches(path)
			|| relative_path.is_some_and(|relative_path| pattern.matches(relative_path))
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
#[path = "__tests__/changeset_policy_tests.rs"]
mod tests;
