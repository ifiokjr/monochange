use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::Method::PUT;
use httpmock::MockServer;
use monochange_core::BotSettings;
use monochange_core::BumpSeverity;
use monochange_core::ChangeRequestSettings;
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
use monochange_core::SourceChangeRequestOperation;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::SourceReleaseOperation;
use monochange_core::VersionFormat;
use tempfile::tempdir;

use super::*;

#[test]
fn build_release_requests_uses_gitlab_provider() {
	let source = sample_source(None);
	let manifest = sample_manifest();
	let requests = build_release_requests(&source, &manifest);
	assert_eq!(requests.len(), 1);
	assert_eq!(
		requests
			.first()
			.unwrap_or_else(|| panic!("expected request"))
			.provider,
		SourceProvider::GitLab,
	);
}

#[test]
fn build_release_pull_request_request_uses_gitlab_provider_and_sanitized_branch() {
	let source = sample_source(None);
	let manifest = ReleaseManifest {
		command: "Release PR!".to_string(),
		..sample_manifest()
	};

	let request = build_release_pull_request_request(&source, &manifest);

	assert_eq!(request.provider, SourceProvider::GitLab);
	assert_eq!(request.repository, "group/monochange");
	assert_eq!(request.base_branch, "main");
	assert_eq!(request.head_branch, "monochange/release/release-pr");
	assert!(request.body.contains("## Prepared release"));
	assert!(request.body.contains("## Changed files"));
}

#[test]
fn gitlab_api_base_uses_api_url_or_host_defaults() {
	let explicit = sample_source(Some("https://gitlab.example.com/api/v4/".to_string()));
	assert_eq!(
		gitlab_api_base(&explicit).unwrap_or_else(|error| panic!("api base: {error}")),
		"https://gitlab.example.com/api/v4"
	);

	let custom_host = SourceConfiguration {
		host: Some("https://forge.example.com/".to_string()),
		api_url: None,
		..sample_source(None)
	};
	assert_eq!(
		gitlab_api_base(&custom_host).unwrap_or_else(|error| panic!("api base: {error}")),
		"https://forge.example.com/api/v4"
	);

	let default_host = SourceConfiguration {
		host: None,
		api_url: None,
		..sample_source(None)
	};
	assert_eq!(
		gitlab_api_base(&default_host).unwrap_or_else(|error| panic!("api base: {error}")),
		"https://gitlab.com/api/v4"
	);
}

#[test]
fn gitlab_token_supports_primary_and_fallback_environment_variables() {
	temp_env::with_vars([("GITLAB_TOKEN", Some("primary-token"))], || {
		assert_eq!(
			gitlab_token().unwrap_or_else(|error| panic!("token: {error}")),
			"primary-token"
		);
	});

	temp_env::with_vars(
		[("GITLAB_TOKEN", None), ("GL_TOKEN", Some("fallback-token"))],
		|| {
			assert_eq!(
				gitlab_token().unwrap_or_else(|error| panic!("token: {error}")),
				"fallback-token"
			);
		},
	);

	temp_env::with_vars(
		[
			("GITLAB_TOKEN", None::<String>),
			("GL_TOKEN", None::<String>),
		],
		|| {
			let error = gitlab_token()
				.err()
				.unwrap_or_else(|| panic!("expected missing token error"));
			assert!(error
				.to_string()
				.contains("set `GITLAB_TOKEN` (or `GL_TOKEN`) before running GitLab automation"));
		},
	);
}

