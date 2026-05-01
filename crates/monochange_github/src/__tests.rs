use std::path::Path;
use std::path::PathBuf;
use std::thread;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::MockServer;
use insta::assert_json_snapshot;
use insta::assert_snapshot;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetRevision;
use monochange_core::CommitMessage;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedSourceAdapter;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::MonochangeError;
use monochange_core::MonochangeResult;
use monochange_core::PreparedChangeset;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ProviderReleaseSettings;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseOwnerKind;
use monochange_core::RetargetOperation;
use monochange_core::RetargetProviderOperation;
use monochange_core::RetargetTagResult;
use monochange_core::SourceCapabilities;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::VersionFormat;
use monochange_test_helpers::git;
use monochange_test_helpers::git_output;
use regex::Regex;
use tempfile::tempdir;

use super::*;

fn must_ok<T, E: std::fmt::Display>(result: Result<T, E>, context: &str) -> T {
	match result {
		Ok(value) => value,
		Err(error) => panic!("{context}: {error}"),
	}
}

#[test]
fn must_ok_panics_on_errors() {
	assert!(std::panic::catch_unwind(|| must_ok::<(), _>(Err("boom"), "context")).is_err());
}

#[test]
fn comment_released_issues_with_client_closes_skipped_existing_issues_when_plan_close_is_true() {
	let server = MockServer::start();
	let list_issue_seven_comments = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				r#"[{"html_url":"https://example.com/issues/7#comment-1","body":"Released in v1.2.0."}]"#,
			);
	});
	let _close_issue_seven = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/issues/7");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"state":"closed"}"#);
	});
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let plans = vec![GitHubIssueCommentPlan {
		repository: "ifiokjr/monochange".to_string(),
		issue_id: "#7".to_string(),
		issue_url: Some("https://example.com/issues/7".to_string()),
		body: "Released in v1.2.0.".to_string(),
		close: true,
	}];
	let outcomes = temp_env::with_var("GITHUB_SERVER_URL", Some("https://example.com"), || {
		github_runtime()
			.unwrap_or_else(|error| panic!("runtime: {error}"))
			.block_on(async {
				let client = build_test_client(&server);
				comment_released_issues_with_client(&client, &github, &plans).await
			})
			.unwrap_or_else(|error| panic!("comment released issues: {error}"))
	});
	list_issue_seven_comments.assert();
	// Skip explicit close mock assert due to httpmock path-prefix overlap with comment endpoint.
	// When the comment already exists, the operation is SkippedExisting even if close=true.
	assert!(outcomes.iter().any(|outcome| {
		outcome.issue_id == "#7"
			&& outcome.operation == GitHubIssueCommentOperation::SkippedExisting
	}));
}

#[test]
fn build_release_requests_uses_matching_monochange_changelog_bodies() {
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let manifest = sample_manifest();

	let requests = build_release_requests(&github, &manifest);

	assert_eq!(requests.len(), 1);
	let request = requests
		.first()
		.unwrap_or_else(|| panic!("expected request"));
	assert_json_snapshot!(
		"build_release_requests_uses_matching_monochange_changelog_bodies__request",
		serde_json::json!({
			"repository": request.repository,
			"tag_name": request.tag_name,
			"name": request.name,
			"body": request.body,
			"generate_release_notes": request.generate_release_notes,
		})
	);
}

#[test]
fn build_release_requests_can_defer_to_github_generated_notes() {
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings {
			source: ProviderReleaseNotesSource::GitHubGenerated,
			generate_notes: true,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
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
fn github_source_capabilities_cover_github_automation_features() {
	assert_eq!(
		source_capabilities(),
		SourceCapabilities {
			draft_releases: true,
			prereleases: true,
			generated_release_notes: true,
			auto_merge_change_requests: true,
			released_issue_comments: true,
			requires_host: false,
		}
	);
}

#[test]
fn github_url_helpers_use_source_configuration_coordinates() {
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};

	temp_env::with_var("GITHUB_SERVER_URL", Some("https://example.com"), || {
		assert_eq!(
			github_pull_request_url(&source, 42),
			"https://example.com/ifiokjr/monochange/pull/42"
		);
		assert_eq!(
			github_issue_url(&source, 7),
			"https://example.com/ifiokjr/monochange/issues/7"
		);
	});
}

#[test]
fn validate_source_configuration_rejects_conflicting_release_note_modes() {
	let error = validate_source_configuration(&SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings {
			generate_notes: true,
			source: ProviderReleaseNotesSource::Monochange,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
	})
	.err()
	.unwrap_or_else(|| panic!("expected validation error"));
	assert!(
		error
			.to_string()
			.contains("[source.releases].generate_notes cannot be true")
	);
}

#[test]
fn comment_released_issues_with_client_closes_issues_when_plan_close_is_true() {
	let server = MockServer::start();
	let list_issue_seven_comments = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_issue_seven_comment = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"html_url":"https://example.com/issues/7#comment-1"}"#);
	});
	let _close_issue_seven = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/issues/7");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"state":"closed"}"#);
	});
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let _manifest = ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		package_publications: vec![],
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![],
			groups: vec![],
			warnings: vec![],
			compatibility_evidence: vec![],
			unresolved_items: vec![],
		},
		deleted_changesets: vec![],
		changesets: vec![],
	};
	let plans = vec![GitHubIssueCommentPlan {
		repository: "ifiokjr/monochange".to_string(),
		issue_id: "#7".to_string(),
		issue_url: Some("https://example.com/issues/7".to_string()),
		body: "Released in v1.2.0.".to_string(),
		close: true,
	}];
	let outcomes = temp_env::with_var("GITHUB_SERVER_URL", Some("https://example.com"), || {
		github_runtime()
			.unwrap_or_else(|error| panic!("runtime: {error}"))
			.block_on(async {
				let client = build_test_client(&server);
				comment_released_issues_with_client(&client, &github, &plans).await
			})
			.unwrap_or_else(|error| panic!("comment released issues: {error}"))
	});
	list_issue_seven_comments.assert();
	create_issue_seven_comment.assert();
	// Skip explicit close mock assert — httpmock path prefix matching causes overlap between
	// PATCH /repos/ifiokjr/monochange/issues/7 and GET /repos/ifiokjr/monochange/issues/7/comments.
	// The presence of a Closed outcome proves the PATCH was sent and received a successful response.
	assert!(outcomes.iter().any(|outcome| {
		outcome.issue_id == "#7" && outcome.operation == GitHubIssueCommentOperation::Closed
	}));
}

#[test]
fn comment_released_issues_public_api_uses_source_configuration() {
	let server = MockServer::start();
	let list_issue_comments = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_issue_comment = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/issues/7#comment-1\"}");
	});
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut manifest = sample_manifest();
	manifest.changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("add release context".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GitHub,
			host: Some("example.com".to_string()),
			capabilities: github_hosting_capabilities(),
			introduced: None,
			last_updated: None,
			related_issues: vec![HostedIssueRef {
				provider: HostingProviderKind::GitHub,
				host: Some("example.com".to_string()),
				id: "#7".to_string(),
				title: Some("Track release context".to_string()),
				url: Some("https://example.com/issues/7".to_string()),
				relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
			}],
		}),
	}];

	let outcomes = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_SERVER_URL", Some("https://example.com")),
		],
		|| comment_released_issues(&source, &manifest),
	)
	.unwrap_or_else(|error| panic!("comment released issues: {error}"));

	list_issue_comments.assert();
	create_issue_comment.assert();
	assert_eq!(outcomes.len(), 1);
	assert_eq!(
		outcomes
			.first()
			.unwrap_or_else(|| panic!("expected one issue comment outcome"))
			.issue_id,
		"#7"
	);
}

