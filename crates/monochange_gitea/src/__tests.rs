use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::MockServer;
use insta::assert_snapshot;
use monochange_core::BotSettings;
use monochange_core::BumpSeverity;
use monochange_core::ChangeRequestSettings;
use monochange_core::CommitMessage;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestPlanDecision;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseNotesSource;
use monochange_core::ReleaseOwnerKind;
use monochange_core::ReleaseProviderSettings;
use monochange_core::SourceCapabilities;
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::VersionFormat;
use tempfile::tempdir;

use super::*;

#[test]
fn build_release_requests_uses_gitea_provider() {
	let source = sample_source(None, Some("https://codeberg.org".to_string()));
	let manifest = sample_manifest();
	let requests = build_release_requests(&source, &manifest);
	assert_eq!(requests.len(), 1);
	assert_eq!(
		requests
			.first()
			.unwrap_or_else(|| panic!("expected request"))
			.provider,
		SourceProvider::Gitea,
	);
}

#[test]
fn build_release_pull_request_request_uses_gitea_provider_and_sanitized_branch() {
	let source = sample_source(None, Some("https://codeberg.org".to_string()));
	let manifest = ReleaseManifest {
		command: "Release PR!".to_string(),
		..sample_manifest()
	};

	let request = build_release_pull_request_request(&source, &manifest);

	assert_eq!(request.provider, SourceProvider::Gitea);
	assert_eq!(request.repository, "org/monochange");
	assert_eq!(request.base_branch, "main");
	assert_eq!(request.head_branch, "monochange/release/release-pr");
	assert_snapshot!(
		"build_release_pull_request_request_uses_gitea_provider_and_sanitized_branch__body",
		request.body
	);
}

#[test]
fn gitea_source_capabilities_capture_provider_limits() {
	assert_eq!(
		source_capabilities(),
		SourceCapabilities {
			draft_releases: true,
			prereleases: true,
			generated_release_notes: false,
			auto_merge_change_requests: false,
			released_issue_comments: false,
			requires_host: true,
		}
	);
}

#[test]
fn validate_source_configuration_rejects_missing_host_and_unsupported_features() {
	let error = validate_source_configuration(&sample_source(None, None))
		.err()
		.unwrap_or_else(|| panic!("expected validation error"));
	assert!(error
		.to_string()
		.contains("[source].host must be set for `provider = \"gitea\"`"));

	let error = validate_source_configuration(&SourceConfiguration {
		pull_requests: ChangeRequestSettings {
			auto_merge: true,
			..ChangeRequestSettings::default()
		},
		..sample_source(None, Some("https://codeberg.org".to_string()))
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(error
		.to_string()
		.contains("[source.pull_requests].auto_merge is not supported"));

	let error = validate_source_configuration(&SourceConfiguration {
		releases: ReleaseProviderSettings {
			source: ReleaseNotesSource::GitHubGenerated,
			..ReleaseProviderSettings::default()
		},
		..sample_source(None, Some("https://codeberg.org".to_string()))
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(error
		.to_string()
		.contains("provider-generated release notes are not supported"));
}

#[test]
fn gitea_api_base_requires_host_unless_api_url_is_set() {
	let explicit = sample_source(
		Some("https://codeberg.example.com/api/v1/".to_string()),
		Some("https://codeberg.org".to_string()),
	);
	assert_eq!(
		gitea_api_base(&explicit).unwrap_or_else(|error| panic!("api base: {error}")),
		"https://codeberg.example.com/api/v1"
	);

	let host_only = sample_source(None, Some("https://codeberg.org/".to_string()));
	assert_eq!(
		gitea_api_base(&host_only).unwrap_or_else(|error| panic!("api base: {error}")),
		"https://codeberg.org/api/v1"
	);

	let error = gitea_api_base(&sample_source(None, None))
		.err()
		.unwrap_or_else(|| panic!("expected missing host error"));
	assert!(error
		.to_string()
		.contains("[source].host must be set for `provider = \"gitea\"`"));
}

#[test]
fn gitea_token_requires_environment_variable() {
	temp_env::with_vars([("GITEA_TOKEN", Some("token"))], || {
		assert_eq!(
			gitea_token().unwrap_or_else(|error| panic!("token: {error}")),
			"token"
		);
	});

	temp_env::with_vars([("GITEA_TOKEN", None::<String>)], || {
		let error = gitea_token()
			.err()
			.unwrap_or_else(|| panic!("expected missing token error"));
		assert!(error
			.to_string()
			.contains("set `GITEA_TOKEN` before running Gitea automation"));
	});
}

#[test]
fn release_body_supports_generated_notes_and_minimal_fallback() {
	let generated_source = SourceConfiguration {
		releases: ReleaseProviderSettings {
			source: ReleaseNotesSource::GitHubGenerated,
			..ReleaseProviderSettings::default()
		},
		..sample_source(None, Some("https://codeberg.org".to_string()))
	};
	let manifest = sample_manifest();
	let target = manifest
		.release_targets
		.first()
		.unwrap_or_else(|| panic!("expected release target"));
	assert_eq!(release_body(&generated_source, &manifest, target), None);

	let manifest = sample_manifest_without_changelog();
	let target = manifest
		.release_targets
		.first()
		.unwrap_or_else(|| panic!("expected release target"));
	let body = release_body(
		&sample_source(None, Some("https://codeberg.org".to_string())),
		&manifest,
		target,
	)
	.unwrap_or_else(|| panic!("expected release body"));
	assert_snapshot!(
		"release_body_supports_generated_notes_and_minimal_fallback__minimal_body",
		body
	);
}

#[test]
fn release_pull_request_body_uses_minimal_notes_when_changelog_is_missing() {
	let manifest = sample_manifest_without_changelog();
	let body = release_pull_request_body(&manifest);

	assert_snapshot!(
		"release_pull_request_body_uses_minimal_notes_when_changelog_is_missing__body",
		body
	);
}

#[test]
fn publish_release_requests_creates_release_via_gitea_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(404);
	});
	let create = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v1/repos/org/monochange/releases");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://codeberg.org/org/monochange/releases/tag/v1.2.0\"}");
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_gitea_env(Some("token"), || {
		publish_release_requests(&source, &requests)
			.unwrap_or_else(|error| panic!("publish release: {error}"))
	});

	lookup.assert();
	create.assert();
	assert_eq!(
		outcomes
			.first()
			.unwrap_or_else(|| panic!("expected outcome"))
			.operation,
		SourceReleaseOperation::Created,
	);
}