#[test]
fn release_body_supports_generated_notes_and_minimal_fallback() {
	let generated_source = SourceConfiguration {
		releases: ReleaseProviderSettings {
			source: ReleaseNotesSource::GitHubGenerated,
			..ReleaseProviderSettings::default()
		},
		..sample_source(None)
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
	let body = release_body(&sample_source(None), &manifest, target)
		.unwrap_or_else(|| panic!("expected release body"));
	assert!(body.contains("Release target `sdk`"));
	assert!(body.contains("Members: core, app"));
	assert!(body.contains("- add provider automation"));
}

#[test]
fn release_pull_request_body_uses_minimal_notes_when_changelog_is_missing() {
	let manifest = sample_manifest_without_changelog();
	let body = release_pull_request_body(&manifest);

	assert!(body.contains("### sdk 1.2.0"));
	assert!(body.contains("Release target `sdk`"));
	assert!(body.contains("## Changed files"));
}

#[test]
fn publish_release_requests_creates_release_via_gitlab_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(404);
	});
	let create = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/group%2Fmonochange/releases");
		then.status(201)
			.header("content-type", "application/json")
			.body(
				"{\"web_url\":\"https://gitlab.example.com/group/monochange/-/releases/v1.2.0\"}",
			);
	});
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let manifest = sample_manifest();
	let requests = build_release_requests(&source, &manifest);
	let outcomes = with_gitlab_env(Some("token"), || {
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
fn publish_release_requests_updates_existing_release_via_gitlab_api() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"web_url\":\"https://gitlab.example.com/releases/v1.2.0\"}");
	});
	let update = server.mock(|when, then| {
		when.method(PATCH)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"web_url\":\"https://gitlab.example.com/releases/v1.2.0\"}");
	});
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let requests = build_release_requests(&source, &sample_manifest());

	let outcomes = with_gitlab_env(Some("token"), || {
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
fn publish_release_requests_reports_gitlab_api_errors() {
	let server = MockServer::start();
	let lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/releases/v1.2.0");
		then.status(500);
	});
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let requests = build_release_requests(&source, &sample_manifest());

	let error = with_gitlab_env(Some("token"), || {
		publish_release_requests(&source, &requests)
	})
	.err()
	.unwrap_or_else(|| panic!("expected publish error"));

	lookup.assert();
	assert!(error.to_string().contains("GitLab API GET"));
}

#[test]
fn publish_release_pull_request_creates_merge_request_and_pushes_branch() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests")
			.query_param("state", "opened")
			.query_param("source_branch", "monochange/release/release")
			.query_param("target_branch", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create = server.mock(|when, then| {
		when.method(POST)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests");
		then.status(201)
			.header("content-type", "application/json")
			.body(
				"{\"iid\":12,\"web_url\":\"https://gitlab.example.com/group/monochange/-/merge_requests/12\"}",
			);
	});
	let (_tempdir, repo) = seed_git_repository();
	let source = sample_source(Some(format!("{}/api/v4", server.base_url())));
	let request = build_release_pull_request_request(&source, &sample_manifest());

	let outcome = with_gitlab_env(Some("token"), || {
		publish_release_pull_request(&source, &repo, &request, &[PathBuf::from("release.txt")])
			.unwrap_or_else(|error| panic!("publish merge request: {error}"))
	});

	list.assert();
	create.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Created);
	assert!(!git_output(
		&repo,
		&["rev-parse", "--verify", "monochange/release/release"]
	)
	.trim()
	.is_empty());
}

#[test]
fn publish_merge_request_updates_existing_merge_request() {
	let server = MockServer::start();
	let list = server.mock(|when, then| {
		when.method(GET)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests")
			.query_param("state", "opened")
			.query_param("source_branch", "monochange/release/release")
			.query_param("target_branch", "main");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"[{\"iid\":12,\"web_url\":\"https://gitlab.example.com/group/monochange/-/merge_requests/12\"}]",
			);
	});
	let update = server.mock(|when, then| {
		when.method(PUT)
			.path("/api/v4/projects/group%2Fmonochange/merge_requests/12");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"iid\":12,\"web_url\":\"https://gitlab.example.com/group/monochange/-/merge_requests/12\"}",
			);
	});
	let request = build_release_pull_request_request(
		&sample_source(Some(format!("{}/api/v4", server.base_url()))),
		&sample_manifest(),
	);
	let client = gitlab_client().unwrap_or_else(|error| panic!("client: {error}"));

	let outcome = publish_merge_request(
		&client,
		"token",
		&format!("{}/api/v4", server.base_url()),
		&request,
	)
	.unwrap_or_else(|error| panic!("update merge request: {error}"));

	list.assert();
	update.assert();
	assert_eq!(outcome.operation, SourceChangeRequestOperation::Updated);
}

fn sample_source(api_url: Option<String>) -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitLab,
		owner: "group".to_string(),
		repo: "monochange".to_string(),
		host: Some("https://gitlab.com".to_string()),
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
			members: vec!["core".to_string(), "app".to_string()],
		}],
		released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
		changed_files: vec![PathBuf::from("Cargo.toml")],
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
					entries: vec!["add gitlab publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add gitlab publishing"
					.to_string(),
		}],
		deleted_changesets: Vec::new(),
		deployments: Vec::new(),
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

fn with_gitlab_env<R>(token: Option<&str>, action: impl FnOnce() -> R) -> R {
	temp_env::with_vars([("GITLAB_TOKEN", token), ("GL_TOKEN", None)], action)
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
	git(&repo, &["config", "user.name", "MonoChange Tests"]);
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