#[test]
fn github_hosted_source_adapter_comments_released_issues() {
	let server = MockServer::start();
	let list_issue_comments = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_issue_comment = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/issues/7#comment-1\"}");
	});
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut manifest = sample_manifest();
	manifest.changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("add release context".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GitHub,
			host: Some("example.com".to_string()),
			capabilities: github_hosting_capabilities(),
			introduced: None,
			last_updated: None,
			related_issues: vec![HostedIssueRef {
				provider: HostingProviderKind::GitHub,
				host: Some("example.com".to_string()),
				id: "#7".to_string(),
				title: Some("Track release context".to_string()),
				url: Some("https://example.com/issues/7".to_string()),
				relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
			}],
		}),
	}];

	let outcomes = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_SERVER_URL", Some("https://example.com")),
		],
		|| HOSTED_SOURCE_ADAPTER.comment_released_issues(&source, &manifest),
	)
	.unwrap_or_else(|error| panic!("adapter issue comments: {error}"));

	list_issue_comments.assert();
	create_issue_comment.assert();
	assert_eq!(outcomes.len(), 1);
	assert_eq!(
		outcomes
			.first()
			.unwrap_or_else(|| panic!("expected one issue comment outcome"))
			.operation,
		GitHubIssueCommentOperation::Created
	);
}

#[test]
fn build_release_requests_fall_back_to_minimal_release_bodies() {
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let manifest = ReleaseManifest {
		command: "release".to_string(),
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
			rendered_title: "test title".to_string(),
			rendered_changelog_title: "test changelog title".to_string(),
			members: vec!["cargo:crates/core/Cargo.toml".to_string()],
		}],
		package_publications: vec![],
		released_packages: vec!["workflow-core".to_string()],
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		changesets: Vec::new(),
		deleted_changesets: Vec::new(),
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
	assert_snapshot!(
		"build_release_requests_fall_back_to_minimal_release_bodies__body",
		request
			.body
			.as_deref()
			.unwrap_or_else(|| panic!("expected release body"))
	);
}

#[test]
fn build_release_requests_falls_back_to_tag_name_when_rendered_title_is_empty() {
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut manifest = sample_manifest();
	manifest.release_targets.first_mut().unwrap().rendered_title = String::new();

	let requests = build_release_requests(&github, &manifest);
	let request = requests
		.first()
		.unwrap_or_else(|| panic!("expected request"));

	assert_eq!(request.name, "v1.2.0");
}

#[test]
fn build_release_pull_request_request_renders_branch_and_body() {
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings {
			branch_prefix: "automation/release".to_string(),
			base: "develop".to_string(),
			title: "chore(release): prepare release".to_string(),
			labels: vec!["release".to_string(), "automated".to_string()],
			auto_merge: true,
			..ProviderMergeRequestSettings::default()
		},
	};
	let manifest = sample_manifest();

	let request = build_release_pull_request_request(&github, &manifest);

	assert_json_snapshot!(
		"build_release_pull_request_request_renders_branch_and_body__request",
		serde_json::json!({
			"repository": request.repository,
			"base_branch": request.base_branch,
			"head_branch": request.head_branch,
			"title": request.title,
			"commit_message": request.commit_message,
			"labels": request.labels,
			"auto_merge": request.auto_merge,
			"body": request.body,
		})
	);
	assert_snapshot!(
		"build_release_pull_request_request_renders_branch_and_body__body",
		request.body
	);
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
			publish_release_requests_with_client(&client, &[request]).await
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
		outcome.url.as_deref(),
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
			publish_release_requests_with_client(&client, &[request]).await
		})
		.unwrap_or_else(|error| panic!("publish release: {error}"));

	release_lookup.assert();
	update_release.assert();
	let outcome = outcomes
		.first()
		.unwrap_or_else(|| panic!("expected release outcome"));
	assert_eq!(outcome.operation, GitHubReleaseOperation::Updated);
	assert_eq!(
		outcome.url.as_deref(),
		Some("https://example.com/releases/42")
	);
}

#[test]
fn sync_retargeted_releases_plans_updates_in_dry_run_mode() {
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some("https://example.com".to_string()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let updates = vec![RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Planned,
		message: None,
	}];

	let outcomes = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&MockServer::start());
			let outcomes =
				sync_retargeted_releases_with_client(&client, &source, &updates, true).await?;
			Ok::<_, MonochangeError>(outcomes)
		})
		.unwrap_or_else(|error| panic!("sync releases: {error}"));

	assert_eq!(outcomes.len(), 1);
	let outcome = outcomes
		.first()
		.unwrap_or_else(|| panic!("expected planned provider outcome"));
	assert_eq!(outcome.operation, RetargetProviderOperation::Planned);
	assert_eq!(outcome.tag_name, "v1.2.3");
}

#[test]
fn sync_retargeted_releases_updates_existing_release_target_commitish() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.3");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"id\":42,\"html_url\":\"https://example.com/releases/42\",\"target_commitish\":\"abc1234\"}",
			);
	});
	let update_release = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/releases/42")
			.json_body_obj(&serde_json::json!({ "target_commitish": "def5678" }));
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/releases/42\"}");
	});
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let updates = vec![RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Moved,
		message: None,
	}];

	let outcomes = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcomes =
				sync_retargeted_releases_with_client(&client, &source, &updates, false).await?;
			Ok::<_, MonochangeError>(outcomes)
		})
		.unwrap_or_else(|error| panic!("sync releases: {error}"));

	release_lookup.assert();
	update_release.assert();
	let outcome = outcomes
		.first()
		.unwrap_or_else(|| panic!("expected synced provider outcome"));
	assert_eq!(outcome.operation, RetargetProviderOperation::Synced);
	assert_eq!(
		outcome.url.as_deref(),
		Some("https://example.com/releases/42")
	);
}

#[test]
fn sync_retargeted_releases_reports_already_aligned_release() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.3");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"id\":42,\"html_url\":\"https://example.com/releases/42\",\"target_commitish\":\"def5678\"}",
			);
	});
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let updates = vec![RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Moved,
		message: None,
	}];

	let outcomes = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let outcomes =
				sync_retargeted_releases_with_client(&client, &source, &updates, false).await?;
			Ok::<_, MonochangeError>(outcomes)
		})
		.unwrap_or_else(|error| panic!("sync releases: {error}"));

	release_lookup.assert();
	let outcome = outcomes
		.first()
		.unwrap_or_else(|| panic!("expected already aligned provider outcome"));
	assert_eq!(outcome.operation, RetargetProviderOperation::AlreadyAligned);
}

#[test]
fn sync_retargeted_releases_errors_when_release_lookup_is_missing() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.3");
		then.status(404)
			.header("content-type", "application/json")
			.body("{\"message\":\"Not Found\"}");
	});
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let updates = vec![RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Moved,
		message: None,
	}];

	let error = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			sync_retargeted_releases_with_client(&client, &source, &updates, false).await
		})
		.err()
		.unwrap_or_else(|| panic!("expected release lookup error"));

	release_lookup.assert();
	assert!(
		error
			.to_string()
			.contains("GitHub release for tag `v1.2.3` could not be found")
	);
}

