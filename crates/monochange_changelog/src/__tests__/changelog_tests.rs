use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::path::PathBuf;

use monochange_core::ChangeSignal;
use monochange_core::ChangelogFormat;
use monochange_core::ChangelogSectionDef;
use monochange_core::ChangelogSettings;
use monochange_core::ChangelogTarget;
use monochange_core::ChangelogType;
use monochange_core::ChangesetContext;
use monochange_core::ChangesetRevision;
use monochange_core::ChangesetTargetKind;
use monochange_core::Ecosystem;
use monochange_core::GroupChangelogInclude;
use monochange_core::GroupDefinition;
use monochange_core::HostedActorRef;
use monochange_core::HostedActorSourceKind;
use monochange_core::HostedCommitRef;
use monochange_core::HostedIssueRef;
use monochange_core::HostedIssueRelationshipKind;
use monochange_core::HostedReviewRequestKind;
use monochange_core::HostedReviewRequestRef;
use monochange_core::HostingCapabilities;
use monochange_core::HostingProviderKind;
use monochange_core::PackageDefinition;
use monochange_core::PackageRecord;
use monochange_core::PackageType;
use monochange_core::PlannedVersionGroup;
use monochange_core::PreparedChangeset;
use monochange_core::PreparedChangesetTarget;
use monochange_core::PublishState;
use monochange_core::ReleaseDecision;
use monochange_core::VersionFormat;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use semver::Version;
use tempfile::tempdir;

use super::*;

fn empty_configuration(root: &Path) -> WorkspaceConfiguration {
	WorkspaceConfiguration {
		root_path: root.to_path_buf(),
		defaults: WorkspaceDefaults::default(),
		changelog: ChangelogSettings::default(),
		packages: Vec::new(),
		groups: Vec::new(),
		cli: Vec::new(),
		changesets: monochange_core::ChangesetSettings::default(),
		source: None,
		lints: monochange_core::lint::WorkspaceLintSettings::default(),
		cargo: monochange_core::EcosystemSettings::default(),
		npm: monochange_core::EcosystemSettings::default(),
		deno: monochange_core::EcosystemSettings::default(),
		dart: monochange_core::EcosystemSettings::default(),
		python: monochange_core::EcosystemSettings::default(),
		go: monochange_core::EcosystemSettings::default(),
	}
}

fn sample_package_record(root: &Path, config_id: &str, name: &str) -> PackageRecord {
	let manifest_dir = root.join("packages").join(config_id);
	fs::create_dir_all(&manifest_dir)
		.unwrap_or_else(|error| panic!("create manifest dir: {error}"));
	let manifest_path = manifest_dir.join("package.json");
	fs::write(&manifest_path, "{}\n")
		.unwrap_or_else(|error| panic!("write manifest file: {error}"));

	let mut package = PackageRecord::new(
		Ecosystem::Npm,
		name,
		manifest_path,
		root.to_path_buf(),
		Some(Version::new(1, 0, 0)),
		PublishState::Public,
	);
	package
		.metadata
		.insert("config_id".to_string(), config_id.to_string());
	package
}

fn sample_package_definition(config_id: &str) -> PackageDefinition {
	PackageDefinition {
		id: config_id.to_string(),
		path: PathBuf::from(format!("packages/{config_id}")),
		package_type: PackageType::Npm,
		changelog: Some(ChangelogTarget {
			path: PathBuf::from(format!("packages/{config_id}/CHANGELOG.md")),
			format: ChangelogFormat::Monochange,
			initial_header: None,
		}),
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		publish: monochange_core::PublishSettings::default(),
		version_format: VersionFormat::Namespaced,
	}
}

