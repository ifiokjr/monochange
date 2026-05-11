use std::fs;

use monochange_core::ChangesetTargetKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestKind;
use monochange_core::HostedReviewRequestRef;

use super::*;

#[test]
fn resolve_changeset_path_and_discovery_cover_missing_and_empty_directories() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(tempdir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::write(tempdir.path().join(".changeset/feature.md"), "---\n")
		.unwrap_or_else(|error| panic!("write changeset: {error}"));

	let resolved = resolve_changeset_path(tempdir.path(), "feature.md")
		.unwrap_or_else(|error| panic!("resolve changeset path: {error}"));
	assert_eq!(resolved, tempdir.path().join(".changeset/feature.md"));

	let invalid = resolve_changeset_path(tempdir.path(), "notes.txt")
		.err()
		.unwrap_or_else(|| panic!("expected invalid changeset path"));
	assert!(
		invalid
			.to_string()
			.contains("requested changeset `notes.txt` does not exist")
	);

	let absolute = resolve_changeset_path(
		tempdir.path(),
		tempdir
			.path()
			.join(".changeset/feature.md")
			.to_string_lossy()
			.as_ref(),
	)
	.unwrap_or_else(|error| panic!("resolve absolute changeset path: {error}"));
	assert_eq!(absolute, tempdir.path().join(".changeset/feature.md"));

	let missing_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let missing_error = discover_changeset_paths(missing_dir.path(), false)
		.err()
		.unwrap_or_else(|| panic!("expected missing changeset directory error"));
	assert!(
		missing_error
			.to_string()
			.contains("no markdown changesets found under .changeset")
	);

	let empty_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::create_dir_all(empty_dir.path().join(".changeset"))
		.unwrap_or_else(|error| panic!("create empty changeset dir: {error}"));
	let empty_error = discover_changeset_paths(empty_dir.path(), false)
		.err()
		.unwrap_or_else(|| panic!("expected empty changeset directory error"));
	assert!(
		empty_error
			.to_string()
			.contains("no markdown changesets found under .changeset")
	);

	let blocked_dir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	fs::write(blocked_dir.path().join(".changeset"), "not a directory\n")
		.unwrap_or_else(|error| panic!("write blocking .changeset file: {error}"));
	let blocked_error = discover_changeset_paths(blocked_dir.path(), false)
		.err()
		.unwrap_or_else(|| panic!("expected read_dir error for file-backed .changeset path"));
	assert!(blocked_error.to_string().contains("failed to read"));
}

