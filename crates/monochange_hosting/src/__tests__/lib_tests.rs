#![forbid(clippy::indexing_slicing)]

use std::path::PathBuf;

use httpmock::Method::GET;
use httpmock::Method::PATCH;
use httpmock::Method::POST;
use httpmock::Method::PUT;
use httpmock::MockServer;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;
use monochange_core::ReleaseOwnerKind;
use monochange_core::VersionFormat;
use reqwest::header::HeaderMap;
use serde::Deserialize;
use serde::Serialize;

use super::*;

#[derive(Debug, Serialize)]
struct SampleBody {
	name: String,
}

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct SampleResponse {
	ok: bool,
}

fn empty_headers() -> HeaderMap {
	HeaderMap::new()
}

fn sample_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![],
		package_publications: vec![],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		changesets: vec![],
		deleted_changesets: vec![],
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![],
			groups: vec![],
			warnings: vec![],
			unresolved_items: vec![],
			compatibility_evidence: vec![],
		},
	}
}

fn minimal_target(id: &str) -> ReleaseManifestTarget {
	ReleaseManifestTarget {
		id: id.to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "0.1.0".to_string(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		tag_name: "v0.1.0".to_string(),
		members: vec![],
		rendered_title: String::new(),
		rendered_changelog_title: String::new(),
	}
}

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::disallowed_methods)]
async fn git_checkout_branch_creates_release_branch_from_detached_head() -> Result<(), String> {
	let tempdir = tempfile::tempdir().map_err(|error| format!("tempdir: {error}"))?;
	let root = tempdir.path();
	monochange_test_helpers::git(root, &["init", "-b", "main"]);
	monochange_test_helpers::git(root, &["config", "user.name", "monochange Tests"]);
	monochange_test_helpers::git(root, &["config", "user.email", "monochange@example.com"]);
	std::fs::write(root.join("README.md"), "initial\n")
		.map_err(|error| format!("write readme: {error}"))?;
	monochange_test_helpers::git(root, &["add", "README.md"]);
	monochange_test_helpers::git(root, &["commit", "-m", "initial commit"]);
	let head = monochange_test_helpers::git_output_trimmed(root, &["rev-parse", "HEAD"]);
	monochange_test_helpers::git(root, &["checkout", "--detach", &head]);

	git_checkout_branch(root, "monochange/release", "checkout release branch")
		.await
		.map_err(|error| format!("checkout release branch: {error}"))?;

	let branch = monochange_test_helpers::git_output_trimmed(root, &["branch", "--show-current"]);
	if branch != "monochange/release" {
		return Err(format!("checked out branch: {branch}"));
	}

	Ok(())
}

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::disallowed_methods)]
async fn git_stage_paths_with_stage_all_stages_incidental_changes() -> Result<(), String> {
	let tempdir = tempfile::tempdir().map_err(|error| format!("tempdir: {error}"))?;
	let root = tempdir.path();
	monochange_test_helpers::git(root, &["init", "-b", "main"]);
	monochange_test_helpers::git(root, &["config", "user.name", "monochange Tests"]);
	monochange_test_helpers::git(root, &["config", "user.email", "monochange@example.com"]);
	std::fs::write(
		root.join("release.txt"),
		"release
",
	)
	.map_err(|error| format!("write release file: {error}"))?;
	monochange_test_helpers::git(root, &["add", "release.txt"]);
	monochange_test_helpers::git(root, &["commit", "-m", "initial"]);
	std::fs::write(
		root.join("release.txt"),
		"release update
",
	)
	.map_err(|error| format!("update release file: {error}"))?;
	std::fs::write(
		root.join("pnpm-lock.yaml"),
		"lockfile
",
	)
	.map_err(|error| format!("write lockfile: {error}"))?;

	git_stage_paths(root, &[PathBuf::from("release.txt")], "stage release", true)
		.await
		.map_err(|error| format!("stage release: {error}"))?;

	let status = monochange_test_helpers::git_output_trimmed(root, &["status", "--short"]);
	assert!(status.contains("M  release.txt"), "status: {status}");
	assert!(status.contains("A  pnpm-lock.yaml"), "status: {status}");
	Ok(())
}

#[test]
fn push_body_entries_adds_dash_prefix_to_plain_entries() {
	let mut lines = Vec::new();
	push_body_entries(
		&mut lines,
		&["fix bug".to_string(), "add feature".to_string()],
	);
	assert_eq!(lines, vec!["- fix bug", "- add feature"]);
}