fn sample_group_definition(include: GroupChangelogInclude) -> GroupDefinition {
	GroupDefinition {
		id: "sdk".to_string(),
		packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
		changelog: Some(ChangelogTarget {
			path: PathBuf::from("groups/sdk/CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
			initial_header: None,
		}),
		changelog_include: include,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: None,
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
	}
}

fn sample_decision(package_id: &str, group_id: Option<&str>) -> ReleaseDecision {
	ReleaseDecision {
		package_id: package_id.to_string(),
		trigger_type: "changeset".to_string(),
		recommended_bump: BumpSeverity::Patch,
		planned_version: Some(Version::new(1, 2, 3)),
		group_id: group_id.map(ToString::to_string),
		reasons: vec!["covered".to_string()],
		upstream_sources: Vec::new(),
		warnings: Vec::new(),
	}
}

fn sample_group(member_ids: Vec<String>) -> PlannedVersionGroup {
	PlannedVersionGroup {
		group_id: "sdk".to_string(),
		display_name: "SDK".to_string(),
		members: member_ids,
		mismatch_detected: false,
		planned_version: Some(Version::new(2, 0, 0)),
		recommended_bump: BumpSeverity::Minor,
	}
}

fn sample_change(package_id: &str, package_name: &str, source_path: &str) -> ReleaseNoteChange {
	ReleaseNoteChange {
		package_id: package_id.to_string(),
		package_name: package_name.to_string(),
		package_labels: Vec::new(),
		source_path: Some(source_path.to_string()),
		summary: "Added release note support".to_string(),
		details: Some("Detailed explanation".to_string()),
		bump: BumpSeverity::Minor,
		change_type: Some("note".to_string()),
		context: Some("> _Owner:_ @octocat".to_string()),
		changeset_path: Some(source_path.to_string()),
		change_owner: Some("@octocat".to_string()),
		change_owner_link: Some("[@octocat](https://example.com/octocat)".to_string()),
		review_request: Some("PR 42".to_string()),
		review_request_link: Some("[PR 42](https://example.com/pr/42)".to_string()),
		introduced_commit: Some("abc1234".to_string()),
		introduced_commit_link: Some("[`abc1234`](https://example.com/commit/abc1234)".to_string()),
		last_updated_commit: Some("def5678".to_string()),
		last_updated_commit_link: Some(
			"[`def5678`](https://example.com/commit/def5678)".to_string(),
		),
		related_issues: Some("#10".to_string()),
		related_issue_links: Some("[#10](https://example.com/issues/10)".to_string()),
		closed_issues: Some("#20".to_string()),
		closed_issue_links: Some("[#20](https://example.com/issues/20)".to_string()),
	}
}

#[test]
fn initial_changelog_header_renders_custom_template_and_format_default() {
	let mut metadata = BTreeMap::new();
	metadata.insert("package_name", "workflow-core".to_string());
	metadata.insert("monochange_version", "1.2.3".to_string());

	let custom = ChangelogTarget {
		path: PathBuf::from("CHANGELOG.md"),
		format: ChangelogFormat::Monochange,
		initial_header: Some(
			"# {{ package_name }}\n\nGenerated by {{ monochange_version }}".to_string(),
		),
	};
	assert_eq!(
		render_initial_changelog_header(&custom, &metadata),
		"# workflow-core\n\nGenerated by 1.2.3"
	);

	let format_default = ChangelogTarget {
		path: PathBuf::from("CHANGELOG.md"),
		format: ChangelogFormat::KeepAChangelog,
		initial_header: None,
	};
	assert!(
		render_initial_changelog_header(&format_default, &metadata).contains("Keep a Changelog")
	);
}

#[test]
fn build_changelog_updates_writes_package_and_group_initial_headers() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let package_a = sample_package_record(root, "pkg-a", "pkg-a");
	let package_b = sample_package_record(root, "pkg-b", "pkg-b");
	let package_a_id = package_a.id.clone();
	let other_package_id = package_b.id.clone();
	let packages = vec![package_a, package_b];

	let mut configuration = empty_configuration(root);
	configuration.packages = vec![
		sample_package_definition("pkg-a"),
		sample_package_definition("pkg-b"),
	];
	configuration.groups = vec![sample_group_definition(GroupChangelogInclude::All)];

	let plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![sample_decision(&package_a_id, None)],
		groups: vec![sample_group(vec![package_a_id.clone(), other_package_id])],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let package_changelog_path = root.join("packages/pkg-a/CHANGELOG.md");
	let group_changelog_path = root.join("groups/sdk/CHANGELOG.md");
	let changelog_targets = (
		BTreeMap::from([(
			package_a_id,
			ChangelogTarget {
				path: package_changelog_path.clone(),
				format: ChangelogFormat::Monochange,
				initial_header: Some("# {{ package_name }} changelog".to_string()),
			},
		)]),
		BTreeMap::from([(
			"sdk".to_string(),
			ChangelogTarget {
				path: group_changelog_path.clone(),
				format: ChangelogFormat::Monochange,
				initial_header: Some("# {{ group_name }} changelog".to_string()),
			},
		)]),
	);

	let updates = build_changelog_updates(ChangelogBuildContext {
		root,
		configuration: &configuration,
		packages: &packages,
		plan: &plan,
		change_signals: &[],
		changesets: &[],
		changelog_targets: &changelog_targets,
		release_targets: &[],
	})
	.unwrap_or_else(|error| panic!("build changelog updates: {error}"));

	let package_update = updates
		.iter()
		.find(|update| update.file.path == package_changelog_path)
		.unwrap_or_else(|| panic!("missing package changelog update"));
	assert!(String::from_utf8_lossy(&package_update.file.content).starts_with("# pkg-a changelog"));

	let group_update = updates
		.iter()
		.find(|update| update.file.path == group_changelog_path)
		.unwrap_or_else(|| panic!("missing group changelog update"));
	assert!(String::from_utf8_lossy(&group_update.file.content).starts_with("# sdk changelog"));
}

#[test]
fn build_changelog_updates_reports_package_and_group_append_errors() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let package_a = sample_package_record(root, "pkg-a", "pkg-a");
	let package_a_id = package_a.id.clone();
	let packages = vec![package_a];

	let mut configuration = empty_configuration(root);
	configuration.packages = vec![sample_package_definition("pkg-a")];
	configuration.groups = vec![sample_group_definition(GroupChangelogInclude::All)];

	let bad_changelog_path = root.join("invalid-utf8-changelog.md");
	fs::write(&bad_changelog_path, [0xff])
		.unwrap_or_else(|error| panic!("write invalid changelog file: {error}"));

	let package_plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![sample_decision(&package_a_id, None)],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let package_error = build_changelog_updates(ChangelogBuildContext {
		root,
		configuration: &configuration,
		packages: &packages,
		plan: &package_plan,
		change_signals: &[],
		changesets: &[],
		changelog_targets: &(
			BTreeMap::from([(
				package_a_id.clone(),
				ChangelogTarget {
					path: bad_changelog_path.clone(),
					format: ChangelogFormat::Monochange,
					initial_header: None,
				},
			)]),
			BTreeMap::new(),
		),
		release_targets: &[],
	});
	assert!(package_error.is_err());

	let group_plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![sample_decision(&package_a_id, Some("sdk"))],
		groups: vec![sample_group(vec![package_a_id.clone()])],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};
	let group_error = build_changelog_updates(ChangelogBuildContext {
		root,
		configuration: &configuration,
		packages: &packages,
		plan: &group_plan,
		change_signals: &[],
		changesets: &[],
		changelog_targets: &(
			BTreeMap::new(),
			BTreeMap::from([(
				"sdk".to_string(),
				ChangelogTarget {
					path: bad_changelog_path,
					format: ChangelogFormat::Monochange,
					initial_header: None,
				},
			)]),
		),
		release_targets: &[],
	});
	assert!(group_error.is_err());
}