#[test]
fn publish_release_requests_updates_existing_release_via_gitea_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://codeberg.org/org/monochange/releases/tag/v1.2.0\"}");
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://codeberg.org/org/monochange/releases/tag/v1.2.0\"}");
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_gitea_env(Some("token"), || {
		publish_release_requests(&source, &requests)
			.unwrap_or_else(|error| panic!("publish release: {error}"))
	});

	lookup.assert();
	update.assert();
	assert_eq!(
		outcomes
			.first()
			.unwrap_or_else(|| panic!("expected outcome"))
			.operation,
		SourceReleaseOperation::Updated,
	);
}

#[test]
fn publish_release_requests_reports_gitea_api_errors() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/releases/tags/v1.2.0");
		then.status(500);
	});
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let requests = build_release_requests(&source, &sample_manifest());

	let error = with_gitea_env(Some("token"), || {
		publish_release_requests(&source, &requests)
	})
	.err()
	.unwrap_or_else(|| panic!("expected publish error"));

	lookup.assert();
	assert!(error.to_string().contains("Gitea API GET"));
}

#[test]
fn publish_release_pull_request_creates_pull_request_and_labels() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/pulls")
			.query_param("state", "open")
			.query_param("head", "org:monochange/release/release")
			.query_param("base", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create = server.mock(|when, then| {
		when.method(POST).path("/api/v1/repos/org/monochange/pulls");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\"}");
	});
	let labels = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v1/repos/org/monochange/issues/12/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let (_tempdir, repo) = seed_git_repository();
	let source = sample_source(
		Some(format!("{}/api/v1", server.base_url())),
		Some("https://codeberg.org".to_string()),
	);
	let mut request = build_release_pull_request_request(&source, &sample_manifest());
	request.commit_message.body = Some("release body".to_string());

	let outcome = with_gitea_env(Some("token"), || {
		publish_release_pull_request(&source, &repo, &request, &[PathBuf::from("release.txt")])
			.unwrap_or_else(|error| panic!("publish pull request: {error}"))
	});

	list.assert();
	create.assert();
	labels.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Created);
	assert!(!git_output(
		&repo,
		&["rev-parse", "--verify", "monochange/release/release"]
	)
	.trim()
	.is_empty());
	let commit_body = git_output(&repo, &["log", "-1", "--pretty=%B"]);
	assert!(commit_body.contains("release body"));
}