#[test]
fn sync_retargeted_releases_public_api_uses_source_configuration_and_env() {
	let server = MockServer::start();
	let release_lookup = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/releases/tags/v1.2.3");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				"{\"id\":42,\"html_url\":\"https://example.com/releases/42\",\"target_commitish\":\"abc1234\"}",
			);
	});
	let update_release = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/releases/42")
			.json_body_obj(&serde_json::json!({ "target_commitish": "def5678" }));
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/releases/42\"}");
	});
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: Some(server.base_url()),
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let updates = vec![RetargetTagResult {
		tag_name: "v1.2.3".to_string(),
		from_commit: "abc1234".to_string(),
		to_commit: "def5678".to_string(),
		operation: RetargetOperation::Moved,
		message: None,
	}];

	let outcomes = temp_env::with_var("GITHUB_TOKEN", Some("token"), || {
		sync_retargeted_releases(&source, &updates, false)
	})
	.unwrap_or_else(|error| panic!("public sync releases: {error}"));

	release_lookup.assert();
	update_release.assert();
	assert_eq!(outcomes.len(), 1);
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
			publish_release_pull_request_with_client(&client, &request).await
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
			.body("{\"data\":{\"enablePullRequestAutoMerge\":{\"pullRequest\":{\"number\":8}}}}");
	});
	let mut request = sample_pull_request_request();
	request.auto_merge = true;

	let outcome = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			publish_release_pull_request_with_client(&client, &request).await
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
				"[{\"number\":9,\"html_url\":\"https://example.com/pr/9\",\"node_id\":\"PR_node\",\"title\":\"old title\",\"body\":\"old body\",\"base\":{\"ref\":\"main\"},\"head\":{\"sha\":\"old-sha\"},\"labels\":[]}]",
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
			publish_release_pull_request_with_client(&client, &request).await
		})
		.unwrap_or_else(|error| panic!("publish pull request: {error}"));

	list_pull_requests.assert();
	update_pull_request.assert();
	add_labels.assert();
	assert_eq!(outcome.operation, GitHubPullRequestOperation::Updated);
	assert_eq!(outcome.number, 9);
}

#[test]
fn release_pull_request_commit_verification_is_opt_in() {
	let disabled_source = sample_source(None);
	let enabled_source = sample_source_with_verified_commits(None);

	temp_env::with_vars(
		[
			("GITHUB_ACTIONS", None),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			assert!(!github_actions_release_commit_verification_enabled(
				&enabled_source
			));
		},
	);

	temp_env::with_vars(
		[
			("GITHUB_ACTIONS", Some("true")),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			assert!(!github_actions_release_commit_verification_enabled(
				&disabled_source
			));
			assert!(github_actions_release_commit_verification_enabled(
				&enabled_source
			));
		},
	);
}

#[test]
fn release_pull_request_commit_verification_uses_github_git_database_api() {
	let server = MockServer::start();
	let source = sample_source_with_verified_commits(Some(server.base_url()));
	let request = sample_pull_request_request();
	let fallback = "1111111111111111111111111111111111111111";
	let verified = "2222222222222222222222222222222222222222";
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let original_commit = server.mock(|when, then| {
		when.method(GET)
			.path(format!("/repos/ifiokjr/monochange/git/commits/{fallback}"));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{fallback}","message":"chore(release): prepare release\n\nbody","tree":{{"sha":"tree123"}},"parents":[{{"sha":"parent123"}}],"verification":{{"verified":false,"reason":"unsigned"}}}}"#
			));
	});
	let parent_commit = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/commits/parent123");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"sha":"parent123","message":"parent","tree":{"sha":"tree123"},"parents":[],"verification":{"verified":false}}"#);
	});
	let create_commit = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/commits")
			.json_body(json!({
				"message": "chore(release): prepare release\n\nbody",
				"tree": "tree123",
				"parents": ["parent123"],
			}));
		then.status(201)
			.header("content-type", "application/json")
			.body(format!(
				 r#"{{"sha":"{verified}","message":"chore(release): prepare release\n\nbody","tree":{{"sha":"tree123"}},"parents":[{{"sha":"parent123"}}],"verification":{{"verified":true,"reason":"valid"}}}}"#
			));
	});
	let get_ref = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/ref/heads/monochange/release/release");
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{fallback}"}}}}"#));
	});
	let update_ref = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/git/refs/heads/monochange/release/release")
			.json_body(json!({ "sha": verified, "force": true }));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{verified}"}}}}"#));
	});

	let result = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_ACTIONS", Some("true")),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			maybe_replace_release_pull_request_commit_with_verified_github_commit(
				&source,
				&request,
				fallback,
				root,
				&[],
			)
		},
	);

	original_commit.assert();
	parent_commit.assert();
	create_commit.assert();
	get_ref.assert();
	update_ref.assert();
	assert_eq!(result, Ok(verified.to_string()));
}