#[test]
fn changelog_file_helpers_append_and_deduplicate_updates() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let changelog_path = tempdir.path().join("CHANGELOG.md");

	let first =
		append_changelog_section(&changelog_path, "## 1.0.0\n- initial", Some("# Changelog"))
			.unwrap_or_else(|error| panic!("append first changelog section: {error}"));
	assert_eq!(first, "# Changelog\n\n## 1.0.0\n- initial\n");

	let without_header_path = tempdir.path().join("without-header.md");
	let without_header =
		append_changelog_section(&without_header_path, "## 1.0.0\n- initial", None)
			.unwrap_or_else(|error| panic!("append changelog section without header: {error}"));
	assert_eq!(without_header, "## 1.0.0\n- initial\n");

	fs::write(&changelog_path, "# Changelog\n\n## 0.9.0\n- older\n")
		.unwrap_or_else(|error| panic!("write existing changelog: {error}"));
	let appended =
		append_changelog_section(&changelog_path, "## 1.0.0\n- latest", Some("# Ignored"))
			.unwrap_or_else(|error| panic!("append second changelog section: {error}"));
	assert_eq!(
		appended,
		"# Changelog\n\n## 1.0.0\n- latest\n\n## 0.9.0\n- older\n"
	);

	let earlier = ChangelogUpdate {
		file: FileUpdate {
			path: changelog_path.clone(),
			content: b"old".to_vec(),
		},
		owner_id: "pkg-a".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "0.9.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: "old".to_string(),
	};
	let latest = ChangelogUpdate {
		file: FileUpdate {
			path: changelog_path.clone(),
			content: b"new".to_vec(),
		},
		owner_id: "pkg-a".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "1.0.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: "new".to_string(),
	};
	let unique_path = tempdir.path().join("OTHER.md");
	let unique = ChangelogUpdate {
		file: FileUpdate {
			path: unique_path.clone(),
			content: b"other".to_vec(),
		},
		owner_id: "pkg-b".to_string(),
		owner_kind: ReleaseOwnerKind::Package,
		format: ChangelogFormat::Monochange,
		notes: ReleaseNotesDocument {
			title: "1.0.0".to_string(),
			summary: Vec::new(),
			sections: Vec::new(),
		},
		rendered: "other".to_string(),
	};

	let deduped = dedup_changelog_updates(vec![earlier, latest.clone(), unique]);
	assert_eq!(deduped.len(), 2);
	assert!(deduped.iter().any(|update| update.file.path == unique_path));
	assert!(deduped.iter().any(|update| {
		update.file.path == changelog_path && update.file.content == latest.file.content
	}));
}

#[test]
fn rendered_changeset_context_and_signal_mapping_cover_hosted_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let changeset_path = tempdir.path().join(".changeset/feature.md");
	fs::create_dir_all(changeset_path.parent().unwrap_or_else(|| Path::new(".")))
		.unwrap_or_else(|error| panic!("create changeset dir: {error}"));
	fs::write(&changeset_path, "feature\n")
		.unwrap_or_else(|error| panic!("write changeset file: {error}"));

	let changeset = PreparedChangeset {
		path: changeset_path.clone(),
		summary: Some("summary".to_string()),
		details: Some("details".to_string()),
		targets: vec![PreparedChangesetTarget {
			id: "pkg-a".to_string(),
			kind: ChangesetTargetKind::Package,
			bump: Some(BumpSeverity::Minor),
			origin: "manual".to_string(),
			evidence_refs: Vec::new(),
			change_type: Some("note".to_string()),
			caused_by: Vec::new(),
		}],
		context: Some(ChangesetContext {
			provider: HostingProviderKind::GitHub,
			host: Some("github.com".to_string()),
			capabilities: HostingCapabilities::default(),
			introduced: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: Some("1".to_string()),
					login: Some("octocat".to_string()),
					display_name: Some("Octo Cat".to_string()),
					url: Some("https://example.com/octocat".to_string()),
					source: HostedActorSourceKind::ReviewRequestAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					sha: "abcdef123456".to_string(),
					short_sha: "abcdef1".to_string(),
					url: Some("https://example.com/commit/abcdef1".to_string()),
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: Some(HostedReviewRequestRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					kind: HostedReviewRequestKind::PullRequest,
					id: "42".to_string(),
					title: Some("release notes".to_string()),
					url: Some("https://example.com/pull/42".to_string()),
					author: None,
				}),
			}),
			last_updated: Some(ChangesetRevision {
				actor: Some(HostedActorRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: None,
					login: None,
					display_name: Some("Release Bot".to_string()),
					url: None,
					source: HostedActorSourceKind::CommitAuthor,
				}),
				commit: Some(HostedCommitRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					sha: "9876543210ab".to_string(),
					short_sha: "9876543".to_string(),
					url: Some("https://example.com/commit/9876543".to_string()),
					authored_at: None,
					committed_at: None,
					author_name: None,
					author_email: None,
				}),
				review_request: Some(HostedReviewRequestRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					kind: HostedReviewRequestKind::MergeRequest,
					id: "77".to_string(),
					title: None,
					url: None,
					author: None,
				}),
			}),
			related_issues: vec![
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: "#123".to_string(),
					title: Some("closed".to_string()),
					url: Some("https://example.com/issues/123".to_string()),
					relationship: HostedIssueRelationshipKind::ClosedByReviewRequest,
				},
				HostedIssueRef {
					provider: HostingProviderKind::GitHub,
					host: Some("github.com".to_string()),
					id: "#456".to_string(),
					title: Some("related".to_string()),
					url: None,
					relationship: HostedIssueRelationshipKind::ReferencedByReviewRequest,
				},
			],
		}),
	};

	let rendered = build_rendered_changeset_context(tempdir.path(), &changeset);
	assert!(
		rendered
			.context
			.contains("> _Owner:_ [@octocat](https://example.com/octocat)")
	);
	assert!(
		rendered
			.context
			.contains("> _Review:_ [PR 42](https://example.com/pull/42)")
	);
	assert!(
		rendered
			.context
			.contains("> _Introduced in:_ [`abcdef1`](https://example.com/commit/abcdef1)")
	);
	assert!(
		rendered
			.context
			.contains("> _Last updated in:_ [`9876543`](https://example.com/commit/9876543)")
	);
	assert!(
		rendered
			.context
			.contains("> _Closed issues:_ [#123](https://example.com/issues/123)")
	);
	assert!(rendered.context.contains("> _Related issues:_ #456"));
	assert_eq!(rendered.change_owner.as_deref(), Some("@octocat"));
	assert_eq!(rendered.review_request.as_deref(), Some("PR 42"));
	assert_eq!(rendered.closed_issues.as_deref(), Some("#123"));
	assert_eq!(rendered.related_issues.as_deref(), Some("#456"));

	let package = sample_package_record(tempdir.path(), "pkg-a", "package-a");
	let signal = ChangeSignal {
		package_id: package.id.clone(),
		requested_bump: Some(BumpSeverity::Minor),
		explicit_version: None,
		change_origin: "changeset".to_string(),
		evidence_refs: vec!["manual".to_string()],
		notes: Some("Added release note support".to_string()),
		details: Some("Detailed explanation".to_string()),
		change_type: Some("note".to_string()),
		caused_by: Vec::new(),
		source_path: changeset_path.clone(),
	};
	let source_path = root_relative(tempdir.path(), &changeset.path);
	let mapped = build_release_note_change(
		&signal,
		std::slice::from_ref(&package),
		tempdir.path(),
		&BTreeMap::from([(source_path, rendered.clone())]),
	)
	.unwrap_or_else(|| panic!("expected mapped release note change"));
	assert_eq!(mapped.package_name, "pkg-a");
	assert_eq!(mapped.change_owner.as_deref(), Some("@octocat"));
	assert_eq!(
		mapped.review_request_link.as_deref(),
		Some("[PR 42](https://example.com/pull/42)")
	);
	assert_eq!(mapped.related_issue_links.as_deref(), Some("#456"));

	let no_notes = ChangeSignal {
		notes: None,
		..signal
	};
	assert!(
		build_release_note_change(&no_notes, &[package], tempdir.path(), &BTreeMap::new())
			.is_none()
	);
}

