use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::Method::POST;
use httpmock::MockServer;
use monochange_core::CommitMessage;
use monochange_core::ProviderBotSettings;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ReleaseOwnerKind;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_github::GitHubPullRequestOperation;
use monochange_github::GitHubPullRequestRequest;
use monochange_github::GitHubReleaseOperation;
use monochange_github::GitHubReleaseRequest;
use monochange_github::publish_release_pull_request;
use monochange_github::publish_release_requests;
use tempfile::tempdir;

#[test]
fn publish_release_requests_reads_github_env_configuration() {
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

	with_github_env(&server.base_url(), || {
		let github = sample_github_source();
		let outcomes = publish_release_requests(
			&github,
			&[GitHubReleaseRequest {
				provider: SourceProvider::GitHub,
				repository: "ifiokjr/monochange".to_string(),
				owner: "ifiokjr".to_string(),
				repo: "monochange".to_string(),
				target_id: "sdk".to_string(),
				target_kind: ReleaseOwnerKind::Group,
				tag_name: "v1.2.0".to_string(),
				name: "sdk 1.2.0".to_string(),
				body: Some("release body".to_string()),
				draft: false,
				prerelease: false,
				generate_release_notes: false,
			}],
		)
		.unwrap_or_else(|error| panic!("publish releases: {error}"));
		assert_eq!(outcomes[0].operation, GitHubReleaseOperation::Created);
	});

	release_lookup.assert();
	create_release.assert();
}

#[etest::etest(skip=std::env::var_os("PRE_COMMIT").is_some())]
fn publish_release_pull_request_uses_git_and_github_env_configuration() {
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
				"{\"number\":12,\"html_url\":\"https://example.com/pr/12\",\"node_id\":\"PR_node\"}",
			);
	});
	let add_labels = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/12/labels");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
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

	with_github_env(&server.base_url(), || {
		let github = sample_github_source();
		let outcome = publish_release_pull_request(
			&github,
			&repo,
			&GitHubPullRequestRequest {
				provider: SourceProvider::GitHub,
				repository: "ifiokjr/monochange".to_string(),
				owner: "ifiokjr".to_string(),
				repo: "monochange".to_string(),
				base_branch: "main".to_string(),
				head_branch: "monochange/release/release".to_string(),
				title: "chore(release): prepare release".to_string(),
				body: "release body".to_string(),
				labels: vec!["release".to_string()],
				auto_merge: false,
				commit_message: CommitMessage {
					subject: "chore(release): prepare release".to_string(),
					body: None,
				},
			},
			&[PathBuf::from("release.txt")],
		)
		.unwrap_or_else(|error| panic!("publish release pull request: {error}"));
		assert_eq!(outcome.operation, GitHubPullRequestOperation::Created);
	});

	list_pull_requests.assert();
	create_pull_request.assert();
	add_labels.assert();
}

fn sample_github_source() -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
		bot: ProviderBotSettings::default(),
	}
}

fn with_github_env<R>(base_url: &str, action: impl FnOnce() -> R) -> R {
	temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("test-token")),
			("GITHUB_API_URL", Some(base_url)),
		],
		action,
	)
}

fn git(root: &std::path::Path, args: &[&str]) {
	let status = std::process::Command::new("git")
		.current_dir(root)
		.args(args)
		.status()
		.unwrap_or_else(|error| panic!("git {args:?}: {error}"));
	assert!(status.success(), "git {args:?} failed");
}