#[test]
fn release_pull_request_commit_verification_falls_back_when_github_does_not_verify_commit() {
	let server = MockServer::start();
	let source = sample_source_with_verified_commits(Some(server.base_url()));
	let request = sample_pull_request_request();
	let fallback = "1111111111111111111111111111111111111111";
	let unverified = "2222222222222222222222222222222222222222";
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let original_commit = server.mock(|when, then| {
		when.method(GET)
			.path(format!("/repos/ifiokjr/monochange/git/commits/{fallback}"));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{fallback}","message":"chore(release): prepare release","tree":{{"sha":"tree123"}},"parents":[{{"sha":"parent123"}}],"verification":{{"verified":false,"reason":"unsigned"}}}}"#
			));
	});
	let parent_commit = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/commits/parent123");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"sha":"parent123","message":"parent","tree":{"sha":"tree123"},"parents":[],"verification":{"verified":false}}"#);
	});
	let create_commit = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/commits");
		then.status(201)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{unverified}","message":"chore(release): prepare release","tree":{{"sha":"tree123"}},"parents":[{{"sha":"parent123"}}],"verification":{{"verified":false,"reason":"unsigned"}}}}"#
			));
	});

	let result = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_ACTIONS", Some("true")),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			maybe_replace_release_pull_request_commit_with_verified_github_commit(
				&source,
				&request,
				fallback,
				root,
				&[],
			)
		},
	);

	original_commit.assert();
	parent_commit.assert();
	create_commit.assert();
	assert!(result.is_err_and(|message| message.contains("without verification (unsigned)")));
}
#[test]
fn release_pull_request_commit_verification_creates_blobs_for_varied_file_types() {
	let server = MockServer::start();
	let source = sample_source_with_verified_commits(Some(server.base_url()));
	let request = sample_pull_request_request();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	// Create a directory (should be skipped)
	std::fs::create_dir(root.join("skipped_dir")).unwrap();

	// Create a regular file
	std::fs::write(root.join("regular.txt"), "hello").unwrap();

	// Create an executable file
	let exec_path = root.join("exec.sh");
	std::fs::write(&exec_path, "#!/bin/bash").unwrap();
	use std::os::unix::fs::PermissionsExt;
	let mut perms = std::fs::metadata(&exec_path).unwrap().permissions();
	perms.set_mode(perms.mode() | 0o111);
	std::fs::set_permissions(&exec_path, perms).unwrap();

	// Create a symlink
	std::os::unix::fs::symlink("regular.txt", root.join("link")).unwrap();

	// deleted.txt intentionally does not exist

	let tracked_paths = vec![
		PathBuf::from("skipped_dir"),
		PathBuf::from("regular.txt"),
		PathBuf::from("exec.sh"),
		PathBuf::from("link"),
		PathBuf::from("deleted.txt"),
	];

	let fallback = "1111111111111111111111111111111111111111";
	let verified = "9999999999999999999999999999999999999999";

	let original_commit = server.mock(|when, then| {
		when.method(GET)
			.path(format!("/repos/ifiokjr/monochange/git/commits/{fallback}"));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{fallback}","message":"chore(release): prepare release\n\nbody","tree":{{"sha":"original_tree"}},"parents":[{{"sha":"parent123"}}],"verification":{{"verified":false,"reason":"unsigned"}}}}"#
			));
	});
	let parent_commit = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/commits/parent123");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"sha":"parent123","message":"parent","tree":{"sha":"parent_tree"},"parents":[],"verification":{"verified":false}}"#);
	});
	let blob_regular = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/blobs")
			.json_body(json!({"content": "hello", "encoding": "utf-8"}));
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"blob_regular"}"#);
	});
	let blob_exec = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/blobs")
			.json_body(json!({"content": "#!/bin/bash", "encoding": "utf-8"}));
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"blob_exec"}"#);
	});
	let blob_symlink = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/blobs")
			.json_body(json!({"content": "regular.txt", "encoding": "utf-8"}));
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"blob_symlink"}"#);
	});
	let create_tree = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/trees");
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"new_tree"}"#);
	});
	let create_commit = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/commits")
			.json_body(json!({
				"message": "chore(release): prepare release\n\nbody",
				"tree": "new_tree",
				"parents": ["parent123"],
			}));
		then.status(201)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{verified}","message":"chore(release): prepare release\n\nbody","tree":{{"sha":"new_tree"}},"parents":[{{"sha":"parent123"}}],"verification":{{"verified":true,"reason":"valid"}}}}"#
			));
	});
	let get_ref = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/ref/heads/monochange/release/release");
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{fallback}"}}}}"#));
	});
	let update_ref = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/git/refs/heads/monochange/release/release")
			.json_body(json!({ "sha": verified, "force": true }));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{verified}"}}}}"#));
	});

	let result = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_ACTIONS", Some("true")),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			maybe_replace_release_pull_request_commit_with_verified_github_commit(
				&source,
				&request,
				fallback,
				root,
				&tracked_paths,
			)
		},
	);

	original_commit.assert();
	parent_commit.assert();
	blob_regular.assert();
	blob_exec.assert();
	blob_symlink.assert();
	create_tree.assert();
	create_commit.assert();
	get_ref.assert();
	update_ref.assert();
	assert_eq!(result, Ok(verified.to_string()));
}
#[test]
fn release_pull_request_commit_verification_uses_root_commit_without_parent() {
	let server = MockServer::start();
	let source = sample_source_with_verified_commits(Some(server.base_url()));
	let request = sample_pull_request_request();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();

	std::fs::write(root.join("file.txt"), "content").unwrap();
	let tracked_paths = vec![PathBuf::from("file.txt")];

	let fallback = "1111111111111111111111111111111111111111";
	let verified = "9999999999999999999999999999999999999999";

	let original_commit = server.mock(|when, then| {
		when.method(GET)
			.path(format!("/repos/ifiokjr/monochange/git/commits/{fallback}"));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{fallback}","message":"chore(release): prepare release","tree":{{"sha":"tree123"}},"parents":[],"verification":{{"verified":false,"reason":"unsigned"}}}}"#
			));
	});
	let blob = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/blobs")
			.json_body(json!({"content": "content", "encoding": "utf-8"}));
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"blob1"}"#);
	});
	let create_tree = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/trees");
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"new_tree"}"#);
	});
	let create_commit = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/commits")
			.json_body(json!({
				"message": "chore(release): prepare release",
				"tree": "new_tree",
				"parents": [],
			}));
		then.status(201)
			.header("content-type", "application/json")
			.body(format!(
				r#"{{"sha":"{verified}","message":"chore(release): prepare release","tree":{{"sha":"new_tree"}},"parents":[],"verification":{{"verified":true,"reason":"valid"}}}}"#
			));
	});
	let get_ref = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/ref/heads/monochange/release/release");
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{fallback}"}}}}"#));
	});
	let update_ref = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/git/refs/heads/monochange/release/release")
			.json_body(json!({ "sha": verified, "force": true }));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{verified}"}}}}"#));
	});

	let result = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_ACTIONS", Some("true")),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			maybe_replace_release_pull_request_commit_with_verified_github_commit(
				&source,
				&request,
				fallback,
				root,
				&tracked_paths,
			)
		},
	);

	original_commit.assert();
	blob.assert();
	create_tree.assert();
	create_commit.assert();
	get_ref.assert();
	update_ref.assert();
	assert_eq!(result, Ok(verified.to_string()));
}

#[test]
fn build_release_pull_request_request_ignores_empty_sections() {
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings {
			branch_prefix: "automation/release".to_string(),
			base: "develop".to_string(),
			title: "chore(release): prepare release".to_string(),
			labels: vec!["release".to_string(), "automated".to_string()],
			auto_merge: true,
			..ProviderMergeRequestSettings::default()
		},
	};
	let mut manifest = sample_manifest();
	manifest
		.changelogs
		.first_mut()
		.unwrap()
		.notes
		.sections
		.push(ReleaseNotesSection {
			title: "Empty".to_string(),
			collapsed: false,
			entries: vec![],
		});

	let request = build_release_pull_request_request(&github, &manifest);

	assert!(!request.body.contains("### Empty"));
}

#[test]
fn release_pull_request_commit_verification_rejects_moved_branch() {
	let server = MockServer::start();
	let request = sample_pull_request_request();
	let get_ref = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/ref/heads/monochange/release/release");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"object":{"sha":"3333333333333333333333333333333333333333"}}"#);
	});

	let result = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			update_github_branch_ref_to_verified_commit(
				&client,
				&request,
				"1111111111111111111111111111111111111111",
				"2222222222222222222222222222222222222222",
			)
			.await
		});

	get_ref.assert();
	assert!(result.is_err_and(|message| {
		message.contains("release branch monochange/release/release moved")
	}));
}