#[test]
fn render_helpers_cover_actor_labels_links_sections_and_templates() {
	assert_eq!(
		render_actor_label(&HostedActorRef {
			login: Some("octocat".to_string()),
			source: HostedActorSourceKind::CommitAuthor,
			..HostedActorRef::default()
		}),
		"@octocat"
	);
	assert_eq!(
		render_actor_label(&HostedActorRef {
			display_name: Some("Release Bot".to_string()),
			source: HostedActorSourceKind::CommitAuthor,
			..HostedActorRef::default()
		}),
		"Release Bot"
	);
	assert_eq!(
		render_actor_label(&HostedActorRef {
			source: HostedActorSourceKind::CommitAuthor,
			..HostedActorRef::default()
		}),
		"unknown"
	);
	assert_eq!(
		render_review_request_label(&HostedReviewRequestRef {
			kind: HostedReviewRequestKind::PullRequest,
			id: "12".to_string(),
			..HostedReviewRequestRef::default()
		}),
		"PR 12"
	);
	assert_eq!(
		render_review_request_label(&HostedReviewRequestRef {
			kind: HostedReviewRequestKind::MergeRequest,
			id: "9".to_string(),
			..HostedReviewRequestRef::default()
		}),
		"MR 9"
	);
	assert_eq!(render_markdown_link("plain", None), "plain");
	assert_eq!(
		render_markdown_link("linked", Some("https://example.com")),
		"[linked](https://example.com)"
	);
	let issues = [
		HostedIssueRef {
			id: "#1".to_string(),
			url: Some("https://example.com/issues/1".to_string()),
			relationship: HostedIssueRelationshipKind::Mentioned,
			..HostedIssueRef::default()
		},
		HostedIssueRef {
			id: "#2".to_string(),
			url: None,
			relationship: HostedIssueRelationshipKind::Manual,
			..HostedIssueRef::default()
		},
	];
	let issue_refs = issues.iter().collect::<Vec<_>>();
	assert_eq!(render_issue_labels(&issue_refs), "#1, #2");
	assert_eq!(
		render_issue_links(&issue_refs),
		"[#1](https://example.com/issues/1), #2"
	);

	let mut multi_label_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
	multi_label_change.package_labels = vec!["pkg-a".to_string(), "pkg-b".to_string()];
	let block = format_group_labeled_entry(&multi_label_change, "#### Summary\n\nMore");
	assert!(block.contains("> [!NOTE]"));
	assert!(block.contains("*pkg-a*, *pkg-b*"));

	let mut single_label_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
	single_label_change.package_labels = vec!["pkg-a".to_string()];
	assert_eq!(
		format_group_labeled_entry(&single_label_change, "- Added release note support"),
		"- **pkg-a**: Added release note support"
	);

	let rendered = apply_change_template(
		"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}\n\n{{ change_owner_link }}\n\n{{ review_request_link }}\n\n{{ introduced_commit_link }}\n\n{{ last_updated_commit_link }}\n\n{{ related_issue_links }}\n\n{{ closed_issue_links }}",
		&sample_change("pkg-a", "pkg-a", ".changeset/a.md"),
		"sdk",
		"1.2.3",
	)
	.unwrap_or_else(|| panic!("expected template to render"));
	assert!(rendered.contains("Detailed explanation"));
	assert!(rendered.contains("[@octocat](https://example.com/octocat)"));
	assert!(rendered.contains("[PR 42](https://example.com/pr/42)"));
	assert!(rendered.contains("[#10](https://example.com/issues/10)"));
	assert!(rendered.contains("[#20](https://example.com/issues/20)"));
	assert!(
		apply_change_template(
			"{{ missing_value }}",
			&sample_change("pkg-a", "pkg-a", ".changeset/a.md"),
			"sdk",
			"1.2.3"
		)
		.is_none()
	);
	assert!(
		apply_change_template(
			"   ",
			&sample_change("pkg-a", "pkg-a", ".changeset/a.md"),
			"sdk",
			"1.2.3"
		)
		.is_none()
	);

	let mut extra_settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	extra_settings.sections.insert(
		"highlights".to_string(),
		ChangelogSectionDef {
			heading: "Highlights".to_string(),
			description: None,
			priority: 20,
		},
	);
	extra_settings.sections.insert(
		"notes".to_string(),
		ChangelogSectionDef {
			heading: "Notes".to_string(),
			description: None,
			priority: 100,
		},
	);
	extra_settings.types.insert(
		"minor".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "highlights".to_string(),
			description: None,
		},
	);
	extra_settings.types.insert(
		"note".to_string(),
		ChangelogType {
			bump: BumpSeverity::None,
			section: "notes".to_string(),
			description: None,
		},
	);
	let sections = render_release_note_sections(
		"sdk",
		"1.2.3",
		&extra_settings,
		&[
			ReleaseNoteChange {
				change_type: Some("note".to_string()),
				..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
			},
			ReleaseNoteChange {
				change_type: Some("minor".to_string()),
				bump: BumpSeverity::Minor,
				summary: "Added group support".to_string(),
				..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
			},
			ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Major,
				summary: "Breaking API".to_string(),
				..sample_change("pkg-c", "pkg-c", ".changeset/c.md")
			},
			ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Patch,
				summary: "Bug fix".to_string(),
				..sample_change("pkg-d", "pkg-d", ".changeset/d.md")
			},
			ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Patch,
				summary: "Bug fix".to_string(),
				..sample_change("pkg-d", "pkg-d", ".changeset/d.md")
			},
		],
	);
	assert_eq!(sections[0].title, "Highlights");
	assert_eq!(sections[1].title, "Notes");
	assert_eq!(sections[2].title, "Changed");
	assert_eq!(
		sections[0].entries,
		vec!["- Added group support".to_string()]
	);
	assert_eq!(
		sections[2].entries,
		vec!["- Breaking API".to_string(), "- Bug fix".to_string()]
	);

	let fallback = render_release_note_sections("sdk", "1.2.3", &ChangelogSettings::default(), &[]);
	assert_eq!(fallback[0].title, "Changed");
	assert_eq!(fallback[0].entries, vec!["- prepare release".to_string()]);

	let document = build_release_notes_document(
		"sdk",
		"1.2.3",
		vec!["Summary".to_string()],
		&extra_settings,
		&[sample_change("pkg-a", "pkg-a", ".changeset/a.md")],
	);
	assert_eq!(document.title, "1.2.3");
	assert_eq!(document.summary, vec!["Summary".to_string()]);
	assert!(!document.sections.is_empty());
}