#[test]
fn render_changeset_diagnostics_renders_full_context() {
	let report = ChangesetDiagnosticsReport {
		requested_changesets: vec![PathBuf::from(".changeset/feature.md")],
		changesets: vec![PreparedChangeset {
			path: PathBuf::from(".changeset/feature.md"),
			summary: Some("ship feature".to_string()),
			details: Some("long details".to_string()),
			targets: vec![PreparedChangesetTarget {
				id: "core".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: Some(BumpSeverity::Minor),
				origin: "manual".to_string(),
				evidence_refs: vec!["src/lib.rs".to_string()],
				change_type: Some("feature".to_string()),
				caused_by: vec!["core".to_string()],
			}],
			context: Some(ChangesetContext {
				provider: HostingProviderKind::GitHub,
				host: Some("github.com".to_string()),
				capabilities: HostingCapabilities::default(),
				introduced: Some(ChangesetRevision {
					actor: None,
					commit: Some(HostedCommitRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						sha: "abc123456789".to_string(),
						short_sha: "abc1234".to_string(),
						url: Some(
							"https://github.com/example/repo/commit/abc123456789".to_string(),
						),
						authored_at: None,
						committed_at: None,
						author_name: None,
						author_email: None,
					}),
					review_request: Some(HostedReviewRequestRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						kind: HostedReviewRequestKind::PullRequest,
						id: "#42".to_string(),
						title: None,
						url: Some("https://github.com/example/repo/pull/42".to_string()),
						author: None,
					}),
				}),
				last_updated: Some(ChangesetRevision {
					actor: None,
					commit: Some(HostedCommitRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						sha: "def123456789".to_string(),
						short_sha: "def1234".to_string(),
						url: None,
						authored_at: None,
						committed_at: None,
						author_name: None,
						author_email: None,
					}),
					review_request: None,
				}),
				related_issues: vec![HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: "#99".to_string(),
					title: None,
					url: None,
					relationship: HostedIssueRelationshipKind::Mentioned,
				}],
			}),
		}],
	};

	let rendered = render_changeset_diagnostics(&report);
	assert!(rendered.contains("changeset: .changeset/feature.md"));
	assert!(rendered.contains("summary: ship feature"));
	assert!(rendered.contains("details: long details"));
	assert!(rendered.contains("- package core (bump: minor, origin: manual)"));
	assert!(rendered.contains("caused by: core"));
	assert!(rendered.contains("evidence: src/lib.rs"));
	assert!(rendered.contains("introduced: abc1234"));
	assert!(rendered.contains("last-updated: def1234"));
	assert!(rendered.contains("review request: #42 (https://github.com/example/repo/pull/42)"));
	assert!(rendered.contains("related issues: #99"));

	let minimal = render_changeset_diagnostics(&ChangesetDiagnosticsReport {
		requested_changesets: vec![PathBuf::from(".changeset/minimal.md")],
		changesets: vec![PreparedChangeset {
			path: PathBuf::from(".changeset/minimal.md"),
			summary: None,
			details: None,
			targets: Vec::new(),
			context: Some(ChangesetContext {
				provider: HostingProviderKind::GenericGit,
				host: None,
				capabilities: HostingCapabilities::default(),
				introduced: None,
				last_updated: Some(ChangesetRevision {
					actor: None,
					commit: None,
					review_request: Some(HostedReviewRequestRef {
						provider: HostingProviderKind::GitHub,
						host: Some("github.com".to_string()),
						kind: HostedReviewRequestKind::PullRequest,
						id: "#77".to_string(),
						title: None,
						url: None,
						author: None,
					}),
				}),
				related_issues: Vec::new(),
			}),
		}],
	});
	assert!(minimal.contains("summary: <missing summary>"));
	assert!(minimal.contains("review request: #77"));
	assert!(!minimal.contains("targets:"));
	assert_eq!(
		render_changeset_diagnostics(&ChangesetDiagnosticsReport {
			requested_changesets: Vec::new(),
			changesets: Vec::new(),
		}),
		"no matching changesets found"
	);
}

#[test]
fn build_prepared_changesets_uses_generic_context_without_a_git_repository() {
	let tempdir = tempfile::tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let loaded = vec![monochange_config::LoadedChangesetFile {
		path: tempdir.path().join(".changeset/feature.md"),
		summary: Some("feature".to_string()),
		details: Some("details".to_string()),
		targets: vec![monochange_config::LoadedChangesetTarget {
			id: "core".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: Some(BumpSeverity::Patch),
			explicit_version: None,
			origin: "manual".to_string(),
			evidence_refs: vec!["src/lib.rs".to_string()],
			change_type: Some("fix".to_string()),
			caused_by: Vec::new(),
		}],
		signals: Vec::new(),
	}];

	let prepared = build_prepared_changesets(tempdir.path(), &loaded);
	assert_eq!(prepared.len(), 1);
	assert!(prepared[0].path.ends_with(".changeset/feature.md"));
	assert_eq!(prepared[0].summary.as_deref(), Some("feature"));
	assert_eq!(prepared[0].details.as_deref(), Some("details"));
	assert_eq!(prepared[0].targets[0].id, "core");
	let context = prepared[0]
		.context
		.as_ref()
		.unwrap_or_else(|| panic!("expected generic git context"));
	assert_eq!(context.provider, HostingProviderKind::GenericGit);
	assert!(context.introduced.is_none());
	assert!(context.last_updated.is_none());
	assert!(context.related_issues.is_empty());
}

#[test]
fn batch_git_log_helpers_cover_empty_and_malformed_output_paths() {
	assert!(batch_load_changeset_contexts(Path::new("."), &[]).is_empty());
	assert_eq!(
		batch_git_log(Path::new("."), &[]),
		(
			std::collections::HashMap::default(),
			std::collections::HashMap::default(),
		)
	);
	assert_eq!(
		parse_batch_git_log_bytes(&[0xff, 0xfe], &[PathBuf::from(".changeset/feature.md")]),
		(
			std::collections::HashMap::default(),
			std::collections::HashMap::default(),
		)
	);

	let malformed = "\
header-without-separators
M .changeset/feature.md

sha\x1fauthor\x1femail\x1fauthored\x1fcommitted
M
M .changeset/ignored.md
";
	let (introduced, last_updated) =
		parse_batch_git_log_output(malformed, &[PathBuf::from(".changeset/feature.md")]);
	assert!(introduced.is_empty());
	assert!(last_updated.is_empty());
}