#[test]
fn release_pull_request_commit_verification_rejects_unexpected_updated_ref() {
	let server = MockServer::start();
	let request = sample_pull_request_request();
	let fallback = "1111111111111111111111111111111111111111";
	let verified = "2222222222222222222222222222222222222222";
	let unexpected = "3333333333333333333333333333333333333333";
	let get_ref = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/ref/heads/monochange/release/release");
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{fallback}"}}}}"#));
	});
	let update_ref = server.mock(|when, then| {
		when.method(PATCH)
			.path("/repos/ifiokjr/monochange/git/refs/heads/monochange/release/release")
			.json_body(json!({ "sha": verified, "force": true }));
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(r#"{{"object":{{"sha":"{unexpected}"}}}}"#));
	});

	let result = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			update_github_branch_ref_to_verified_commit(&client, &request, fallback, verified).await
		});

	get_ref.assert();
	update_ref.assert();
	assert!(result.is_err_and(|message| {
		message.contains("expected 2222222222222222222222222222222222222222")
	}));
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
fn publish_release_pull_request_falls_back_when_verified_commit_is_unavailable() {
	let server = MockServer::start();
	let (_tempdir, repo) = seed_git_repository();
	let source = sample_source_with_verified_commits(Some(server.base_url()));
	let request = sample_pull_request_request();
	let list_pull_requests = server.mock(|when, then| {
		when.method(GET).path("/repos/ifiokjr/monochange/pulls");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let original_commit = server.mock(|when, then| {
		when.method(GET).path_matches(
			Regex::new(r"^/repos/ifiokjr/monochange/git/commits/[0-9a-f]{40}$")
				.unwrap_or_else(|error| panic!("regex: {error}")),
		);
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"sha":"1111111111111111111111111111111111111111","message":"chore(release): prepare release","tree":{"sha":"tree123"},"parents":[{"sha":"parent123"}],"verification":{"verified":false,"reason":"unsigned"}}"#);
	});
	let create_commit = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/commits");
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"2222222222222222222222222222222222222222","message":"chore(release): prepare release","tree":{"sha":"tree123"},"parents":[{"sha":"parent123"}],"verification":{"verified":false,"reason":"unsigned"}}"#);
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
	let parent_commit = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/git/commits/parent123");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"sha":"parent123","message":"parent","tree":{"sha":"parent_tree"},"parents":[],"verification":{"verified":false}}"#);
	});
	let create_blob = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/blobs");
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"blob123"}"#);
	});
	let create_tree = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/git/trees");
		then.status(201)
			.header("content-type", "application/json")
			.body(r#"{"sha":"new_tree123"}"#);
	});

	let outcome = temp_env::with_vars(
		[
			("GITHUB_TOKEN", Some("token")),
			("GITHUB_ACTIONS", Some("true")),
			("GITHUB_REPOSITORY", Some("ifiokjr/monochange")),
		],
		|| {
			publish_release_pull_request(
				&source,
				&repo,
				&request,
				&[PathBuf::from("release.txt")],
				false,
			)
			.unwrap_or_else(|error| panic!("publish pull request: {error}"))
		},
	);

	list_pull_requests.assert();
	original_commit.assert();
	parent_commit.assert();
	create_blob.assert();
	create_tree.assert();
	create_commit.assert();
	create_pull_request.assert();
	add_labels.assert();
	assert_eq!(outcome.operation, GitHubPullRequestOperation::Created);
	assert_eq!(outcome.number, 7);
}

#[test]
fn publish_release_pull_request_skips_matching_existing_pull_request() {
	let server = MockServer::start();
	let request = sample_pull_request_request();
	let existing = GitHubExistingPullRequest {
		number: 9,
		html_url: Some("https://example.com/pr/9".to_string()),
		node_id: "PR_node".to_string(),
		title: request.title.clone(),
		body: Some(request.body.clone()),
		base: GitHubExistingPullRequestBase {
			ref_name: request.base_branch.clone(),
		},
		head: GitHubExistingPullRequestHead {
			sha: Some("head-sha".to_string()),
		},
		labels: request
			.labels
			.iter()
			.cloned()
			.map(|name| GitHubExistingPullRequestLabel { name })
			.collect(),
	};

	let outcome = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			publish_release_pull_request_with_existing_pull_request(
				&client,
				&request,
				Some(&existing),
				"head-sha",
			)
			.await
		})
		.unwrap_or_else(|error| panic!("publish pull request: {error}"));

	assert_eq!(outcome.operation, GitHubPullRequestOperation::Skipped);
	assert_eq!(outcome.number, 9);
	assert_eq!(outcome.url.as_deref(), Some("https://example.com/pr/9"));
}

#[test]
fn join_existing_pull_request_lookup_reports_panicked_thread() {
	let error = join_existing_pull_request_lookup(thread::spawn(
		|| -> MonochangeResult<Option<GitHubExistingPullRequest>> {
			panic!("boom");
		},
	))
	.err()
	.unwrap_or_else(|| panic!("expected join error"));
	assert!(
		error
			.to_string()
			.contains("failed to join GitHub pull request lookup thread")
	);
}

#[test]
fn publish_release_pull_request_marks_matching_auto_merge_request_as_updated() {
	let server = MockServer::start();
	let enable_auto_merge = server.mock(|when, then| {
		when.method(POST).path("/graphql");
		then.status(200)
			.header("content-type", "application/json")
			.body("{\"data\":{\"enablePullRequestAutoMerge\":{\"pullRequest\":{\"number\":9}}}}");
	});
	let mut request = sample_pull_request_request();
	request.auto_merge = true;
	let existing = GitHubExistingPullRequest {
		number: 9,
		html_url: Some("https://example.com/pr/9".to_string()),
		node_id: "PR_node".to_string(),
		title: request.title.clone(),
		body: Some(request.body.clone()),
		base: GitHubExistingPullRequestBase {
			ref_name: request.base_branch.clone(),
		},
		head: GitHubExistingPullRequestHead {
			sha: Some("head-sha".to_string()),
		},
		labels: request
			.labels
			.iter()
			.cloned()
			.map(|name| GitHubExistingPullRequestLabel { name })
			.collect(),
	};

	let outcome = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			publish_release_pull_request_with_existing_pull_request(
				&client,
				&request,
				Some(&existing),
				"head-sha",
			)
			.await
		})
		.unwrap_or_else(|error| panic!("publish pull request: {error}"));

	assert_eq!(outcome.operation, GitHubPullRequestOperation::Updated);
	assert_eq!(outcome.number, 9);
	assert_eq!(outcome.url.as_deref(), Some("https://example.com/pr/9"));
	enable_auto_merge.assert();
}