#[test]
fn package_and_group_release_note_helpers_cover_empty_filtered_and_aggregated_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let package_a = sample_package_record(root, "pkg-a", "package-a");
	let package_b = sample_package_record(root, "pkg-b", "package-b");
	let mut configuration = empty_configuration(root);
	configuration.defaults.empty_update_message =
		Some("Default release for {{ package }} {{ version }}".to_string());
	let mut package_definition = sample_package_definition("pkg-a");
	package_definition.empty_update_message =
		Some("Package release for {{ package }} in {{ group }} -> {{ version }}".to_string());
	let mut group_definition =
		sample_group_definition(GroupChangelogInclude::Selected(BTreeSet::from([
			"pkg-a".to_string()
		])));
	group_definition.empty_update_message =
		Some("Group {{ group }} now at {{ version }} with {{ member_count }} members".to_string());
	configuration.packages = vec![package_definition.clone()];
	configuration.groups = vec![group_definition.clone()];

	let package_changes = package_release_note_changes(
		&configuration,
		Some(&package_definition),
		Some(&group_definition),
		&sample_decision("pkg-a", Some("sdk")),
		&package_a,
		None,
		"1.2.3",
	);
	assert_eq!(package_changes.len(), 1);
	assert!(
		package_changes[0]
			.summary
			.contains("Package release for package-a in sdk -> 1.2.3")
	);

	let direct_change = sample_change("pkg-a", "pkg-a", ".changeset/a.md");
	let direct = package_release_note_changes(
		&configuration,
		Some(&package_definition),
		Some(&group_definition),
		&sample_decision("pkg-a", Some("sdk")),
		&package_a,
		Some(&vec![direct_change.clone()]),
		"1.2.3",
	);
	assert_eq!(direct, vec![direct_change.clone()]);

	let group = sample_group(vec![package_a.id.clone(), package_b.id.clone()]);
	let group_empty = group_release_note_changes(
		&configuration,
		Some(&group_definition),
		&group,
		&BTreeMap::new(),
		&BTreeMap::new(),
		&[package_a.clone(), package_b.clone()],
		"2.0.0",
	);
	assert_eq!(group_empty.len(), 1);
	assert!(
		group_empty[0]
			.summary
			.contains("Group sdk now at 2.0.0 with 2 members")
	);

	let changes_by_package = BTreeMap::from([
		(
			package_a.id.clone(),
			vec![
				sample_change(&package_a.id, "pkg-a", ".changeset/shared.md"),
				sample_change(&package_a.id, "pkg-a", ".changeset/shared.md"),
			],
		),
		(
			package_b.id.clone(),
			vec![sample_change(
				&package_b.id,
				"pkg-b",
				".changeset/shared.md",
			)],
		),
	]);
	let targets_by_path = BTreeMap::from([(
		PathBuf::from(".changeset/shared.md"),
		vec![
			PreparedChangesetTarget {
				id: "pkg-a".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: Some(BumpSeverity::Minor),
				origin: "changeset".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
				caused_by: Vec::new(),
			},
			PreparedChangesetTarget {
				id: "pkg-b".to_string(),
				kind: ChangesetTargetKind::Package,
				bump: Some(BumpSeverity::Minor),
				origin: "changeset".to_string(),
				evidence_refs: Vec::new(),
				change_type: None,
				caused_by: Vec::new(),
			},
		],
	)]);
	let aggregate_group_definition = sample_group_definition(GroupChangelogInclude::All);
	let grouped = group_release_note_changes(
		&configuration,
		Some(&aggregate_group_definition),
		&group,
		&changes_by_package,
		&targets_by_path,
		&[package_a.clone(), package_b.clone()],
		"2.0.0",
	);
	assert_eq!(grouped.len(), 1);
	assert_eq!(
		grouped[0].package_labels,
		vec!["pkg-a".to_string(), "pkg-b".to_string()]
	);
	assert_eq!(grouped[0].package_name, "pkg-a, pkg-b");

	let selected_filtered = group_release_note_changes(
		&configuration,
		Some(&group_definition),
		&group,
		&changes_by_package,
		&targets_by_path,
		&[package_a.clone(), package_b.clone()],
		"2.0.0",
	);
	assert_eq!(selected_filtered.len(), 1);
	assert!(
		selected_filtered[0]
			.summary
			.contains("No group-facing notes were recorded")
	);

	let group_only_definition = sample_group_definition(GroupChangelogInclude::GroupOnly);
	let filtered = group_release_note_changes(
		&configuration,
		Some(&group_only_definition),
		&group,
		&changes_by_package,
		&targets_by_path,
		&[package_a.clone(), package_b.clone()],
		"2.0.0",
	);
	assert_eq!(filtered.len(), 1);
	assert!(
		filtered[0]
			.summary
			.contains("No group-facing notes were recorded")
	);

	let mut include_targets = BTreeSet::new();
	include_targets.insert("pkg-a".to_string());
	assert!(group_changelog_include_allows(
		&GroupChangelogInclude::All,
		&include_targets
	));
	assert!(!group_changelog_include_allows(
		&GroupChangelogInclude::GroupOnly,
		&include_targets,
	));
	assert!(group_changelog_include_allows(
		&GroupChangelogInclude::Selected(BTreeSet::from(["pkg-a".to_string()])),
		&include_targets,
	));
	assert!(!group_changelog_include_allows(
		&GroupChangelogInclude::Selected(BTreeSet::from(["pkg-b".to_string()])),
		&include_targets,
	));

	let group_target_map = BTreeMap::from([(
		PathBuf::from(".changeset/group.md"),
		vec![PreparedChangesetTarget {
			id: "sdk".to_string(),
			kind: ChangesetTargetKind::Group,
			bump: Some(BumpSeverity::Minor),
			origin: "changeset".to_string(),
			evidence_refs: Vec::new(),
			change_type: None,
			caused_by: Vec::new(),
		}],
	)]);
	let group_target_change = sample_change("pkg-a", "pkg-a", ".changeset/group.md");
	let filtered_group_target = filter_group_release_note_change(
		&group_target_change,
		Some(&group_definition),
		&group,
		&group_target_map,
	)
	.unwrap_or_else(|| panic!("expected group target to be included"));
	assert_eq!(filtered_group_target.package_name, "sdk");
	assert!(
		filter_group_release_note_change(
			&ReleaseNoteChange {
				source_path: Some(".changeset/unknown.md".to_string()),
				..sample_change("pkg-a", "pkg-a", ".changeset/group.md")
			},
			Some(&group_definition),
			&group,
			&group_target_map,
		)
		.is_none()
	);

	assert_eq!(
		group_release_summary("sdk"),
		vec!["Grouped release for `sdk`.".to_string()]
	);
	assert_eq!(
		render_group_filtered_update_message("sdk"),
		"No group-facing notes were recorded for this release. Member packages were updated as part of the synchronized group `sdk` version, but their changes are not configured for inclusion in this changelog."
	);
}