#[test]
fn push_body_entries_preserves_list_markers() {
	let mut lines = Vec::new();
	push_body_entries(&mut lines, &["- already a list item".to_string()]);
	assert_eq!(lines, vec!["- already a list item"]);
}

#[test]
fn push_body_entries_preserves_star_markers() {
	let mut lines = Vec::new();
	push_body_entries(&mut lines, &["* starred item".to_string()]);
	assert_eq!(lines, vec!["* starred item"]);
}

#[test]
fn push_body_entries_preserves_headings() {
	let mut lines = Vec::new();
	push_body_entries(&mut lines, &["### Bug Fixes".to_string()]);
	assert_eq!(lines, vec!["### Bug Fixes"]);
}

#[test]
fn push_body_entries_splits_multiline_entries() {
	let mut lines = Vec::new();
	push_body_entries(
		&mut lines,
		&["line one\nline two".to_string(), "second entry".to_string()],
	);
	assert_eq!(lines, vec!["line one", "line two", "", "- second entry"]);
}

#[test]
fn push_body_entries_multiline_last_entry_has_no_trailing_blank() {
	let mut lines = Vec::new();
	push_body_entries(&mut lines, &["multi\nline".to_string()]);
	assert_eq!(lines, vec!["multi", "line"]);
}

#[test]
fn minimal_release_body_includes_target_id_and_members() {
	let manifest = sample_manifest();
	let target = ReleaseManifestTarget {
		id: "my-pkg".to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "1.0.0".to_string(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
		tag_name: "v1.0.0".to_string(),
		members: vec!["dep-a".to_string(), "dep-b".to_string()],
		rendered_title: String::new(),
		rendered_changelog_title: String::new(),
	};
	let body = minimal_release_body(&manifest, &target);
	assert!(body.contains("my-pkg"));
	assert!(body.contains("dep-a, dep-b"));
}

#[test]
fn minimal_release_body_without_members_shows_prepare_release() {
	let manifest = sample_manifest();
	let target = minimal_target("core");
	let body = minimal_release_body(&manifest, &target);
	assert!(body.contains("prepare release"));
}

#[test]
fn release_pull_request_branch_sanitizes_special_characters() {
	assert_eq!(
		release_pull_request_branch("release/", "My Cool PR!"),
		"release/my-cool-pr"
	);
}

#[test]
fn release_pull_request_branch_falls_back_for_empty_command() {
	assert_eq!(
		release_pull_request_branch("release/", "!!!"),
		"release/release"
	);
}

#[test]
fn release_pull_request_branch_preserves_alphanumeric() {
	assert_eq!(
		release_pull_request_branch("release/", "v2-Feature"),
		"release/v2-feature"
	);
}

#[test]
fn release_pull_request_branch_strips_trailing_slash_from_prefix() {
	assert_eq!(
		release_pull_request_branch("monochange/release/", "Add Feature"),
		"monochange/release/add-feature"
	);
}

#[test]
fn build_http_client_succeeds() {
	assert!(build_http_client("test").is_ok());
}

#[test]
fn release_pull_request_body_includes_command_and_targets() {
	let manifest = ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![minimal_target("core")],
		package_publications: vec![],
		released_packages: vec![],
		changed_files: vec![PathBuf::from("Cargo.toml")],
		changelogs: vec![],
		changesets: vec![],
		deleted_changesets: vec![],
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![],
			groups: vec![],
			warnings: vec![],
			unresolved_items: vec![],
			compatibility_evidence: vec![],
		},
	};
	let body = release_pull_request_body(&manifest);
	assert!(body.contains("## Prepared release"));
	assert!(body.contains("`release`"));
	assert!(body.contains("core"));
}

#[test]
fn release_pull_request_body_shows_no_outward_targets_when_none_release() {
	let mut manifest = sample_manifest();
	manifest.release_targets = vec![ReleaseManifestTarget {
		id: "internal".to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "1.0.0".to_string(),
		tag: true,
		release: false,
		version_format: VersionFormat::Namespaced,
		tag_name: "v1.0.0".to_string(),
		members: vec![],
		rendered_title: String::new(),
		rendered_changelog_title: String::new(),
	}];
	let body = release_pull_request_body(&manifest);
	assert!(body.contains("no outward release targets"));
}

#[test]
fn release_pull_request_body_lists_changed_files() {
	let manifest = ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![],
		package_publications: vec![],
		released_packages: vec![],
		changed_files: vec![PathBuf::from("src/main.rs")],
		changelogs: vec![],
		changesets: vec![],
		deleted_changesets: vec![],
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![],
			groups: vec![],
			warnings: vec![],
			unresolved_items: vec![],
			compatibility_evidence: vec![],
		},
	};
	let body = release_pull_request_body(&manifest);
	assert!(body.contains("## Changed files"));
	assert!(body.contains("src/main.rs"));
}