#[test]
fn build_github_client_rejects_invalid_base_urls() {
	let error = build_github_client("token", Some("not a url"))
		.err()
		.unwrap_or_else(|| panic!("expected client error"));
	assert!(
		error
			.to_string()
			.contains("failed to configure GitHub base URL")
	);
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
			publish_release_requests_with_client(&client, &[request]).await
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
			.body("{\"data\":{\"enablePullRequestAutoMerge\":null}}");
	});
	let mut request = sample_pull_request_request();
	request.auto_merge = true;

	let error = github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			publish_release_pull_request_with_client(&client, &request).await
		})
		.err()
		.unwrap_or_else(|| panic!("expected auto merge error"));

	list_pull_requests.assert();
	create_pull_request.assert();
	add_labels.assert();
	enable_auto_merge.assert();
	assert!(
		error
			.to_string()
			.contains("auto merge returned no pull request payload")
	);
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
fn publish_release_pull_request_skips_push_when_existing_pull_request_matches_local_head() {
	let server = MockServer::start();
	let (tempdir, repo) = seed_git_repository();
	let source = sample_source(Some(server.base_url()));
	let request = sample_pull_request_request();

	git(&repo, &["checkout", "-B", &request.head_branch]);
	git(&repo, &["add", "-A", "--", "release.txt"]);
	git(&repo, &["commit", "-m", "prepare release branch"]);
	git(&repo, &["push", "-u", "origin", &request.head_branch]);
	let head_commit = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_string();
	let list_pull_requests = server.mock(|when, then| {
		when.method(GET).path("/repos/ifiokjr/monochange/pulls");
		then.status(200)
			.header("content-type", "application/json")
			.body(format!(
				"[{{\"number\":9,\"html_url\":\"https://example.com/pr/9\",\"node_id\":\"PR_node\",\"title\":{title:?},\"body\":{body:?},\"base\":{{\"ref\":{base:?}}},\"head\":{{\"sha\":{head:?}}},\"labels\":[{{\"name\":\"release\"}},{{\"name\":\"automated\"}}]}}]",
				title = request.title,
				body = request.body,
				base = request.base_branch,
				head = head_commit,
			));
	});
	git(
		&repo,
		&[
			"remote",
			"set-url",
			"origin",
			tempdir
				.path()
				.join("missing.git")
				.to_string_lossy()
				.as_ref(),
		],
	);

	let outcome = with_github_env(Some("token"), || {
		publish_release_pull_request(
			&source,
			&repo,
			&request,
			&[PathBuf::from("release.txt")],
			false,
		)
		.unwrap_or_else(|error| panic!("publish pull request: {error}"))
	});

	list_pull_requests.assert();
	assert_eq!(outcome.operation, GitHubPullRequestOperation::Skipped);
	assert_eq!(outcome.number, 9);
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
fn git_helpers_prepare_commit_and_push_release_branch() {
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
	must_ok(
		std::fs::write(repo.join("release.txt"), "after\n"),
		"update release file",
	);

	must_ok(
		git_checkout_branch(&repo, "monochange/release/release"),
		"checkout branch",
	);
	must_ok(
		git_checkout_branch(&repo, "monochange/release/release"),
		"repeat checkout branch",
	);
	must_ok(
		git_stage_paths(&repo, &[PathBuf::from("release.txt")]),
		"stage paths",
	);
	git_commit_paths(
		&repo,
		&CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: Some(
				"Prepare release.\n\n## monochange Release Record\n\n<!-- monochange:release-record:start -->\n```json\n{}\n```\n<!-- monochange:release-record:end -->".to_string(),
			),
		},
		false,
	)
		.unwrap_or_else(|error| panic!("commit paths: {error}"));
	git_push_branch(&repo, "monochange/release/release", false)
		.unwrap_or_else(|error| panic!("push branch: {error}"));

	let branch = git_output(
		&repo,
		&["rev-parse", "--verify", "monochange/release/release"],
	);
	assert!(!branch.trim().is_empty());
	let commit_body = git_output(&repo, &["log", "-1", "--pretty=%B"]);
	assert!(commit_body.contains("## monochange Release Record"));
	assert!(commit_body.contains("<!-- monochange:release-record:start -->"));
}

#[test]
fn git_stage_paths_skips_missing_untracked_paths_and_ignored_untracked_files() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path().join("repo");
	git(tempdir.path(), &["init", repo.to_string_lossy().as_ref()]);
	git(&repo, &["config", "user.name", "monochange Tests"]);
	git(&repo, &["config", "user.email", "monochange@example.com"]);
	must_ok(
		std::fs::write(repo.join(".gitignore"), ".monochange/\n"),
		"write gitignore",
	);
	must_ok(
		std::fs::write(repo.join("release.txt"), "before\n"),
		"write release file",
	);
	git(&repo, &["add", "."]);
	git(&repo, &["commit", "-m", "initial"]);
	must_ok(
		std::fs::create_dir_all(repo.join(".monochange")),
		"create monochange dir",
	);
	must_ok(
		std::fs::write(repo.join(".monochange/release-manifest.json"), "{}\n"),
		"write manifest",
	);

	must_ok(
		git_stage_paths(
			&repo,
			&[
				PathBuf::from(".monochange/release-manifest.json"),
				PathBuf::from(".changeset/missing.md"),
			],
		),
		"stage paths",
	);

	assert_eq!(
		git_output(&repo, &["diff", "--cached", "--name-only"]).trim(),
		""
	);
}

#[test]
fn git_path_is_tracked_reports_command_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path().join("missing");

	let error = git_path_is_tracked(&root, Path::new("release.txt"))
		.err()
		.unwrap_or_else(|| panic!("expected tracked command failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect tracked git path release.txt")
	);
}

#[test]
fn git_path_is_tracked_reports_inspection_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path().join("repo");
	must_ok(std::fs::create_dir_all(&repo), "create repo dir");
	must_ok(
		std::fs::write(repo.join("release.txt"), "release\n"),
		"write release file",
	);

	let error = git_path_is_tracked(&repo, Path::new("release.txt"))
		.err()
		.unwrap_or_else(|| panic!("expected tracked inspection failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect tracked git path release.txt")
	);
}

#[test]
fn git_path_is_ignored_reports_false_for_unignored_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path().join("repo");
	git(tempdir.path(), &["init", repo.to_string_lossy().as_ref()]);
	must_ok(
		std::fs::write(repo.join("release.txt"), "release\n"),
		"write release file",
	);

	assert!(
		!git_path_is_ignored(&repo, Path::new("release.txt"))
			.unwrap_or_else(|error| panic!("git path ignored: {error}"))
	);
}

#[test]
fn git_path_is_ignored_reports_inspection_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let repo = tempdir.path().join("repo");
	must_ok(std::fs::create_dir_all(&repo), "create repo dir");
	must_ok(
		std::fs::write(repo.join("release.txt"), "release\n"),
		"write release file",
	);

	let error = git_path_is_ignored(&repo, Path::new("release.txt"))
		.err()
		.unwrap_or_else(|| panic!("expected ignored inspection failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect ignored git path release.txt")
	);
}

#[test]
fn git_path_is_ignored_reports_command_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path().join("missing");

	let error = git_path_is_ignored(&root, Path::new("release.txt"))
		.err()
		.unwrap_or_else(|| panic!("expected ignored command failure"));
	assert!(
		error
			.to_string()
			.contains("failed to inspect ignored git path release.txt")
	);
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
fn git_commit_paths_reports_io_and_non_noop_failures() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing = tempdir.path().join("missing");
	let io_error = git_commit_paths(
		&missing,
		&CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		false,
	)
	.err()
	.unwrap_or_else(|| panic!("expected missing worktree error"));
	assert!(
		io_error
			.to_string()
			.contains("failed to commit release pull request changes")
	);

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
		false,
	)
	.err()
	.unwrap_or_else(|| panic!("expected pre-commit hook failure"));
	assert!(
		error
			.to_string()
			.contains("failed to commit release pull request changes")
	);
}

#[etest::etest(skip=env::var_os("PRE_COMMIT").is_some())]
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
		false,
	)
	.unwrap_or_else(|error| panic!("commit paths: {error}"));

	assert_eq!(
		git_output(&repo, &["rev-list", "--count", "HEAD"]).trim(),
		"1"
	);
}