#[test]
fn config_and_section_helpers_cover_package_ids_and_section_resolution() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let mut configured = sample_package_record(tempdir.path(), "pkg-a", "package-a");
	assert_eq!(config_package_id(&configured), "pkg-a");
	configured.metadata.clear();
	assert_eq!(config_package_id(&configured), "package-a");
	let selected = ResolvedSectionDefinition {
		types: vec!["custom".to_string(), " minor ".to_string()],
	};
	assert!(section_matches_resolved_type(&selected, "custom"));
	assert!(section_matches_resolved_type(&selected, "minor"));

	// When change_type is "note" and doesn't match any section, it falls to bump match.
	// The sample_change has bump: Minor, which matches the selected section's " minor " type.
	assert_eq!(
		classify_release_note_change(
			&ReleaseNoteChange {
				change_type: Some("note".to_string()),
				..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
			},
			std::slice::from_ref(&selected),
		),
		ResolvedReleaseSectionTarget::Section(0)
	);
	assert_eq!(
		classify_release_note_change(
			&ReleaseNoteChange {
				change_type: Some("custom".to_string()),
				..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
			},
			std::slice::from_ref(&selected),
		),
		ResolvedReleaseSectionTarget::Section(0)
	);
	assert_eq!(
		classify_release_note_change(
			&ReleaseNoteChange {
				change_type: None,
				bump: BumpSeverity::Minor,
				..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
			},
			std::slice::from_ref(&selected),
		),
		ResolvedReleaseSectionTarget::Section(0)
	);
}