#[test]
fn git_commit_paths_reports_io_and_non_noop_failures() {
	if env::var_os("PRE_COMMIT").is_some() {
		return;
	}

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = tempdir.path().join("missing");
	let io_error = git_commit_paths(
		&missing,
		&CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing worktree error"));
	assert!(io_error
		.to_string()
		.contains("failed to commit release pull request changes"));

	let repo = tempdir.path().join("repo-error");
	git(tempdir.path(), &["init", repo.to_string_lossy().as_ref()]);
	git(&repo, &["config", "user.name", "monochange Tests"]);
	git(&repo, &["config", "user.email", "monochange@example.com"]);
	let hooks_dir = repo.join(".git/hooks");
	std::fs::write(hooks_dir.join("pre-commit"), "#!/bin/sh\nexit 1\n")
		.unwrap_or_else(|error| panic!("write hook: {error}"));
	std::fs::set_permissions(
		hooks_dir.join("pre-commit"),
		std::os::unix::fs::PermissionsExt::from_mode(0o755),
	)
	.unwrap_or_else(|error| panic!("chmod hook: {error}"));
	std::fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(&repo, &["add", "release.txt"]);
	let error = git_commit_paths(
		&repo,
		&CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
	)
	.err()
	.unwrap_or_else(|| panic!("expected pre-commit hook failure"));
	assert!(error
		.to_string()
		.contains("failed to commit release pull request changes"));
}

#[test]
fn git_commit_paths_treats_clean_worktrees_as_already_committed() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path().join("repo");
	git(tempdir.path(), &["init", repo.to_string_lossy().as_ref()]);
	git(&repo, &["config", "user.name", "monochange Tests"]);
	git(&repo, &["config", "user.email", "monochange@example.com"]);
	std::fs::write(repo.join("release.txt"), "initial\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(&repo, &["add", "release.txt"]);
	git(&repo, &["commit", "-m", "initial"]);

	git_commit_paths(
		&repo,
		&CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
	)
	.unwrap_or_else(|error| panic!("commit paths: {error}"));

	assert_eq!(
		git_output(&repo, &["rev-list", "--count", "HEAD"]).trim(),
		"1"
	);
}

#[test]
fn publish_pull_request_updates_existing_pull_request() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v1/repos/org/monochange/pulls")
			.query_param("state", "open")
			.query_param("head", "org:monochange/release/release")
			.query_param("base", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"[{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\"}]",
			);
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v1/repos/org/monochange/pulls/12");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"number\":12,\"html_url\":\"https://codeberg.org/org/monochange/pulls/12\"}");
	});
	let labels = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v1/repos/org/monochange/issues/12/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let request = build_release_pull_request_request(
		&sample_source(
			Some(format!("{}/api/v1", server.base_url())),
			Some("https://codeberg.org".to_string()),
		),
		&sample_manifest(),
	);
	let client = gitea_client().unwrap_or_else(|error| panic!("client: {error}"));

	let outcome = publish_pull_request(
		&client,
		"token",
		&format!("{}/api/v1", server.base_url()),
		&request,
	)
	.unwrap_or_else(|error| panic!("update pull request: {error}"));

	list.assert();
	update.assert();
	labels.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Updated);
}

fn sample_source(api_url: Option<String>, host: Option<String>) -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::Gitea,
		owner: "org".to_string(),
		repo: "monochange".to_string(),
		host,
		api_url,
		releases: ReleaseProviderSettings::default(),
		pull_requests: ChangeRequestSettings::default(),
		bot: BotSettings::default(),
	}
}

fn sample_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: true,
		version: Some("1.2.0".to_string()),
		group_version: Some("1.2.0".to_string()),
		release_targets: vec![ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.2.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.2.0".to_string(),
			rendered_title: "test title".to_string(),
			rendered_changelog_title: "test changelog title".to_string(),
			members: vec!["core".to_string(), "app".to_string()],
		}],
		released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changesets: Vec::new(),
		changelogs: vec![ReleaseManifestChangelog {
			owner_id: "sdk".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			path: PathBuf::from("changelog.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "1.2.0".to_string(),
				summary: vec!["Grouped release for `sdk`.".to_string()],
				sections: vec![ReleaseNotesSection {
					title: "Features".to_string(),
					entries: vec!["add gitea publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add gitea publishing"
					.to_string(),
		}],
		deleted_changesets: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![ReleaseManifestPlanDecision {
				package: "core".to_string(),
				bump: BumpSeverity::Minor,
				trigger: "changeset".to_string(),
				planned_version: Some("1.2.0".to_string()),
				reasons: vec!["add provider automation".to_string()],
				upstream_sources: vec!["sdk".to_string()],
			}],
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}

fn sample_manifest_without_changelog() -> ReleaseManifest {
	let mut manifest = sample_manifest();
	manifest.changelogs.clear();
	manifest
}

fn with_gitea_env<R>(token: Option<&str>, action: impl FnOnce() -> R) -> R {
	temp_env::with_vars([("GITEA_TOKEN", token)], action)
}

fn seed_git_repository() -> (tempfile::TempDir, PathBuf) {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let bare = tempdir.path().join("origin.git");
	let repo = tempdir.path().join("repo");
	git(
		tempdir.path(),
		&["init", "--bare", bare.to_string_lossy().as_ref()],
	);
	git(tempdir.path(), &["init", repo.to_string_lossy().as_ref()]);
	git(&repo, &["config", "user.name", "monochange Tests"]);
	git(&repo, &["config", "user.email", "monochange@example.com"]);
	std::fs::write(repo.join("release.txt"), "before\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(&repo, &["add", "release.txt"]);
	git(&repo, &["commit", "-m", "initial"]);
	git(&repo, &["branch", "-M", "main"]);
	git(
		&repo,
		&["remote", "add", "origin", bare.to_string_lossy().as_ref()],
	);
	git(&repo, &["push", "-u", "origin", "main"]);
	std::fs::write(repo.join("release.txt"), "after\n")
		.unwrap_or_else(|error| panic!("update release file: {error}"));
	(tempdir, repo)
}

fn git(root: &Path, args: &[&str]) {
	let status = Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}

fn git_output(root: &Path, args: &[&str]) -> String {
	let output = Command::new("git")
		.current_dir(root)
		.args(args)
		.output()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(output.status.success(), "git {args:?} failed");
	String::from_utf8(output.stdout).unwrap_or_else(|error| panic!("utf8: {error}"))
}