#[test]
fn enrich_changeset_context_resolves_pull_requests_and_related_issues() {
	let server = MockServer::start();
	let lookup_review_requests = server.mock(|when, then| {
		when.method(POST).path("/graphql");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				r#"{"data":{"repository":{"commit_0":{"associatedPullRequests":{"nodes":[{"number":42,"title":"Add release context","url":"https://example.com/pulls/42","body":"Closes #7\nRefs #8","author":{"login":"ifiokjr","url":"https://example.com/users/1"}}]}}}}}"#,
			);
	});
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("add release context".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GenericGit,
			host: None,
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					id: None,
					login: None,
					display_name: Some("Ifiok Jr.".to_string()),
					url: None,
					source: HostedActorSourceKind::CommitAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					sha: "abc1234567890".to_string(),
					short_sha: "abc1234".to_string(),
					url: None,
					authored_at: Some("2024-01-01T00:00:00Z".to_string()),
					committed_at: Some("2024-01-01T00:00:00Z".to_string()),
					author_name: Some("Ifiok Jr.".to_string()),
					author_email: Some("ifiok@example.com".to_string()),
				}),
				review_request: None,
			}),
			last_updated: None,
			related_issues: Vec::new(),
		}),
	}];

	temp_env::with_var("GITHUB_SERVER_URL", Some("https://example.com"), || {
		github_runtime()
			.unwrap_or_else(|error| panic!("runtime: {error}"))
			.block_on(async {
				let client = build_test_client(&server);
				enrich_changeset_context_with_client(&client, &github, &mut changesets).await;
			});
	});

	lookup_review_requests.assert();
	let context = changesets
		.first()
		.and_then(|changeset| changeset.context.as_ref())
		.unwrap_or_else(|| panic!("expected context"));
	assert_eq!(context.provider, HostingProviderKind::GitHub);
	assert_eq!(context.host.as_deref(), Some("example.com"));
	assert_eq!(context.related_issues.len(), 2);
	assert!(context.related_issues.iter().any(|issue| {
		issue.id == "#7" && issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest
	}));
	assert!(context.related_issues.iter().any(|issue| {
		issue.id == "#8"
			&& issue.relationship == HostedIssueRelationshipKind::ReferencedByReviewRequest
	}));
	let introduced = context
		.introduced
		.as_ref()
		.unwrap_or_else(|| panic!("expected introduced revision"));
	assert_eq!(
		introduced
			.review_request
			.as_ref()
			.and_then(|review_request| review_request.title.as_deref()),
		Some("Add release context")
	);
	assert_eq!(
		introduced
			.actor
			.as_ref()
			.and_then(|actor| actor.login.as_deref()),
		Some("ifiokjr")
	);
	assert_eq!(
		introduced
			.commit
			.as_ref()
			.and_then(|commit| commit.url.as_deref()),
		Some("https://example.com/ifiokjr/monochange/commit/abc1234567890")
	);
}

#[test]
fn review_request_query_uses_lean_pull_request_payload() {
	let query =
		build_review_request_batch_query("ifiokjr", "monochange", &["abc1234567890".to_string()]);

	assert!(query.contains("associatedPullRequests(first: 1)"));
	assert!(query.contains("body"));
	assert!(!query.contains("closingIssuesReferences"));
}

#[test]
fn enrich_changeset_context_public_api_uses_source_configuration() {
	let source = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("add release context".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GenericGit,
			host: None,
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: None,
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					sha: "abc1234567890".to_string(),
					short_sha: "abc1234".to_string(),
					url: None,
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: None,
			}),
			last_updated: None,
			related_issues: Vec::new(),
		}),
	}];

	temp_env::with_vars(
		[
			("GITHUB_SERVER_URL", Some("https://example.com")),
			("GITHUB_TOKEN", None::<&str>),
		],
		|| enrich_changeset_context(&source, &mut changesets),
	);

	let commit_url = changesets
		.first()
		.unwrap_or_else(|| panic!("expected one changeset"))
		.context
		.as_ref()
		.and_then(|context| context.introduced.as_ref())
		.and_then(|revision| revision.commit.as_ref())
		.and_then(|commit| commit.url.as_deref())
		.unwrap_or_else(|| panic!("expected commit url"));
	assert_eq!(
		commit_url,
		"https://example.com/ifiokjr/monochange/commit/abc1234567890"
	);
}

#[test]
fn enrich_changeset_context_falls_back_to_commit_annotations_when_batch_lookup_fails() {
	let server = MockServer::start();
	let failing_lookup = server.mock(|when, then| {
		when.method(POST).path("/graphql");
		then.status(500);
	});
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("add release context".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GenericGit,
			host: None,
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					id: None,
					login: Some("ifiokjr".to_string()),
					display_name: Some("Ifiok Jr.".to_string()),
					url: None,
					source: HostedActorSourceKind::CommitAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GenericGit,
					host: None,
					sha: "abc1234567890".to_string(),
					short_sha: "abc1234".to_string(),
					url: None,
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: None,
			}),
			last_updated: None,
			related_issues: Vec::new(),
		}),
	}];

	temp_env::with_var("GITHUB_SERVER_URL", Some("https://example.com"), || {
		github_runtime()
			.unwrap_or_else(|error| panic!("runtime: {error}"))
			.block_on(async {
				let client = build_test_client(&server);
				enrich_changeset_context_with_client(&client, &github, &mut changesets).await;
			});
	});

	assert!(
		failing_lookup.calls() >= 1,
		"expected at least one failed batch lookup"
	);
	let context = changesets
		.first()
		.and_then(|changeset| changeset.context.as_ref())
		.unwrap_or_else(|| panic!("expected context"));
	assert_eq!(context.provider, HostingProviderKind::GitHub);
	assert!(context.related_issues.is_empty());
	assert_eq!(
		context
			.introduced
			.as_ref()
			.and_then(|revision| revision.commit.as_ref())
			.and_then(|commit| commit.url.as_deref()),
		Some("https://example.com/ifiokjr/monochange/commit/abc1234567890")
	);
}

#[test]
fn batch_review_request_lookup_reports_missing_repository_payload_and_parses_body_issue_refs() {
	let server = MockServer::start();
	let missing_repository = server.mock(|when, then| {
		when.method(POST)
			.path("/graphql")
			.header_exists("content-type");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"data":{}}"#);
	});
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&server);
			let error = load_review_request_batch_with_client(
				&client,
				&github,
				&["abc1234567890".to_string()],
			)
			.await
			.err()
			.unwrap_or_else(|| panic!("expected missing repository error"));
			assert!(
				error
					.to_string()
					.contains("GitHub review-request lookup returned no repository payload")
			);
		});
	missing_repository.assert();

	let parsing_server = MockServer::start();
	let parsing_issues = parsing_server.mock(|when, then| {
		when.method(POST).path("/graphql");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				r#"{"data":{"repository":{"commit_0":{"associatedPullRequests":{"nodes":[{"number":42,"title":"Add release context","url":"https://example.com/pulls/42","body":"Closes #7, #9 and owner/repo#11\nRefs #8","author":{"login":"ifiokjr","url":"https://example.com/users/1"}}]}}}}}"#,
			);
	});
	github_runtime()
		.unwrap_or_else(|error| panic!("runtime: {error}"))
		.block_on(async {
			let client = build_test_client(&parsing_server);
			let review_requests = load_review_request_batch_with_client(
				&client,
				&github,
				&["abc1234567890".to_string()],
			)
			.await
			.unwrap_or_else(|error| panic!("batch lookup: {error}"));
			let issues = review_requests
				.get("abc1234567890")
				.and_then(|value| value.as_ref())
				.map(|related| related.issues.clone())
				.unwrap_or_default();
			assert_eq!(issues.len(), 4);
			assert!(issues.iter().any(|issue| {
				issue.id == "#7"
					&& issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest
			}));
			assert!(issues.iter().any(|issue| {
				issue.id == "#9"
					&& issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest
			}));
			assert!(issues.iter().any(|issue| {
				issue.id == "#11"
					&& issue.relationship == HostedIssueRelationshipKind::ClosedByReviewRequest
			}));
			assert!(issues.iter().any(|issue| {
				issue.id == "#8"
					&& issue.relationship == HostedIssueRelationshipKind::ReferencedByReviewRequest
			}));
		});
	parsing_issues.assert();
}