#[test]
fn render_sections_groups_entries_by_type_section_key() {
	let mut settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	settings.sections.insert(
		"breaking_change".to_string(),
		ChangelogSectionDef {
			heading: "Breaking Changes".to_string(),
			description: None,
			priority: 5,
		},
	);
	settings.sections.insert(
		"features".to_string(),
		ChangelogSectionDef {
			heading: "Features".to_string(),
			description: None,
			priority: 10,
		},
	);
	settings.sections.insert(
		"bug_fixes".to_string(),
		ChangelogSectionDef {
			heading: "Bug Fixes".to_string(),
			description: None,
			priority: 20,
		},
	);
	settings.types.insert(
		"feat".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
		},
	);
	settings.types.insert(
		"fix".to_string(),
		ChangelogType {
			bump: BumpSeverity::Patch,
			section: "bug_fixes".to_string(),
			description: None,
		},
	);
	settings.types.insert(
		"breaking".to_string(),
		ChangelogType {
			bump: BumpSeverity::Major,
			section: "breaking_change".to_string(),
			description: None,
		},
	);

	let changes = vec![
		ReleaseNoteChange {
			change_type: Some("feat".to_string()),
			bump: BumpSeverity::Minor,
			summary: "add feature".to_string(),
			..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
		},
		ReleaseNoteChange {
			change_type: Some("fix".to_string()),
			bump: BumpSeverity::Patch,
			summary: "fix bug".to_string(),
			..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
		},
		ReleaseNoteChange {
			change_type: Some("breaking".to_string()),
			bump: BumpSeverity::Major,
			summary: "remove deprecated API".to_string(),
			..sample_change("pkg-c", "pkg-c", ".changeset/c.md")
		},
	];

	let sections = render_release_note_sections("sdk", "1.0.0", &settings, &changes);

	// Sections should be ordered by priority (lower = first)
	assert_eq!(sections.len(), 3);
	assert_eq!(sections[0].title, "Breaking Changes");
	assert_eq!(sections[1].title, "Features");
	assert_eq!(sections[2].title, "Bug Fixes");

	// Entries should be under the correct section
	assert_eq!(
		sections[0].entries,
		vec!["- remove deprecated API".to_string()]
	);
	assert_eq!(sections[1].entries, vec!["- add feature".to_string()]);
	assert_eq!(sections[2].entries, vec!["- fix bug".to_string()]);
}

#[test]
fn render_sections_with_multiple_types_routing_to_same_section() {
	let mut settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	settings.sections.insert(
		"features".to_string(),
		ChangelogSectionDef {
			heading: "Features".to_string(),
			description: None,
			priority: 10,
		},
	);
	settings.types.insert(
		"feat".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
		},
	);
	settings.types.insert(
		"minor".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
		},
	);

	let changes = vec![
		ReleaseNoteChange {
			change_type: Some("feat".to_string()),
			bump: BumpSeverity::Minor,
			summary: "add feature".to_string(),
			..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
		},
		ReleaseNoteChange {
			change_type: Some("minor".to_string()),
			bump: BumpSeverity::Minor,
			summary: "minor improvement".to_string(),
			..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
		},
	];

	let sections = render_release_note_sections("sdk", "1.0.0", &settings, &changes);

	// Both feat and minor should appear under the same "Features" section
	assert_eq!(sections.len(), 1);
	assert_eq!(sections[0].title, "Features");
	assert_eq!(
		sections[0].entries,
		vec![
			"- add feature".to_string(),
			"- minor improvement".to_string()
		]
	);
}

#[test]
fn render_uncategorized_changes_fall_under_changed_heading() {
	let settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};

	let changes = vec![ReleaseNoteChange {
		change_type: None,
		bump: BumpSeverity::Patch,
		summary: "uncategorized fix".to_string(),
		..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
	}];

	let sections = render_release_note_sections("sdk", "1.0.0", &settings, &changes);

	// Changes without a matching type should fall under "Changed"
	assert_eq!(sections.len(), 1);
	assert_eq!(sections[0].title, "Changed");
	assert_eq!(sections[0].entries, vec!["- uncategorized fix".to_string()]);
}

#[test]
fn render_empty_changes_produces_prepare_release_placeholder() {
	let settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};

	let sections = render_release_note_sections("sdk", "1.0.0", &settings, &[]);

	assert_eq!(sections.len(), 1);
	assert_eq!(sections[0].title, "Changed");
	assert!(!sections[0].collapsed);
	assert_eq!(sections[0].entries, vec!["- prepare release".to_string()]);
}

#[test]
fn render_sections_collapse_and_ignore_by_priority_thresholds() {
	let mut settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	settings.section_thresholds.collapse = 50;
	settings.section_thresholds.ignored = 100;
	settings.sections.insert(
		"highlights".to_string(),
		ChangelogSectionDef {
			heading: "Highlights".to_string(),
			description: None,
			priority: 20,
		},
	);
	settings.sections.insert(
		"notes".to_string(),
		ChangelogSectionDef {
			heading: "Notes".to_string(),
			description: None,
			priority: 50,
		},
	);
	settings.sections.insert(
		"internal".to_string(),
		ChangelogSectionDef {
			heading: "Internal".to_string(),
			description: None,
			priority: 101,
		},
	);
	settings.types.insert(
		"feat".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "highlights".to_string(),
			description: None,
		},
	);
	settings.types.insert(
		"note".to_string(),
		ChangelogType {
			bump: BumpSeverity::None,
			section: "notes".to_string(),
			description: None,
		},
	);
	settings.types.insert(
		"internal".to_string(),
		ChangelogType {
			bump: BumpSeverity::None,
			section: "internal".to_string(),
			description: None,
		},
	);

	let sections = render_release_note_sections(
		"sdk",
		"1.0.0",
		&settings,
		&[
			ReleaseNoteChange {
				change_type: Some("feat".to_string()),
				summary: "ship highlights".to_string(),
				..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
			},
			ReleaseNoteChange {
				change_type: Some("note".to_string()),
				summary: "background note".to_string(),
				..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
			},
			ReleaseNoteChange {
				change_type: Some("internal".to_string()),
				summary: "internal cleanup".to_string(),
				..sample_change("pkg-c", "pkg-c", ".changeset/c.md")
			},
		],
	);

	assert_eq!(sections.len(), 2);
	assert_eq!(sections[0].title, "Highlights");
	assert!(!sections[0].collapsed);
	assert_eq!(sections[1].title, "Notes");
	assert!(sections[1].collapsed);
	assert!(!sections.iter().any(|section| section.title == "Internal"));
}