#[test]
fn release_pull_request_body_ignores_changelogs_without_exact_owner_match() {
	let mut manifest = sample_manifest();
	manifest.release_targets = vec![minimal_target("core")];
	manifest.changelogs = vec![
		ReleaseManifestChangelog {
			owner_id: "other".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			path: PathBuf::from("other.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "0.1.0".to_string(),
				summary: vec![],
				sections: vec![ReleaseNotesSection {
					title: "Wrong".to_string(),
					collapsed: false,
					entries: vec!["wrong package".to_string()],
				}],
			},
			rendered: "wrong package changelog".to_string(),
		},
		ReleaseManifestChangelog {
			owner_id: "core".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			path: PathBuf::from("group.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "0.1.0".to_string(),
				summary: vec![],
				sections: vec![ReleaseNotesSection {
					title: "Wrong kind".to_string(),
					collapsed: false,
					entries: vec!["wrong owner kind".to_string()],
				}],
			},
			rendered: "wrong kind changelog".to_string(),
		},
	];

	let body = release_pull_request_body(&manifest);
	assert!(!body.contains("wrong package changelog"));
	assert!(!body.contains("wrong kind changelog"));
	assert!(body.contains(&minimal_release_body(&manifest, &minimal_target("core"))));
}

#[test]
fn minimal_release_body_with_decision_reasons() {
	let manifest = ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![],
		package_publications: vec![],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		changesets: vec![],
		deleted_changesets: vec![],
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![monochange_core::ReleaseManifestPlanDecision {
				package: "my-pkg".to_string(),
				bump: monochange_core::BumpSeverity::Patch,
				trigger: "direct-change".to_string(),
				planned_version: Some("1.0.1".to_string()),
				reasons: vec!["fix race condition".to_string()],
				upstream_sources: vec![],
			}],
			groups: vec![],
			warnings: vec![],
			unresolved_items: vec![],
			compatibility_evidence: vec![],
		},
	};
	let target = minimal_target("my-pkg");
	let body = minimal_release_body(&manifest, &target);
	assert!(body.contains("fix race condition"));
	assert!(!body.contains("prepare release"));
}