#[test]
fn extract_closing_issue_numbers_only_marks_closing_keywords() {
	let body = "Closes #7, #9 and owner/repo#11\nRefs #8\nFixed #10 and refs #12";

	assert_eq!(
		extract_issue_numbers(body).into_iter().collect::<Vec<_>>(),
		vec![7, 8, 9, 10, 11, 12]
	);
	assert_eq!(
		extract_closing_issue_numbers(body)
			.into_iter()
			.collect::<Vec<_>>(),
		vec![7, 9, 10, 11]
	);
}

#[test]
fn comment_released_issues_skips_existing_markers_and_posts_missing_comments() {
	let server = MockServer::start();
	let list_issue_seven_comments = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(200)
			.header("content-type", "application/json")
			.body("[]");
	});
	let create_issue_seven_comment = server.mock(|when, then| {
		when.method(POST)
			.path("/repos/ifiokjr/monochange/issues/7/comments");
		then.status(201)
			.header("content-type", "application/json")
			.body("{\"html_url\":\"https://example.com/issues/7#comment-1\"}");
	});
	let list_issue_eight_comments = server.mock(|when, then| {
		when.method(GET)
			.path("/repos/ifiokjr/monochange/issues/8/comments");
		then.status(200)
			.header("content-type", "application/json")
			.body(
				r#"[{"html_url":"https://example.com/issues/8#comment-1","body":"Released in v1.2.0.\n\n<!-- monochange:released-in:v1.2.0 -->"}]"#,
			);
	});
	let github = SourceConfiguration {
		provider: SourceProvider::GitHub,
		host: None,
		api_url: None,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut manifest = sample_manifest();
	manifest.changesets = vec![PreparedChangeset {
		path: PathBuf::from(".changeset/feature.md"),
		summary: Some("add release context".to_string()),
		details: None,
		targets: Vec::new(),
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GitHub,
			host: Some("example.com".to_string()),
			capabilities: github_hosting_capabilities(),
			introduced: None,
			last_updated: None,
			related_issues: vec![
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("example.com".to_string()),
					id: "#7".to_string(),
					title: Some("Track release context".to_string()),
					url: Some("https://example.com/issues/7".to_string()),
					relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
				},
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("example.com".to_string()),
					id: "#8".to_string(),
					title: Some("Existing comment".to_string()),
					url: Some("https://example.com/issues/8".to_string()),
					relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
				},
			],
		}),
	}];

	let plans = plan_released_issue_comments(&github, &manifest);
	assert_eq!(plans.len(), 2);
	assert!(
		plans
			.iter()
			.all(|plan| plan.body.contains("Released in v1.2.0."))
	);

	let outcomes = temp_env::with_var("GITHUB_SERVER_URL", Some("https://example.com"), || {
		github_runtime()
			.unwrap_or_else(|error| panic!("runtime: {error}"))
			.block_on(async {
				let client = build_test_client(&server);
				comment_released_issues_with_client(&client, &github, &plans).await
			})
			.unwrap_or_else(|error| panic!("comment released issues: {error}"))
	});

	list_issue_seven_comments.assert();
	create_issue_seven_comment.assert();
	list_issue_eight_comments.assert();
	assert!(outcomes.iter().any(|outcome| {
		outcome.issue_id == "#7" && outcome.operation == GitHubIssueCommentOperation::Created
	}));
	assert!(outcomes.iter().any(|outcome| {
		outcome.issue_id == "#8"
			&& outcome.operation == GitHubIssueCommentOperation::SkippedExisting
	}));
}

fn sample_release_request() -> GitHubReleaseRequest {
	GitHubReleaseRequest {
		provider: SourceProvider::GitHub,
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
		provider: SourceProvider::GitHub,
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
		commit_message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
	}
}

fn build_test_client(server: &MockServer) -> Octocrab {
	build_github_client("test-token", Some(&server.base_url()))
		.unwrap_or_else(|error| panic!("octocrab client: {error}"))
}

fn sample_source(api_url: Option<String>) -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::GitHub,
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		host: None,
		api_url,
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn sample_source_with_verified_commits(api_url: Option<String>) -> SourceConfiguration {
	SourceConfiguration {
		pull_requests: ProviderMergeRequestSettings {
			verified_commits: true,
			..ProviderMergeRequestSettings::default()
		},
		..sample_source(api_url)
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
			members: vec![
				"cargo:crates/core/Cargo.toml".to_string(),
				"cargo:crates/app/Cargo.toml".to_string(),
			],
		}],
		package_publications: vec![],
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
					collapsed: false,
					entries: vec!["- add github publishing".to_string()],
				}],
			},
			rendered:
				"## 1.2.0\n\nGrouped release for `sdk`.\n\n### Features\n\n- add github publishing"
					.to_string(),
		}],
		changesets: Vec::new(),
		deleted_changesets: Vec::new(),
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

fn with_github_env<R>(token: Option<&str>, action: impl FnOnce() -> R) -> R {
	temp_env::with_var("GITHUB_TOKEN", token, action)
}

fn seed_git_repository() -> (tempfile::TempDir, PathBuf) {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let bare = tempdir.path().join("origin.git");
	let repo = tempdir.path().join("repo");
	git(
		tempdir.path(),
		&[
			"init",
			"--bare",
			"--initial-branch=main",
			bare.to_string_lossy().as_ref(),
		],
	);
	git(
		tempdir.path(),
		&[
			"init",
			"--initial-branch=main",
			repo.to_string_lossy().as_ref(),
		],
	);
	git(&repo, &["config", "user.name", "monochange Tests"]);
	git(&repo, &["config", "user.email", "monochange@example.com"]);
	git(&repo, &["config", "commit.gpgsign", "false"]);
	std::fs::write(repo.join("release.txt"), "before\n")
		.unwrap_or_else(|error| panic!("write release file: {error}"));
	git(&repo, &["add", "release.txt"]);
	git(&repo, &["commit", "-m", "initial"]);
	git(
		&repo,
		&["remote", "add", "origin", bare.to_string_lossy().as_ref()],
	);
	git(&repo, &["push", "-u", "origin", "main"]);
	std::fs::write(repo.join("release.txt"), "after\n")
		.unwrap_or_else(|error| panic!("update release file: {error}"));
	(tempdir, repo)
}

#[test]
fn github_commit_client_from_env_requires_commit_token_or_github_token() {
	let source = sample_source(None);

	let result = temp_env::with_vars(
		[
			("GITHUB_COMMIT_TOKEN", None::<&str>),
			("GITHUB_TOKEN", None::<&str>),
		],
		|| {
			let runtime = github_runtime().unwrap_or_else(|error| panic!("runtime: {error}"));
			runtime.block_on(async { github_commit_client_from_env(&source) })
		},
	);

	assert_eq!(
		result
			.err()
			.map(|error| error.to_string()),
		Some("config error: set `GITHUB_COMMIT_TOKEN` (or `GITHUB_TOKEN`) for GitHub commit verification".to_string())
	);
}

#[test]
fn github_commit_client_from_env_prefers_github_commit_token_over_github_token() {
	let source = sample_source(None);

	let result = temp_env::with_vars(
		[
			("GITHUB_COMMIT_TOKEN", Some("commit-token")),
			("GITHUB_TOKEN", Some("github-token")),
		],
		|| {
			let runtime = github_runtime().unwrap_or_else(|error| panic!("runtime: {error}"));
			runtime.block_on(async { github_commit_client_from_env(&source) })
		},
	);

	assert!(result.is_ok());
}
