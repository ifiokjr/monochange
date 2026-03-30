use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::MockServer;
use monochange_core::GitHubBotSettings;
use monochange_core::GitHubConfiguration;
use monochange_core::GitHubPullRequestSettings;
use monochange_core::GitHubReleaseNotesSource;
use monochange_core::GitHubReleaseSettings;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseOwnerKind;
use monochange_core::VersionFormat;
use tempfile::tempdir;

use super::*;

#[test]
fn build_release_requests_uses_matching_monochange_changelog_bodies() {
	let github = GitHubConfiguration {
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: GitHubReleaseSettings::default(),
		pull_requests: GitHubPullRequestSettings::default(),
		bot: GitHubBotSettings::default(),
	};
	let manifest = sample_manifest();

	let requests = build_release_requests(&github, &manifest);

	assert_eq!(requests.len(), 1);
	let request = requests
		.first()
		.unwrap_or_else(|| panic!("expected request"));
	assert_eq!(request.repository, "ifiokjr/monochange");
	assert_eq!(request.tag_name, "v1.2.0");
	assert_eq!(request.name, "sdk 1.2.0");
	assert_eq!(
		request.body.as_deref(),
		Some("## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add github publishing")
	);
	assert!(!request.generate_release_notes);
}

#[test]
fn build_release_requests_can_defer_to_github_generated_notes() {
	let github = GitHubConfiguration {
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: GitHubReleaseSettings {
			source: GitHubReleaseNotesSource::GitHubGenerated,
			generate_notes: true,
			..GitHubReleaseSettings::default()
		},
		pull_requests: GitHubPullRequestSettings::default(),
		bot: GitHubBotSettings::default(),
	};
	let manifest = sample_manifest();

	let requests = build_release_requests(&github, &manifest);

	assert_eq!(requests.len(), 1);
	let request = requests
		.first()
		.unwrap_or_else(|| panic!("expected request"));
	assert_eq!(request.body, None);
	assert!(request.generate_release_notes);
}

#[test]
fn build_release_requests_fall_back_to_minimal_release_bodies() {
	let github = GitHubConfiguration {
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: GitHubReleaseSettings::default(),
		pull_requests: GitHubPullRequestSettings::default(),
		bot: GitHubBotSettings::default(),
	};
	let manifest = ReleaseManifest {
		workflow: "release".to_string(),
		dry_run: true,
		version: None,
		group_version: None,
		release_targets: vec![ReleaseManifestTarget {
			id: "core".to_string(),
			kind: ReleaseOwnerKind::Package,
			version: "1.0.1".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
			tag_name: "core/v1.0.1".to_string(),
			members: vec!["cargo:crates/core/Cargo.toml".to_string()],
		}],
		released_packages: vec!["workflow-core".to_string()],
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		deployments: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![monochange_core::ReleaseManifestPlanDecision {
				package: "cargo:crates/core/Cargo.toml".to_string(),
				bump: monochange_core::BumpSeverity::Patch,
				trigger: "direct-change".to_string(),
				planned_version: Some("1.0.1".to_string()),
				reasons: vec!["fix race condition".to_string()],
				upstream_sources: vec!["cargo:crates/core/Cargo.toml".to_string()],
			}],
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	};

	let requests = build_release_requests(&github, &manifest);
	let request = requests
		.first()
		.unwrap_or_else(|| panic!("expected request"));

	assert_eq!(request.tag_name, "core/v1.0.1");
	assert!(request
		.body
		.as_deref()
		.unwrap_or_else(|| panic!("expected release body"))
		.contains("Release target `core`"));
	assert!(request
		.body
		.as_deref()
		.unwrap_or_else(|| panic!("expected release body"))
		.contains("- fix race condition"));
}

#[test]
fn build_release_pull_request_request_renders_branch_and_body() {
	let github = GitHubConfiguration {
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: GitHubReleaseSettings::default(),
		pull_requests: GitHubPullRequestSettings {
			branch_prefix: "automation/release".to_string(),
			base: "develop".to_string(),
			title: "chore(release): prepare release".to_string(),
			labels: vec!["release".to_string(), "automated".to_string()],
			auto_merge: true,
			..GitHubPullRequestSettings::default()
		},
		bot: GitHubBotSettings::default(),
	};
	let manifest = sample_manifest();

	let request = build_release_pull_request_request(&github, &manifest);

	assert_eq!(request.repository, "ifiokjr/monochange");
	assert_eq!(request.base_branch, "develop");
	assert_eq!(request.head_branch, "automation/release/release");
	assert_eq!(request.title, "chore(release): prepare release");
	assert_eq!(request.commit_message, request.title);
	assert_eq!(request.labels, vec!["release", "automated"]);
	assert!(request.auto_merge);
	assert!(request.body.contains("## Prepared release"));
	assert!(request.body.contains("### sdk 1.2.0"));
	assert!(request.body.contains("#### Features"));
	assert!(request.body.contains("- add github publishing"));
}