#[test]
fn render_release_notes_document_includes_section_headings_in_markdown() {
	let mut settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	settings.sections.insert(
		"features".to_string(),
		ChangelogSectionDef {
			heading: "Features".to_string(),
			description: None,
			priority: 10,
		},
	);
	settings.sections.insert(
		"fixes".to_string(),
		ChangelogSectionDef {
			heading: "Bug Fixes".to_string(),
			description: None,
			priority: 20,
		},
	);
	settings.types.insert(
		"feat".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
		},
	);
	settings.types.insert(
		"fix".to_string(),
		ChangelogType {
			bump: BumpSeverity::Patch,
			section: "fixes".to_string(),
			description: None,
		},
	);

	let changes = vec![
		ReleaseNoteChange {
			change_type: Some("feat".to_string()),
			bump: BumpSeverity::Minor,
			summary: "add feature".to_string(),
			..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
		},
		ReleaseNoteChange {
			change_type: Some("fix".to_string()),
			bump: BumpSeverity::Patch,
			summary: "fix bug".to_string(),
			..sample_change("pkg-b", "pkg-b", ".changeset/b.md")
		},
	];

	let document = build_release_notes_document(
		"sdk",
		"1.1.0",
		vec!["Grouped release for sdk".to_string()],
		&settings,
		&changes,
	);

	assert_eq!(document.title, "1.1.0");
	assert_eq!(
		document.summary,
		vec!["Grouped release for sdk".to_string()]
	);
	assert_eq!(document.sections.len(), 2);
	assert_eq!(document.sections[0].title, "Features");
	assert_eq!(document.sections[1].title, "Bug Fixes");

	// Now render to markdown and verify headings appear
	let markdown = render_release_notes(monochange_core::ChangelogFormat::Monochange, &document);
	assert!(
		markdown.contains("### Features"),
		"rendered markdown should include ### Features heading"
	);
	assert!(
		markdown.contains("### Bug Fixes"),
		"rendered markdown should include ### Bug Fixes heading"
	);
	assert!(
		markdown.contains("- add feature"),
		"rendered markdown should include feat entry"
	);
	assert!(
		markdown.contains("- fix bug"),
		"rendered markdown should include fix entry"
	);

	// Verify heading order in markdown matches priority
	let features_pos = markdown.find("### Features").expect("Features heading");
	let fixes_pos = markdown.find("### Bug Fixes").expect("Bug Fixes heading");
	assert!(
		features_pos < fixes_pos,
		"Features should appear before Bug Fixes"
	);
}

#[test]
fn keep_a_changelog_format_always_includes_section_headings() {
	let mut settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	settings.sections.insert(
		"features".to_string(),
		ChangelogSectionDef {
			heading: "Features".to_string(),
			description: None,
			priority: 10,
		},
	);
	settings.types.insert(
		"feat".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
		},
	);

	// Only one section - keep-a-changelog should still include heading
	let changes = vec![ReleaseNoteChange {
		change_type: Some("feat".to_string()),
		bump: BumpSeverity::Minor,
		summary: "add feature".to_string(),
		..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
	}];

	let document = build_release_notes_document("sdk", "1.0.0", Vec::new(), &settings, &changes);

	let markdown =
		render_release_notes(monochange_core::ChangelogFormat::KeepAChangelog, &document);

	// Keep-a-changelog always includes section headings, even for single section
	assert!(
		markdown.contains("### Features"),
		"keep-a-changelog should include heading even with single section"
	);
}

#[test]
fn monochange_format_includes_heading_for_single_changed_section() {
	let settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};

	// Single change with no matching type falls to "Changed"
	let changes = vec![ReleaseNoteChange {
		change_type: None,
		bump: BumpSeverity::Patch,
		summary: "fix bug".to_string(),
		..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
	}];

	let document = build_release_notes_document("sdk", "1.0.0", Vec::new(), &settings, &changes);

	let markdown = render_release_notes(monochange_core::ChangelogFormat::Monochange, &document);

	// Monochange format includes section headings even when the only section is
	// the default `Changed` bucket.
	assert!(
		markdown.contains("### Changed"),
		"monochange format should include ### Changed heading for single default section"
	);
	assert!(
		markdown.contains("- fix bug"),
		"entry should appear after heading"
	);
}

#[test]
fn monochange_format_includes_heading_for_custom_single_section() {
	let mut settings = ChangelogSettings {
		templates: vec!["- {{ summary }}".to_string()],
		..ChangelogSettings::default()
	};
	settings.sections.insert(
		"features".to_string(),
		ChangelogSectionDef {
			heading: "Features".to_string(),
			description: None,
			priority: 10,
		},
	);
	settings.types.insert(
		"feat".to_string(),
		ChangelogType {
			bump: BumpSeverity::Minor,
			section: "features".to_string(),
			description: None,
		},
	);

	let changes = vec![ReleaseNoteChange {
		change_type: Some("feat".to_string()),
		bump: BumpSeverity::Minor,
		summary: "add feature".to_string(),
		..sample_change("pkg-a", "pkg-a", ".changeset/a.md")
	}];

	let document = build_release_notes_document("sdk", "1.0.0", Vec::new(), &settings, &changes);

	let markdown = render_release_notes(monochange_core::ChangelogFormat::Monochange, &document);

	// Single custom section with non-"Changed" title should include heading
	assert!(
		markdown.contains("### Features"),
		"monochange format should include heading for custom single section"
	);
}

#[test]
fn root_relative_renders_workspace_root_as_dot() {
	let root = Path::new("/workspace");

	assert_eq!(root_relative(root, root), PathBuf::from("."));
}
