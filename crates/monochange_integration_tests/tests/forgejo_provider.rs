use std::path::Path;
use std::process::Command;

use insta::assert_json_snapshot;
use insta::assert_snapshot;
use insta_cmd::get_cargo_bin;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ProviderReleaseSettings;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;

fn fixture_path(relative: &str) -> std::path::PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/tests")
		.join(relative)
}

fn forgejo_source() -> SourceConfiguration {
	SourceConfiguration {
		provider: SourceProvider::Forgejo,
		owner: "org".to_string(),
		repo: "monochange".to_string(),
		host: Some("https://codeberg.org".to_string()),
		api_url: Some("https://codeberg.org/api/v1".to_string()),
		releases: ProviderReleaseSettings::default(),
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

#[test]
fn forgejo_fixture_loads_and_validates_source_configuration() {
	let configuration =
		monochange_config::load_workspace_configuration(&fixture_path("source/forgejo"))
			.unwrap_or_else(|error| panic!("load forgejo fixture: {error}"));
	let source = configuration
		.source
		.unwrap_or_else(|| panic!("forgejo fixture should configure [source]"));

	assert_eq!(source.provider, SourceProvider::Forgejo);
	assert_eq!(source.host.as_deref(), Some("https://codeberg.org"));
	assert_eq!(
		source.releases.source,
		ProviderReleaseNotesSource::Monochange
	);
	assert_eq!(source.pull_requests.base, "main");
	assert_json_snapshot!(serde_json::json!({
		"provider": source.provider,
		"owner": &source.owner,
		"repo": &source.repo,
		"host": &source.host,
		"api_url": &source.api_url,
		"release_notes_source": source.releases.source,
		"pull_request_base": &source.pull_requests.base,
	}), @r###"
	{
	  "api_url": null,
	  "host": "https://codeberg.org",
	  "owner": "org",
	  "provider": "forgejo",
	  "pull_request_base": "main",
	  "release_notes_source": "monochange",
	  "repo": "monochange"
	}
	"###);
	monochange_forgejo::validate_source_configuration(&source)
		.unwrap_or_else(|error| panic!("validate forgejo source: {error}"));
}

#[test]
fn forgejo_cli_validate_accepts_fixture_configuration() {
	let output = Command::new(get_cargo_bin("mc"))
		.env("NO_COLOR", "1")
		.env_remove("RUST_LOG")
		.current_dir(fixture_path("source/forgejo"))
		.arg("step:validate")
		.output()
		.unwrap_or_else(|error| panic!("run mc step:validate: {error}"));

	assert!(
		output.status.success(),
		"mc step:validate failed\nstdout:\n{}\nstderr:\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

#[test]
fn forgejo_urls_use_configured_host_and_gitea_compatible_routes() {
	let source = forgejo_source();

	assert_json_snapshot!(serde_json::json!({
		"tag": monochange_forgejo::tag_url(&source, "v1.2.3"),
		"compare": monochange_forgejo::compare_url(&source, "v1.2.2", "v1.2.3"),
		"commit": monochange_forgejo::forgejo_commit_url(&source, "abc123"),
		"host_name": monochange_forgejo::forgejo_host_name(&source),
	}), @r###"
	{
	  "commit": "https://codeberg.org/org/monochange/commit/abc123",
	  "compare": "https://codeberg.org/org/monochange/compare/v1.2.2...v1.2.3",
	  "host_name": "codeberg.org",
	  "tag": "https://codeberg.org/org/monochange/releases/tag/v1.2.3"
	}
	"###);
}

#[test]
fn forgejo_rejects_unsupported_source_features() {
	let mut generated_notes = forgejo_source();
	generated_notes.releases.generate_notes = true;
	assert_snapshot!(
		monochange_forgejo::validate_source_configuration(&generated_notes)
			.unwrap_err()
			.to_string(),
		@"config error: provider-generated release notes are not supported for `provider = \"forgejo\"`; use `source = \"monochange\"`"
	);

	let mut auto_merge = forgejo_source();
	auto_merge.pull_requests.auto_merge = true;
	assert_snapshot!(
		monochange_forgejo::validate_source_configuration(&auto_merge)
			.unwrap_err()
			.to_string(),
		@"config error: [source.pull_requests].auto_merge is not supported for `provider = \"forgejo\"`"
	);
}

#[test]
fn forgejo_capabilities_match_hosted_support_without_trusted_publishing_claims() {
	assert_json_snapshot!(monochange_forgejo::source_capabilities(), @r###"
	{
	  "draft_releases": true,
	  "prereleases": true,
	  "generated_release_notes": false,
	  "auto_merge_change_requests": false,
	  "released_issue_comments": false,
	  "requires_host": true
	}
	"###);
	assert_json_snapshot!(monochange_forgejo::forgejo_hosting_capabilities(), @r###"
	{
	  "commitWebUrls": true,
	  "actorProfiles": false,
	  "reviewRequestLookup": false,
	  "relatedIssues": false,
	  "issueComments": false
	}
	"###);
}