#[test]
fn publish_release_requests_creates_release_via_octocrab() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.0");
		then.status(404)
			.header("content-type", "application/json")
			.body("{\"message\":\"Not Found\"}");
	});
	let create_release = server.mock(|when, then| {
		when.method(POST).path("/repos/ifiokjr/monochange/releases");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/releases/1\"}");
	});
	let request = sample_release_request();

	let outcomes = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_requests_with_client(&client, &[request]).await;
			outcome
		})
		.unwrap_or_else(|error| panic!("publish release: {error}"));

	release_lookup.assert();
	create_release.assert();
	assert_eq!(outcomes.len(), 1);
	let outcome = outcomes
		.first()
		.unwrap_or_else(|| panic!("expected release outcome"));
	assert_eq!(outcome.operation, GitHubReleaseOperation::Created);
	assert_eq!(
		outcome.html_url.as_deref(),
		Some("https://example.com/releases/1")
	);
}

#[test]
fn publish_release_requests_updates_existing_release_via_octocrab() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.0");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"id\":42}");
	});
	let update_release = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/releases/42");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/releases/42\"}");
	});
	let request = sample_release_request();

	let outcomes = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_requests_with_client(&client, &[request]).await;
			outcome
		})
		.unwrap_or_else(|error| panic!("publish release: {error}"));

	release_lookup.assert();
	update_release.assert();
	let outcome = outcomes
		.first()
		.unwrap_or_else(|| panic!("expected release outcome"));
	assert_eq!(outcome.operation, GitHubReleaseOperation::Updated);
	assert_eq!(
		outcome.html_url.as_deref(),
		Some("https://example.com/releases/42")
	);
}

#[test]
fn publish_release_pull_request_creates_pull_request_via_octocrab() {
	let server = MockServer::start();
	let list_pull_requests = server.mock(|when, then| {
		when.method(GET).path("/repos/ifiokjr/monochange/pulls");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_pull_request = server.mock(|when, then| {
		when.method(POST).path("/repos/ifiokjr/monochange/pulls");
		then.status(201)
			.header("content-type", "application/json")
			.body(
				"{\"number\":7,\"html_url\":\"https://example.com/pr/7\",\"node_id\":\"PR_node\"}",
			);
	});
	let add_labels = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/7/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let request = sample_pull_request_request();

	let outcome = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_pull_request_with_client(&client, &request).await;
			outcome
		})
		.unwrap_or_else(|error| panic!("publish pull request: {error}"));

	list_pull_requests.assert();
	create_pull_request.assert();
	add_labels.assert();
	assert_eq!(outcome.operation, GitHubPullRequestOperation::Created);
	assert_eq!(outcome.number, 7);
	assert_eq!(outcome.url.as_deref(), Some("https://example.com/pr/7"));
}

#[test]
fn publish_release_pull_request_can_enable_auto_merge() {
	let server = MockServer::start();
	let list_pull_requests = server.mock(|when, then| {
		when.method(GET).path("/repos/ifiokjr/monochange/pulls");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_pull_request = server.mock(|when, then| {
		when.method(POST).path("/repos/ifiokjr/monochange/pulls");
		then.status(201)
			.header("content-type", "application/json")
			.body(
				"{\"number\":8,\"html_url\":\"https://example.com/pr/8\",\"node_id\":\"PR_node\"}",
			);
	});
	let add_labels = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/8/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let enable_auto_merge = server.mock(|when, then| {
		when.method(POST).path("/graphql");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"enablePullRequestAutoMerge\":{\"pullRequest\":{\"number\":8}}}");
	});
	let mut request = sample_pull_request_request();
	request.auto_merge = true;

	let outcome = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_pull_request_with_client(&client, &request).await;
			outcome
		})
		.unwrap_or_else(|error| panic!("publish pull request: {error}"));

	list_pull_requests.assert();
	create_pull_request.assert();
	add_labels.assert();
	enable_auto_merge.assert();
	assert_eq!(outcome.operation, GitHubPullRequestOperation::Created);
	assert_eq!(outcome.number, 8);
}

#[test]
fn publish_release_pull_request_updates_existing_pull_request_via_octocrab() {
	let server = MockServer::start();
	let list_pull_requests = server.mock(|when, then| {
		when.method(GET).path("/repos/ifiokjr/monochange/pulls");
		then.status(200)
			.header("content-type", "application/json")
			.body(
			"[{\"number\":9,\"html_url\":\"https://example.com/pr/9\",\"node_id\":\"PR_node\"}]",
		);
	});
	let update_pull_request = server.mock(|when, then| {
		when.method(PATCH).path("/repos/ifiokjr/monochange/pulls/9");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"number\":9,\"html_url\":\"https://example.com/pr/9\",\"node_id\":\"PR_node\"}",
			);
	});
	let add_labels = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/9/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let request = sample_pull_request_request();

	let outcome = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_pull_request_with_client(&client, &request).await;
			outcome
		})
		.unwrap_or_else(|error| panic!("publish pull request: {error}"));

	list_pull_requests.assert();
	update_pull_request.assert();
	add_labels.assert();
	assert_eq!(outcome.operation, GitHubPullRequestOperation::Updated);
	assert_eq!(outcome.number, 9);
}