#[test]
fn release_body_returns_none_for_github_generated() {
	use monochange_core::ProviderMergeRequestSettings;
	use monochange_core::ProviderReleaseNotesSource;
	use monochange_core::ProviderReleaseSettings;

	let source = SourceConfiguration {
		provider: monochange_core::SourceProvider::GitHub,
		owner: "org".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings {
			enabled: true,
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: ProviderReleaseNotesSource::GitHubGenerated,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let manifest = sample_manifest();
	let target = minimal_target("core");
	assert_eq!(release_body(&source, &manifest, &target), None);
}

#[test]
fn release_body_returns_rendered_changelog_for_monochange_source() {
	use monochange_core::ChangelogFormat;
	use monochange_core::ProviderMergeRequestSettings;
	use monochange_core::ProviderReleaseNotesSource;
	use monochange_core::ProviderReleaseSettings;
	use monochange_core::ReleaseManifestChangelog;
	use monochange_core::ReleaseNotesDocument;
	use monochange_core::ReleaseNotesSection;

	let source = SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		owner: "org".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings {
			enabled: true,
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: ProviderReleaseNotesSource::Monochange,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut manifest = sample_manifest();
	let target = minimal_target("core");
	manifest.changelogs = vec![ReleaseManifestChangelog {
		owner_id: "core".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		path: PathBuf::from("changelog.md"),
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "1.0.0".to_string(),
			summary: vec![],
			sections: vec![ReleaseNotesSection {
				title: "Bug Fixes".to_string(),
				collapsed: false,
				entries: vec!["fix crash".to_string()],
			}],
		},
		rendered: "## 1.0.0\n\n### Bug Fixes\n\n- fix crash".to_string(),
	}];
	let body = release_body(&source, &manifest, &target);
	assert_eq!(
		body,
		Some("## 1.0.0\n\n### Bug Fixes\n\n- fix crash".to_string())
	);
}

#[test]
fn release_body_falls_back_to_minimal_when_no_changelog() {
	use monochange_core::ProviderMergeRequestSettings;
	use monochange_core::ProviderReleaseNotesSource;
	use monochange_core::ProviderReleaseSettings;

	let source = SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		owner: "org".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings {
			enabled: true,
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: ProviderReleaseNotesSource::Monochange,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let manifest = sample_manifest();
	let target = minimal_target("core");
	let body = release_body(&source, &manifest, &target);
	assert!(body.is_some());
	assert!(body.unwrap().contains("core"));
}

#[test]
fn release_body_falls_back_to_minimal_when_only_non_matching_changelog_exists() {
	use monochange_core::ProviderMergeRequestSettings;
	use monochange_core::ProviderReleaseNotesSource;
	use monochange_core::ProviderReleaseSettings;

	let source = SourceConfiguration {
		provider: monochange_core::SourceProvider::GitLab,
		owner: "org".to_string(),
		repo: "repo".to_string(),
		host: None,
		api_url: None,
		releases: ProviderReleaseSettings {
			enabled: true,
			draft: false,
			prerelease: false,
			generate_notes: false,
			source: ProviderReleaseNotesSource::Monochange,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
	};
	let mut manifest = sample_manifest();
	let target = minimal_target("core");
	manifest.changelogs = vec![
		ReleaseManifestChangelog {
			owner_id: "other".to_string(),
			owner_kind: ReleaseOwnerKind::Package,
			path: PathBuf::from("other.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "0.1.0".to_string(),
				summary: vec![],
				sections: vec![ReleaseNotesSection {
					title: "Wrong package".to_string(),
					collapsed: false,
					entries: vec!["wrong package".to_string()],
				}],
			},
			rendered: "wrong package changelog".to_string(),
		},
		ReleaseManifestChangelog {
			owner_id: "core".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			path: PathBuf::from("group.md"),
			format: monochange_core::ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "0.1.0".to_string(),
				summary: vec![],
				sections: vec![ReleaseNotesSection {
					title: "Wrong kind".to_string(),
					collapsed: false,
					entries: vec!["wrong kind".to_string()],
				}],
			},
			rendered: "wrong kind changelog".to_string(),
		},
	];

	assert_eq!(
		release_body(&source, &manifest, &target),
		Some(minimal_release_body(&manifest, &target))
	);
}

#[test]
fn get_optional_json_returns_none_for_404_and_some_for_success() {
	let server = MockServer::start();
	let not_found = server.mock(|when, then| {
		when.method(GET).path("/missing");
		then.status(404);
	});
	let found = server.mock(|when, then| {
		when.method(GET).path("/present");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"ok":true}"#);
	});
	let client = build_http_client("test").unwrap_or_else(|error| panic!("client: {error}"));
	let headers = empty_headers();

	assert_eq!(
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(get_optional_json::<SampleResponse>(
				&client,
				&headers,
				&server.url("/missing"),
				"test"
			))
			.unwrap_or_else(|error| panic!("404 response: {error}")),
		None
	);
	assert_eq!(
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(get_optional_json::<SampleResponse>(
				&client,
				&headers,
				&server.url("/present"),
				"test"
			))
			.unwrap_or_else(|error| panic!("200 response: {error}")),
		Some(SampleResponse { ok: true })
	);
	not_found.assert();
	found.assert();
}

#[test]
fn get_json_and_write_helpers_require_success_status() {
	let server = MockServer::start();
	let get_ok = server.mock(|when, then| {
		when.method(GET).path("/get");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"ok":true}"#);
	});
	let post_ok = server.mock(|when, then| {
		when.method(POST).path("/post");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"ok":true}"#);
	});
	let put_ok = server.mock(|when, then| {
		when.method(PUT).path("/put");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"ok":true}"#);
	});
	let patch_ok = server.mock(|when, then| {
		when.method(PATCH).path("/patch");
		then.status(200)
			.header("content-type", "application/json")
			.body(r#"{"ok":true}"#);
	});
	let client = build_http_client("test").unwrap_or_else(|error| panic!("client: {error}"));
	let headers = empty_headers();
	let body = SampleBody {
		name: "demo".to_string(),
	};

	assert_eq!(
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(get_json::<SampleResponse>(
				&client,
				&headers,
				&server.url("/get"),
				"test"
			))
			.unwrap_or_else(|error| panic!("get response: {error}")),
		SampleResponse { ok: true }
	);
	assert_eq!(
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(post_json::<_, SampleResponse>(
				&client,
				&headers,
				&server.url("/post"),
				&body,
				"test"
			))
			.unwrap_or_else(|error| panic!("post response: {error}")),
		SampleResponse { ok: true }
	);
	assert_eq!(
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(put_json::<_, SampleResponse>(
				&client,
				&headers,
				&server.url("/put"),
				&body,
				"test"
			))
			.unwrap_or_else(|error| panic!("put response: {error}")),
		SampleResponse { ok: true }
	);
	assert_eq!(
		tokio::runtime::Runtime::new()
			.unwrap()
			.block_on(patch_json::<_, SampleResponse>(
				&client,
				&headers,
				&server.url("/patch"),
				&body,
				"test"
			))
			.unwrap_or_else(|error| panic!("patch response: {error}")),
		SampleResponse { ok: true }
	);
	get_ok.assert();
	post_ok.assert();
	put_ok.assert();
	patch_ok.assert();
}

#[test]
fn release_pull_request_body_skips_empty_sections() {
	let mut manifest = sample_manifest();
	manifest.release_targets = vec![ReleaseManifestTarget {
		id: "sdk".to_string(),
		kind: ReleaseOwnerKind::Group,
		version: "1.2.0".to_string(),
		tag: true,
		release: true,
		version_format: VersionFormat::Primary,
		tag_name: "v1.2.0".to_string(),
		members: vec![],
		rendered_title: "title".to_string(),
		rendered_changelog_title: "changelog".to_string(),
	}];
	manifest.changelogs = vec![ReleaseManifestChangelog {
		owner_id: "sdk".to_string(),
		owner_kind: ReleaseOwnerKind::Group,
		path: PathBuf::from("changelog.md"),
		format: monochange_core::ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "1.2.0".to_string(),
			summary: vec!["Grouped release.".to_string()],
			sections: vec![
				ReleaseNotesSection {
					title: "Features".to_string(),
					collapsed: false,
					entries: vec!["- add publishing".to_string()],
				},
				ReleaseNotesSection {
					title: "Empty".to_string(),
					collapsed: false,
					entries: vec![],
				},
			],
		},
		rendered: String::new(),
	}];

	let body = release_pull_request_body(&manifest);
	assert!(!body.contains("### Empty"), "body:\n{body}");
	assert!(body.contains("### Features"), "body:\n{body}");
}

#[test]
fn get_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(GET).path("/test");
		then.status(500).body("Internal Server Error");
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let result: MonochangeResult<String> = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(get_json(&client, &headers, &server.url("/test"), "test"));

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API GET"));
	assert!(error.contains("500"));
	mock.assert();
}

#[test]
fn get_optional_json_returns_none_for_404() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(GET).path("/missing");
		then.status(404);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let result: MonochangeResult<Option<String>> = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(get_optional_json(
			&client,
			&headers,
			&server.url("/missing"),
			"test",
		));

	assert!(result.is_ok());
	assert!(result.unwrap().is_none());
	mock.assert();
}

#[test]
fn get_optional_json_returns_error_for_non_404_non_success() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(GET).path("/bad");
		then.status(500);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let result: MonochangeResult<Option<String>> = tokio::runtime::Runtime::new()
		.unwrap()
		.block_on(get_optional_json(
			&client,
			&headers,
			&server.url("/bad"),
			"test",
		));

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API GET"));
	assert!(error.contains("500"));
	mock.assert();
}

#[test]
fn post_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(POST).path("/test");
		then.status(422).body("Validation Failed");
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let body = "request body".to_string();
	let result: MonochangeResult<String> = tokio::runtime::Runtime::new().unwrap().block_on(
		post_json(&client, &headers, &server.url("/test"), &body, "test"),
	);

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API POST"));
	assert!(error.contains("422"));
	mock.assert();
}

#[test]
fn put_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(PUT).path("/test");
		then.status(403);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let body = "request body".to_string();
	let result: MonochangeResult<String> = tokio::runtime::Runtime::new().unwrap().block_on(
		put_json(&client, &headers, &server.url("/test"), &body, "test"),
	);

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API PUT"));
	assert!(error.contains("403"));
	mock.assert();
}

#[test]
fn patch_json_returns_error_for_non_success_status() {
	let server = MockServer::start();
	let mock = server.mock(|when, then| {
		when.method(PATCH).path("/test");
		then.status(409);
	});

	let client = build_http_client("test").unwrap();
	let headers = HeaderMap::new();
	let body = "request body".to_string();
	let result: MonochangeResult<String> = tokio::runtime::Runtime::new().unwrap().block_on(
		patch_json(&client, &headers, &server.url("/test"), &body, "test"),
	);

	assert!(result.is_err());
	let error = result.unwrap_err().to_string();
	assert!(error.contains("test API PATCH"));
	assert!(error.contains("409"));
	mock.assert();
}