#[test]
fn build_github_client_rejects_invalid_base_urls() {
	let error = build_github_client("token", Some("not a url"))
		.err()
		.unwrap_or_else(|| panic!("expected client error"));
	assert!(error
		.to_string()
		.contains("failed to configure GitHub base URL"));
}

#[test]
fn publish_release_requests_reports_github_api_errors() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.0");
		then.status(500)
			.header("content-type", "application/json")
			.body("{\"message\":\"boom\"}");
	});
	let request = sample_release_request();

	let error = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_requests_with_client(&client, &[request]).await;
			outcome
		})
		.err()
		.unwrap_or_else(|| panic!("expected GitHub API error"));

	assert!(release_lookup.calls() >= 1);
	assert!(error.to_string().contains("GitHub API GET"));
}

#[test]
fn publish_release_pull_request_reports_auto_merge_payload_errors() {
	let server = MockServer::start();
	let list_pull_requests = server.mock(|when, then| {
		when.method(GET).path("/repos/ifiokjr/monochange/pulls");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_pull_request = server.mock(|when, then| {
		when.method(POST).path("/repos/ifiokjr/monochange/pulls");
		then.status(201)
			.header("content-type", "application/json")
			.body(
			"{\"number\":13,\"html_url\":\"https://example.com/pr/13\",\"node_id\":\"PR_node\"}",
		);
	});
	let add_labels = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/13/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let enable_auto_merge = server.mock(|when, then| {
		when.method(POST).path("/graphql");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"enablePullRequestAutoMerge\":null}");
	});
	let mut request = sample_pull_request_request();
	request.auto_merge = true;

	let error = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcome = publish_release_pull_request_with_client(&client, &request).await;
			outcome
		})
		.err()
		.unwrap_or_else(|| panic!("expected auto merge error"));

	list_pull_requests.assert();
	create_pull_request.assert();
	add_labels.assert();
	enable_auto_merge.assert();
	assert!(error
		.to_string()
		.contains("auto merge returned no pull request payload"));
}

#[test]
fn git_helpers_prepare_commit_and_push_release_branch() {
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

	git_checkout_branch(&repo, "monochange/release/release")
		.unwrap_or_else(|error| panic!("checkout branch: {error}"));
	git_stage_paths(&repo, &[PathBuf::from("release.txt")])
		.unwrap_or_else(|error| panic!("stage paths: {error}"));
	git_commit_paths(&repo, "chore(release): prepare release")
		.unwrap_or_else(|error| panic!("commit paths: {error}"));
	git_push_branch(&repo, "monochange/release/release")
		.unwrap_or_else(|error| panic!("push branch: {error}"));

	let branch = git_output(
		&repo,
		&["rev-parse", "--verify", "monochange/release/release"],
	);
	assert!(!branch.trim().is_empty());
}

fn sample_release_request() -> GitHubReleaseRequest {
	GitHubReleaseRequest {
		repository: "ifiokjr/monochange".to_string(),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		target_id: "sdk".to_string(),
		target_kind: ReleaseOwnerKind::Group,
		tag_name: "v1.2.0".to_string(),
		name: "sdk 1.2.0".to_string(),
		body: Some(
			"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add github publishing"
				.to_string(),
		),
		draft: false,
		prerelease: false,
		generate_release_notes: false,
	}
}

fn sample_pull_request_request() -> GitHubPullRequestRequest {
	GitHubPullRequestRequest {
		repository: "ifiokjr/monochange".to_string(),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		base_branch: "main".to_string(),
		head_branch: "monochange/release/release".to_string(),
		title: "chore(release): prepare release".to_string(),
		body: "## Prepared release\n\n### sdk 1.2.0\n\n#### Features\n\n- add github publishing"
			.to_string(),
		labels: vec!["release".to_string(), "automated".to_string()],
		auto_merge: false,
		commit_message: "chore(release): prepare release".to_string(),
	}
}

fn build_test_client(server: &MockServer) -> Octocrab {
	build_github_client("test-token", Some(&server.base_url()))
		.unwrap_or_else(|error| panic!("octocrab client: {error}"))
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

fn sample_manifest() -> ReleaseManifest {
	ReleaseManifest {
		workflow: "release".to_string(),
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
			members: vec![
				"cargo:crates/core/Cargo.toml".to_string(),
				"cargo:crates/app/Cargo.toml".to_string(),
			],
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
					entries: vec!["- add github publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add github publishing"
					.to_string(),
		}],
		deleted_changesets: Vec::new(),
		deployments: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
	}
}
