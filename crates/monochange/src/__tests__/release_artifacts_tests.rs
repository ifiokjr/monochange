#![allow(clippy::disallowed_methods)]
use std::fs;
use std::process::Command;
use std::time::Duration;

use monochange_core::ChangelogSettings;
use monochange_core::GroupChangelogInclude;
use monochange_core::PackageDefinition;
use monochange_core::PackageType;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseNotesSource;
use monochange_core::ProviderReleaseSettings;
use monochange_core::PublishMode;
use monochange_core::PublishRegistry;
use monochange_core::PublishSettings;
use monochange_core::PublishState;
use monochange_core::RegistryKind;
use monochange_core::ReleaseDecision;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestChangelog;
use monochange_core::ReleaseManifestCompatibilityEvidence;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestPlanDecision;
use monochange_core::ReleaseManifestPlanGroup;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseNotesSection;
use monochange_core::SourceChangeRequest;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::WorkspaceConfiguration;
use monochange_core::WorkspaceDefaults;
use semver::Version;
use tempfile::tempdir;

fn minimal_manifest_with_target(id: &str, version: &str) -> ReleaseManifest {
	ReleaseManifest {
		command: "prepare-release".to_string(),
		dry_run: false,
		version: Some(version.to_string()),
		group_version: None,
		release_targets: vec![ReleaseManifestTarget {
			id: id.to_string(),
			kind: ReleaseOwnerKind::Package,
			version: version.to_string(),
			tag: true,
			release: true,
			tag_name: format!("v{version}"),
			version_format: VersionFormat::Primary,
			members: vec![],
			rendered_title: format!("Release {id} {version}"),
			rendered_changelog_title: format!("{id} {version}"),
		}],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		package_publications: vec![],
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

fn source_configuration(provider: SourceProvider) -> SourceConfiguration {
	SourceConfiguration {
		provider,
		owner: "acme".to_string(),
		repo: "monochange".to_string(),
		host: Some("https://example.com".to_string()),
		api_url: None,
		releases: ProviderReleaseSettings {
			generate_notes: matches!(provider, SourceProvider::GitHub),
			source: ProviderReleaseNotesSource::Monochange,
			..ProviderReleaseSettings::default()
		},
		pull_requests: ProviderMergeRequestSettings::default(),
	}
}

fn sample_manifest() -> ReleaseManifest {
	ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: Some("1.2.3".to_string()),
		group_version: Some("2.0.0".to_string()),
		release_targets: vec![ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "2.0.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Namespaced,
			tag_name: "sdk/v2.0.0".to_string(),
			members: vec!["pkg-a".to_string(), "pkg-b".to_string()],
			rendered_title: "Release sdk v2.0.0".to_string(),
			rendered_changelog_title: "sdk v2.0.0".to_string(),
		}],
		released_packages: vec!["pkg-a".to_string(), "pkg-b".to_string()],
		changed_files: vec![
			PathBuf::from("Cargo.toml"),
			PathBuf::from("packages/pkg-a/package.json"),
		],
		changelogs: vec![ReleaseManifestChangelog {
			owner_id: "sdk".to_string(),
			owner_kind: ReleaseOwnerKind::Group,
			path: PathBuf::from("CHANGELOG.md"),
			format: ChangelogFormat::Monochange,
			notes: ReleaseNotesDocument {
				title: "2.0.0".to_string(),
				summary: vec!["Grouped release".to_string()],
				sections: vec![ReleaseNotesSection {
					title: "Features".to_string(),
					collapsed: false,
					entries: vec!["- Added batching".to_string()],
				}],
			},
			rendered: "## 2.0.0\n- Added batching".to_string(),
		}],
		changesets: Vec::new(),
		deleted_changesets: vec![PathBuf::from(".changeset/feature.md")],
		package_publications: Vec::new(),
		plan: ReleaseManifestPlan {
			workspace_root: PathBuf::from("."),
			decisions: vec![ReleaseManifestPlanDecision {
				package: "pkg-a".to_string(),
				bump: BumpSeverity::Minor,
				trigger: "changeset".to_string(),
				planned_version: Some("1.2.3".to_string()),
				reasons: vec!["feature".to_string()],
				upstream_sources: vec!["github".to_string()],
			}],
			groups: vec![ReleaseManifestPlanGroup {
				id: "sdk".to_string(),
				planned_version: Some("2.0.0".to_string()),
				members: vec!["pkg-a".to_string(), "pkg-b".to_string()],
				bump: BumpSeverity::Minor,
			}],
			warnings: vec!["warn".to_string()],
			unresolved_items: vec!["todo".to_string()],
			compatibility_evidence: vec![ReleaseManifestCompatibilityEvidence {
				package: "pkg-a".to_string(),
				provider: "rust-semver".to_string(),
				severity: BumpSeverity::Minor,
				summary: "minor api expansion".to_string(),
				confidence: "high".to_string(),
				evidence_location: Some("src/lib.rs".to_string()),
			}],
		},
	}
}

fn sample_package(root: &Path, config_id: &str, package_type: PackageType) -> PackageRecord {
	let manifest_path = root.join(format!("{config_id}/manifest"));
	fs::create_dir_all(
		manifest_path
			.parent()
			.unwrap_or_else(|| panic!("manifest path should have a parent")),
	)
	.unwrap_or_else(|error| panic!("create package dir: {error}"));
	fs::write(&manifest_path, "manifest\n")
		.unwrap_or_else(|error| panic!("write manifest: {error}"));
	let ecosystem = match package_type {
		PackageType::Cargo => Ecosystem::Cargo,
		PackageType::Npm => Ecosystem::Npm,
		PackageType::Deno => Ecosystem::Deno,
		PackageType::Dart => Ecosystem::Dart,
		PackageType::Flutter => Ecosystem::Flutter,
		_ => unreachable!("unsupported package type in sample_package"),
	};
	let mut package = PackageRecord::new(
		ecosystem,
		config_id,
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

#[tokio::test(flavor = "multi_thread")]
async fn release_target_and_title_helpers_cover_provider_and_skip_paths() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut configuration = empty_configuration(root);
	let source = source_configuration(SourceProvider::Gitea);
	configuration.source = Some(source.clone());
	configuration.packages = vec![PackageDefinition {
		id: "pkg-a".to_string(),
		path: PathBuf::from("pkg-a"),
		package_type: PackageType::Cargo,
		changelog: None,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: Some("Package {{ id }} {{ previous_version }} -> {{ version }}".to_string()),
		changelog_version_title: Some("{{ version }}".to_string()),
		versioned_files: Vec::new(),
		ignore_ecosystem_versioned_files: false,
		ignored_paths: Vec::new(),
		additional_paths: Vec::new(),
		tag: true,
		release: true,
		publish: PublishSettings::default(),
		version_format: VersionFormat::Namespaced,
	}];
	configuration.groups = vec![monochange_core::GroupDefinition {
		id: "sdk".to_string(),
		packages: vec!["pkg-a".to_string()],
		changelog: None,
		changelog_include: GroupChangelogInclude::All,
		excluded_changelog_types: Vec::new(),
		empty_update_message: None,
		release_title: Some("Group {{ id }} {{ compare_url }}".to_string()),
		changelog_version_title: None,
		versioned_files: Vec::new(),
		tag: true,
		release: true,
		version_format: VersionFormat::Namespaced,
	}];
	let package = sample_package(root, "pkg-a", PackageType::Cargo);
	let sorted_tags = vec![
		"sdk/v2.0.0".to_string(),
		"sdk/v1.5.0".to_string(),
		"pkg-a/v1.0.0".to_string(),
		"pkg-a/v0.9.0".to_string(),
	];
	assert_eq!(
		find_previous_tag_in("pkg-a/v1.0.0", &sorted_tags),
		Some("pkg-a/v0.9.0".to_string())
	);
	assert_eq!(
		parse_tag_prefix_and_version("pkg-a/v1.2.3"),
		Some(("pkg-a/v".to_string(), Version::new(1, 2, 3)))
	);
	assert_eq!(
		compare_url_for_provider(&source, "pkg-a/v0.9.0", "pkg-a/v1.0.0"),
		"https://example.com/acme/monochange/compare/pkg-a/v0.9.0...pkg-a/v1.0.0"
	);

	let plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![
			ReleaseDecision {
				package_id: "missing".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Patch,
				planned_version: Some(Version::new(1, 0, 1)),
				group_id: None,
				reasons: Vec::new(),
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			ReleaseDecision {
				package_id: package.id.clone(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Patch,
				planned_version: None,
				group_id: None,
				reasons: Vec::new(),
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			ReleaseDecision {
				package_id: package.id.clone(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(Version::new(1, 0, 0)),
				group_id: None,
				reasons: vec!["feature".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
		],
		groups: vec![monochange_core::PlannedVersionGroup {
			group_id: "sdk".to_string(),
			display_name: "SDK".to_string(),
			members: vec![package.id.clone()],
			mismatch_detected: false,
			planned_version: Some(Version::new(2, 0, 0)),
			recommended_bump: BumpSeverity::Minor,
		}],
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let packages = vec![package];
	let changeset_paths = vec![PathBuf::from(".changeset/feature.md")];
	let targets = build_release_targets(&configuration, &packages, &plan, &changeset_paths).await;
	assert_eq!(targets.len(), 2);
	assert!(
		targets
			.iter()
			.any(|target| target.id == "sdk" && !target.rendered_title.is_empty())
	);
	assert!(
		targets
			.iter()
			.all(|target| !target.rendered_changelog_title.is_empty())
	);

	let mut ungrouped_configuration = configuration.clone();
	ungrouped_configuration.groups.clear();
	let ungrouped_targets =
		build_release_targets(&ungrouped_configuration, &packages, &plan, &changeset_paths).await;
	assert!(ungrouped_targets.iter().any(|target| {
		target.id == "pkg-a"
			&& target.kind == ReleaseOwnerKind::Package
			&& target.members == ["pkg-a".to_string()]
	}));

	let mut missing_config_package = sample_package(root, "pkg-ghost", PackageType::Cargo);
	missing_config_package
		.metadata
		.insert("config_id".to_string(), "ghost".to_string());
	let mut missing_config_plan = plan.clone();
	missing_config_plan.groups.clear();
	missing_config_plan.decisions = vec![ReleaseDecision {
		package_id: missing_config_package.id.clone(),
		trigger_type: "changeset".to_string(),
		recommended_bump: BumpSeverity::Patch,
		planned_version: Some(Version::new(1, 0, 0)),
		group_id: None,
		reasons: Vec::new(),
		upstream_sources: Vec::new(),
		warnings: Vec::new(),
	}];
	let missing_config_packages = vec![missing_config_package];
	let missing_config_targets = build_release_targets(
		&configuration,
		&missing_config_packages,
		&missing_config_plan,
		&changeset_paths,
	)
	.await;
	assert!(missing_config_targets.is_empty());

	assert_eq!(
		effective_title_template(Some("specific"), Some("default"), "builtin"),
		"specific"
	);
	assert_eq!(
		effective_title_template(None, Some("default"), "builtin"),
		"default"
	);
	assert_eq!(
		default_release_title_for_format(VersionFormat::Primary),
		DEFAULT_RELEASE_TITLE_PRIMARY
	);
	assert_eq!(
		default_changelog_version_title_for_format(VersionFormat::Namespaced),
		DEFAULT_CHANGELOG_VERSION_TITLE_NAMESPACED
	);
	assert!(
		build_cargo_manifest_updates(
			&[],
			&ReleasePlan {
				workspace_root: root.to_path_buf(),
				decisions: Vec::new(),
				groups: Vec::new(),
				warnings: Vec::new(),
				unresolved_items: Vec::new(),
				compatibility_evidence: Vec::new(),
			}
		)
		.unwrap_or_else(|error| panic!("build empty cargo manifest updates: {error}"))
		.is_empty()
	);

	assert!(
		!resolve_release_datetime()
			.format("%Y-%m-%d")
			.to_string()
			.is_empty()
	);
}

#[test]
fn resolve_release_datetime_falls_back_for_invalid_environment_values() {
	temp_env::with_var("MONOCHANGE_RELEASE_DATE", Some("not-a-date"), || {
		assert!(
			!resolve_release_datetime()
				.format("%Y-%m-%d")
				.to_string()
				.is_empty()
		);
	});
}

#[test]
fn build_package_publication_targets_filters_disabled_and_preserves_publish_metadata() {
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tempdir.path();
	let mut configuration = empty_configuration(root);
	configuration.packages = vec![
		PackageDefinition {
			id: "core".to_string(),
			path: PathBuf::from("core"),
			package_type: PackageType::Cargo,
			changelog: None,
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
			publish: PublishSettings {
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				..PublishSettings::default()
			},
			version_format: VersionFormat::Primary,
		},
		PackageDefinition {
			id: "web".to_string(),
			path: PathBuf::from("web"),
			package_type: PackageType::Npm,
			changelog: None,
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
			publish: PublishSettings {
				mode: PublishMode::External,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				..PublishSettings::default()
			},
			version_format: VersionFormat::Primary,
		},
		PackageDefinition {
			id: "disabled".to_string(),
			path: PathBuf::from("disabled"),
			package_type: PackageType::Cargo,
			changelog: None,
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
			publish: PublishSettings {
				enabled: false,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				..PublishSettings::default()
			},
			version_format: VersionFormat::Primary,
		},
		PackageDefinition {
			id: "private".to_string(),
			path: PathBuf::from("private"),
			package_type: PackageType::Cargo,
			changelog: None,
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
			publish: PublishSettings {
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				..PublishSettings::default()
			},
			version_format: VersionFormat::Primary,
		},
	];

	let mut private_package = sample_package(root, "private", PackageType::Cargo);
	private_package.publish_state = PublishState::Private;
	let packages = vec![
		sample_package(root, "core", PackageType::Cargo),
		sample_package(root, "web", PackageType::Npm),
		sample_package(root, "disabled", PackageType::Cargo),
		private_package,
	];
	let plan = ReleasePlan {
		workspace_root: root.to_path_buf(),
		decisions: vec![
			ReleaseDecision {
				package_id: "cargo:core/manifest".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Minor,
				planned_version: Some(Version::new(1, 2, 0)),
				group_id: None,
				reasons: vec!["feature".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			ReleaseDecision {
				package_id: "npm:web/manifest".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Patch,
				planned_version: Some(Version::new(2, 0, 1)),
				group_id: None,
				reasons: vec!["fix".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			ReleaseDecision {
				package_id: "cargo:disabled/manifest".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Patch,
				planned_version: Some(Version::new(1, 0, 1)),
				group_id: None,
				reasons: vec!["fix".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			ReleaseDecision {
				package_id: "cargo:private/manifest".to_string(),
				trigger_type: "changeset".to_string(),
				recommended_bump: BumpSeverity::Patch,
				planned_version: Some(Version::new(1, 0, 1)),
				group_id: None,
				reasons: vec!["fix".to_string()],
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
			ReleaseDecision {
				package_id: "cargo:core/manifest".to_string(),
				trigger_type: "metadata".to_string(),
				recommended_bump: BumpSeverity::None,
				planned_version: Some(Version::new(9, 9, 9)),
				group_id: None,
				reasons: Vec::new(),
				upstream_sources: Vec::new(),
				warnings: Vec::new(),
			},
		],
		groups: Vec::new(),
		warnings: Vec::new(),
		unresolved_items: Vec::new(),
		compatibility_evidence: Vec::new(),
	};

	let targets = build_package_publication_targets(&configuration, &packages, &plan);
	assert_eq!(
		targets,
		vec![
			PackagePublicationTarget {
				package: "core".to_string(),
				ecosystem: Ecosystem::Cargo,
				registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
				version: "1.2.0".to_string(),
				mode: PublishMode::Builtin,
				trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
				attestations: monochange_core::PublishAttestationSettings::default(),
			},
			PackagePublicationTarget {
				package: "web".to_string(),
				ecosystem: Ecosystem::Npm,
				registry: Some(PublishRegistry::Builtin(RegistryKind::Npm)),
				version: "2.0.1".to_string(),
				mode: PublishMode::External,
				trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
				attestations: monochange_core::PublishAttestationSettings::default(),
			},
		]
	);
}

#[test]
fn build_release_manifest_copies_package_publications_from_prepared_release() {
	let cli_command = CliCommandDefinition {
		name: "release".to_string(),
		help_text: None,
		inputs: Vec::new(),
		steps: Vec::new(),
		dry_run: false,
	};
	let prepared_release = PreparedRelease {
		plan: ReleasePlan {
			workspace_root: PathBuf::from("."),
			decisions: Vec::new(),
			groups: Vec::new(),
			warnings: Vec::new(),
			unresolved_items: Vec::new(),
			compatibility_evidence: Vec::new(),
		},
		changeset_paths: Vec::new(),
		changesets: Vec::new(),
		released_packages: vec!["core".to_string()],
		version: Some("1.2.3".to_string()),
		group_version: None,
		release_targets: Vec::new(),
		changed_files: Vec::new(),
		changelogs: Vec::new(),
		updated_changelogs: Vec::new(),
		deleted_changesets: Vec::new(),
		package_publications: vec![PackagePublicationTarget {
			package: "core".to_string(),
			ecosystem: Ecosystem::Cargo,
			registry: Some(PublishRegistry::Builtin(RegistryKind::CratesIo)),
			version: "1.2.3".to_string(),
			mode: PublishMode::Builtin,
			trusted_publishing: monochange_core::TrustedPublishingSettings::default(),
			attestations: monochange_core::PublishAttestationSettings::default(),
		}],
		dry_run: false,
	};

	let manifest = build_release_manifest(&cli_command, &prepared_release, &[]);
	assert_eq!(
		manifest.package_publications,
		prepared_release.package_publications
	);
}

#[test]
fn render_release_cli_command_json_includes_publish_rate_limits_when_present() {
	let manifest = sample_manifest();
	let file_diffs = vec![PreparedFileDiff {
		path: PathBuf::from("Cargo.toml"),
		diff: "-old\n+new".to_string(),
		display_diff: "--- a/Cargo.toml\n+++ b/Cargo.toml\n-old\n+new".to_string(),
	}];
	let json = render_release_cli_command_json(
		&manifest,
		&ReleaseCliJsonSections {
			releases: &[],
			release_request: None,
			issue_comments: &[],
			release_commit: None,
			package_publish: None,
			publish_rate_limits: Some(&monochange_core::PublishRateLimitReport {
				dry_run: true,
				windows: vec![monochange_core::RegistryRateLimitWindowPlan {
					registry: RegistryKind::Npm,
					operation: monochange_core::RateLimitOperation::Publish,
					limit: None,
					window_seconds: None,
					pending: 1,
					batches_required: 1,
					fits_single_window: true,
					confidence: monochange_core::RateLimitConfidence::Low,
					notes: "npm soft limit".to_string(),
					evidence: Vec::new(),
				}],
				batches: vec![monochange_core::PublishRateLimitBatch {
					registry: RegistryKind::Npm,
					operation: monochange_core::RateLimitOperation::Publish,
					batch_index: 1,
					total_batches: 1,
					packages: vec!["pkg".to_string()],
					recommended_wait_seconds: None,
				}],
				warnings: Vec::new(),
			}),
			file_diffs: &file_diffs,
		},
	)
	.unwrap_or_else(|error| panic!("release cli json: {error}"));
	assert!(json.contains("publishRateLimits"));
}

#[tokio::test(flavor = "multi_thread")]
async fn release_manifest_and_source_helpers_cover_provider_specific_paths() {
	let manifest = sample_manifest();
	let source = source_configuration(SourceProvider::GitLab);
	let record = build_release_record(Some(&source), &manifest);
	assert_eq!(record.kind, monochange_core::RELEASE_RECORD_KIND);
	assert!(record.created_at.ends_with('Z'));
	assert_eq!(
		record
			.provider
			.as_ref()
			.map(|provider| provider.repo.as_str()),
		Some("monochange")
	);
	assert_eq!(
		record.updated_changelogs,
		vec![PathBuf::from("CHANGELOG.md")]
	);
	assert_eq!(record.release_targets[0].tag_name, "sdk/v2.0.0");

	let release_request = build_source_release_requests(&source, &manifest);
	assert_eq!(release_request.len(), 1);
	assert_eq!(release_request[0].provider, SourceProvider::GitLab);

	let change_request = build_source_change_request(&source, &manifest);
	assert_eq!(change_request.provider, SourceProvider::GitLab);
	assert!(
		change_request
			.commit_message
			.body
			.as_deref()
			.is_some_and(|body| body.contains("Prepare release."))
	);

	let gitea = source_configuration(SourceProvider::Gitea);
	let gitea_change_request = build_source_change_request(&gitea, &manifest);
	assert_eq!(gitea_change_request.provider, SourceProvider::Gitea);
	assert!(tag_url_for_provider(&gitea, "sdk/v2.0.0").contains("/releases/tag/"));

	let github = source_configuration(SourceProvider::GitHub);
	match publish_source_release_requests(&github, &[]).await {
		Ok(outcomes) => assert!(outcomes.is_empty()),
		Err(error) => assert!(!error.to_string().is_empty()),
	}

	match publish_source_release_requests(&source, &[]).await {
		Ok(outcomes) => assert!(outcomes.is_empty()),
		Err(error) => assert!(!error.to_string().is_empty()),
	}

	match publish_source_release_requests(&gitea, &[]).await {
		Ok(outcomes) => assert!(outcomes.is_empty()),
		Err(error) => assert!(!error.to_string().is_empty()),
	}

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	match publish_source_change_request(
		&source,
		tempdir.path(),
		&build_source_change_request(&source, &manifest),
		&manifest.changed_files,
		false,
		false,
	)
	.await
	{
		Ok(outcome) => assert_eq!(outcome.provider, SourceProvider::GitLab),
		Err(error) => assert!(!error.to_string().is_empty()),
	}

	match publish_source_change_request(
		&github,
		tempdir.path(),
		&build_source_change_request(&github, &manifest),
		&manifest.changed_files,
		false,
		false,
	)
	.await
	{
		Ok(outcome) => assert_eq!(outcome.provider, SourceProvider::GitHub),
		Err(error) => assert!(!error.to_string().is_empty()),
	}

	let publish_error = publish_source_change_request(
		&gitea,
		tempdir.path(),
		&SourceChangeRequest {
			provider: SourceProvider::Gitea,
			repository: "acme/monochange".to_string(),
			owner: "acme".to_string(),
			repo: "monochange".to_string(),
			base_branch: "main".to_string(),
			head_branch: "release/v2.0.0".to_string(),
			title: "chore: prepare release".to_string(),
			body: "release body".to_string(),
			labels: vec!["release".to_string()],
			auto_merge: false,
			commit_message: build_release_commit_message(Some(&gitea), &manifest),
		},
		&manifest.changed_files,
		false,
		false,
	)
	.await
	.err()
	.unwrap_or_else(|| {
		panic!("expected publishing a gitea change request outside a git repo to fail")
	});
	assert!(
		publish_error.to_string().contains("git") || publish_error.to_string().contains("failed")
	);
}

#[test]
fn release_paths_from_manifest_computes_hash_relative_and_absolute() {
	let root = PathBuf::from("/tmp/fake-root");
	let manifest = ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.0.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.0.0".to_string(),
			members: vec![],
			rendered_title: "1.0.0".to_string(),
			rendered_changelog_title: "[1.0.0]".to_string(),
		}],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		package_publications: vec![],
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
	let paths = ReleasePaths::from_manifest(&root, &manifest);
	assert!(!paths.hash.is_empty());
	assert_eq!(
		paths.relative,
		PathBuf::from(".monochange/releases")
			.join(&paths.hash)
			.join("release.json")
	);
	assert_eq!(paths.absolute, root.join(&paths.relative));
}

#[test]
fn release_paths_from_record_produces_same_hash_as_from_manifest() {
	let root = PathBuf::from("/tmp/fake-root");
	let manifest = ReleaseManifest {
		command: "release".to_string(),
		dry_run: false,
		version: None,
		group_version: None,
		release_targets: vec![ReleaseManifestTarget {
			id: "sdk".to_string(),
			kind: ReleaseOwnerKind::Group,
			version: "1.0.0".to_string(),
			tag: true,
			release: true,
			version_format: VersionFormat::Primary,
			tag_name: "v1.0.0".to_string(),
			members: vec![],
			rendered_title: "1.0.0".to_string(),
			rendered_changelog_title: "[1.0.0]".to_string(),
		}],
		released_packages: vec![],
		changed_files: vec![],
		changelogs: vec![],
		package_publications: vec![],
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
	let from_manifest = ReleasePaths::from_manifest(&root, &manifest);
	let record = build_release_record(None, &manifest);
	let from_record = ReleasePaths::from_record(&root, &record);
	assert_eq!(from_manifest.hash, from_record.hash);
	assert_eq!(from_manifest.relative, from_record.relative);
	assert_eq!(from_manifest.absolute, from_record.absolute);
}

#[test]
fn dedup_index_roundtrip() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let index = load_dedup_index(root);
	assert!(index.is_empty());

	let mut set = std::collections::HashSet::new();
	set.insert("abc123".to_string());
	set.insert("def456".to_string());
	save_dedup_index(root, &set).unwrap();
	let content = fs::read_to_string(root.join(DEDUP_INDEX_PATH)).unwrap();
	assert_eq!(content, "{\"hash\":\"abc123\"}\n{\"hash\":\"def456\"}");

	let loaded = load_dedup_index(root);
	assert_eq!(loaded.len(), 2);
	assert!(loaded.contains("abc123"));
	assert!(loaded.contains("def456"));
}

#[test]
fn add_and_remove_from_dedup_index() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	add_to_dedup_index(root, "hash_a").unwrap();
	add_to_dedup_index(root, "hash_b").unwrap();
	let index = load_dedup_index(root);
	assert_eq!(index.len(), 2);

	remove_from_dedup_index(root, "hash_a").unwrap();
	let index = load_dedup_index(root);
	assert_eq!(index.len(), 1);
	assert!(index.contains("hash_b"));
}

#[test]
fn deduplicate_uses_persistent_index_to_skip_scan() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let stale_dir = root.join(".monochange/releases/stale");
	fs::create_dir_all(&stale_dir).unwrap();
	let stale_record = r#"{
		"schemaVersion": 1,
		"kind": "monochange.releaseRecord",
		"createdAt": "2026-01-01T00:00:00Z",
		"command": "prepare-release",
		"version": "1.0.0",
		"releaseTargets": [
			{"id":"pkg-a","kind":"Package","version":"1.0.0","tag":true,"release":true,"tag_name":"v1.0.0","version_format":"primary","members":[],"rendered_title":"Release pkg-a 1.0.0","rendered_changelog_title":"pkg-a 1.0.0"}
		]
	}"#;
	fs::write(stale_dir.join("release.json"), stale_record).unwrap();

	let manifest = minimal_manifest_with_target("pkg-b", "2.0.0");
	let paths = ReleasePaths::from_manifest(root, &manifest);
	add_to_dedup_index(root, &paths.hash).unwrap();

	let target = ReleaseRecordTarget {
		id: "pkg-b".to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "2.0.0".to_string(),
		version_format: VersionFormat::Primary,
		tag: true,
		release: true,
		tag_name: "v2.0.0".to_string(),
		members: vec![],
	};
	let result = deduplicate_overlapping_release_records(
		root,
		&[target],
		root.join(".monochange/releases").as_path(),
	);
	assert!(result.is_ok());
	assert!(stale_dir.is_dir());
}

#[test]
fn deduplicate_skips_current_record_dir_during_overlap_scan() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();
	let current_record_dir = root.join(".monochange/releases/current");
	fs::create_dir_all(&current_record_dir).unwrap();

	let target = ReleaseRecordTarget {
		id: "pkg-current".to_string(),
		kind: ReleaseOwnerKind::Package,
		version: "1.2.3".to_string(),
		version_format: VersionFormat::Primary,
		tag: true,
		release: true,
		tag_name: "v1.2.3".to_string(),
		members: vec![],
	};

	let result = deduplicate_overlapping_release_records(root, &[target], &current_record_dir);

	assert!(result.is_ok());
	assert!(current_record_dir.is_dir());
}

#[test]
fn validate_release_record_file_skips_rebuild_when_targets_match() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();
	assert!(path.is_file());

	let first_content = fs::read_to_string(&path).unwrap();
	let validated = validate_release_record_file(root, None, &manifest, false).unwrap();
	assert_eq!(validated, path);

	let second_content = fs::read_to_string(&path).unwrap();
	assert_eq!(first_content, second_content);
}

#[test]
fn validate_release_record_file_rewrites_when_targets_differ() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();
	let first_content = fs::read_to_string(&path).unwrap();

	// Manually mutate the file so its targets no longer match.
	let mutated = first_content.replace("pkg-a", "pkg-b");
	fs::write(&path, &mutated).unwrap();

	// Validation should detect the mismatch and rewrite.
	let validated = validate_release_record_file(root, None, &manifest, true).unwrap();
	assert_eq!(validated, path);

	let second_content = fs::read_to_string(&path).unwrap();
	// The mutated content should have been overwritten back to the original target.
	assert!(!second_content.contains("pkg-b"));
	assert!(second_content.contains("pkg-a"));
}

#[test]
fn write_release_record_file_updates_persistent_index() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();
	assert!(path.is_file());

	let paths = ReleasePaths::from_manifest(root, &manifest);
	let index = load_dedup_index(root);
	assert!(index.contains(&paths.hash));
}

#[test]
fn load_dedup_index_skips_empty_and_invalid_lines() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();
	let index_path = root.join(".monochange/local/release-index.jsonl");
	fs::create_dir_all(index_path.parent().unwrap()).unwrap();
	fs::write(
		&index_path,
		"\n\n  \nnot-json\n{\"hash\":\"valid\"}\n\n{\"broken\n",
	)
	.unwrap();

	let index = load_dedup_index(root);
	assert_eq!(index.len(), 1);
	assert!(index.contains("valid"));
}

#[test]
fn load_dedup_index_from_reader_returns_none_on_read_error() {
	struct BrokenBufRead {
		line: &'static [u8],
		read_error: bool,
	}

	impl std::io::Read for BrokenBufRead {
		fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
			let available = self.fill_buf()?;
			if available.is_empty() {
				return Ok(0);
			}
			let length = available.len().min(buffer.len());
			buffer[..length].copy_from_slice(&available[..length]);
			self.consume(length);
			Ok(length)
		}
	}

	impl std::io::BufRead for BrokenBufRead {
		fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
			if self.read_error {
				return Err(std::io::Error::other("broken reader"));
			}
			Ok(self.line)
		}

		fn consume(&mut self, amount: usize) {
			if amount >= self.line.len() {
				self.line = b"";
				self.read_error = true;
				return;
			}
			self.line = &self.line[amount..];
		}
	}

	let index = load_dedup_index_from_reader(BrokenBufRead {
		line: b"{\"hash\":\"valid\"}\n",
		read_error: false,
	});
	assert!(index.is_none());
}

#[test]
fn atomic_write_writes_content_through_temp_file() {
	let tmp = tempdir().unwrap();
	let path = tmp.path().join("artifact.txt");

	atomic_write(&path, b"hello").unwrap();

	assert_eq!(fs::read(&path).unwrap(), b"hello");
}

#[test]
fn atomic_write_reports_temp_creation_errors() {
	let tmp = tempdir().unwrap();
	let path = tmp.path().join("missing/artifact.txt");

	let result = atomic_write(&path, b"hello");
	let error = result.unwrap_err().to_string();

	assert!(error.contains("failed to create temp file in"));
}

#[test]
fn write_temp_file_reports_write_errors() {
	struct BrokenWriter;

	impl std::io::Write for BrokenWriter {
		fn write(&mut self, _buffer: &[u8]) -> std::io::Result<usize> {
			Err(std::io::Error::other("broken writer"))
		}

		fn flush(&mut self) -> std::io::Result<()> {
			Ok(())
		}
	}

	let mut writer = BrokenWriter;
	let result = write_temp_file(&mut writer, std::path::Path::new("artifact.txt"), b"hello");
	let error = result.unwrap_err().to_string();

	assert!(error.contains("failed to write temp file for artifact.txt: broken writer"));
}

#[test]
fn persist_temp_file_reports_rename_errors() {
	let tmp = tempdir().unwrap();
	let named_temp = tempfile::NamedTempFile::new_in(tmp.path()).unwrap();
	let path = tmp.path().join("missing/artifact.txt");

	let result = persist_temp_file(named_temp, &path);
	let error = result.unwrap_err().to_string();

	assert!(error.contains("failed to rename temp file to"));
}

#[test]
#[cfg(unix)]
fn save_dedup_index_reports_io_errors() {
	use std::os::unix::fs::PermissionsExt;

	let tmp = tempdir().unwrap();
	let root = tmp.path();

	// Create a file at the exact path where the directory should be,
	// so create_dir_all fails.
	let local_path = root.join(".monochange/local");
	fs::create_dir_all(local_path.parent().unwrap()).unwrap();
	fs::write(&local_path, "block").unwrap();
	let result = save_dedup_index(root, &std::collections::HashSet::new());
	assert!(result.is_err());

	// Restore directory and make it unwritable.
	fs::remove_file(&local_path).unwrap();
	fs::create_dir_all(&local_path).unwrap();
	let mut permissions = fs::metadata(&local_path).unwrap().permissions();
	permissions.set_mode(0o000);
	fs::set_permissions(&local_path, permissions.clone()).unwrap();

	let result = save_dedup_index(root, &std::collections::HashSet::new());
	assert!(result.is_err());

	// Cleanup: restore permissions so tempdir can be deleted.
	permissions.set_mode(0o755);
	let _ = fs::set_permissions(&local_path, permissions);
}

#[test]
fn validate_release_record_file_fast_path_detects_missing_id() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();

	// Overwrite with a record whose target is missing the `id` field.
	let mutated = r#"{"schemaVersion":1,"kind":"monochange.releaseRecord","createdAt":"2026-01-01T00:00:00Z","command":"prepare-release","version":"1.0.0","releaseTargets":[{"kind":"npm","version":"1.0.0"}]}"#;
	fs::write(&path, mutated).unwrap();

	let validated = validate_release_record_file(root, None, &manifest, true).unwrap();
	assert_eq!(validated, path);

	// Should have been rewritten with the correct content.
	let content = fs::read_to_string(&path).unwrap();
	assert!(content.contains("pkg-a"));
}

#[test]
fn validate_release_record_file_fast_path_detects_missing_kind() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();

	// Overwrite with a record whose target is missing the `kind` field.
	let mutated = r#"{"schemaVersion":1,"kind":"monochange.releaseRecord","createdAt":"2026-01-01T00:00:00Z","command":"prepare-release","version":"1.0.0","releaseTargets":[{"id":"pkg-a","version":"1.0.0"}]}"#;
	fs::write(&path, mutated).unwrap();

	let validated = validate_release_record_file(root, None, &manifest, true).unwrap();
	assert_eq!(validated, path);

	let content = fs::read_to_string(&path).unwrap();
	assert!(content.contains("Package"));
}

#[test]
fn validate_release_record_file_fast_path_detects_missing_version() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();

	// Overwrite with a record whose target is missing the `version` field.
	let mutated = r#"{"schemaVersion":1,"kind":"monochange.releaseRecord","createdAt":"2026-01-01T00:00:00Z","command":"prepare-release","version":"1.0.0","releaseTargets":[{"id":"pkg-a","kind":"Package"}]}"#;
	fs::write(&path, mutated).unwrap();

	let validated = validate_release_record_file(root, None, &manifest, true).unwrap();
	assert_eq!(validated, path);

	let content = fs::read_to_string(&path).unwrap();
	assert!(content.contains("1.0.0"));
}

#[test]
fn validate_release_record_file_fast_path_detects_mismatched_target_count() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();

	// Overwrite with a record that has MORE targets than the manifest.
	let mutated = r#"{"schemaVersion":1,"kind":"monochange.releaseRecord","createdAt":"2026-01-01T00:00:00Z","command":"prepare-release","version":"1.0.0","releaseTargets":[{"id":"pkg-a","kind":"Package","version":"1.0.0"},{"id":"pkg-b","kind":"Package","version":"2.0.0"}]}"#;
	fs::write(&path, mutated).unwrap();

	let validated = validate_release_record_file(root, None, &manifest, true).unwrap();
	assert_eq!(validated, path);

	// Should have been rewritten because target counts don't match.
	let content = fs::read_to_string(&path).unwrap();
	assert!(!content.contains("pkg-b"));
}

#[test]
#[cfg(unix)]
fn validate_release_record_file_fast_path_reports_error_for_unreadable_file() {
	use std::os::unix::fs::PermissionsExt;

	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();

	// Make the file unreadable.
	let mut permissions = fs::metadata(&path).unwrap().permissions();
	permissions.set_mode(0o000);
	fs::set_permissions(&path, permissions.clone()).unwrap();

	let result = validate_release_record_file(root, None, &manifest, false);
	assert!(result.is_err());

	// Cleanup.
	permissions.set_mode(0o644);
	let _ = fs::set_permissions(&path, permissions);
}

fn run_git(root: &Path, args: &[&str]) {
	let output = Command::new("git")
		.args(args)
		.current_dir(root)
		.output()
		.unwrap_or_else(|error| panic!("run git {args:?}: {error}"));
	assert!(
		output.status.success(),
		"git {args:?} failed: stdout={} stderr={}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
}

fn initialize_git_repo(root: &Path) {
	run_git(root, &["init", "-b", "release-branch"]);
	run_git(root, &["config", "user.email", "monochange@example.com"]);
	run_git(root, &["config", "user.name", "monochange"]);
	fs::write(root.join("README.md"), "initial\n").unwrap();
	run_git(root, &["add", "README.md"]);
	run_git(
		root,
		&["-c", "commit.gpgsign=false", "commit", "-m", "initial"],
	);
}

#[test]
fn build_hosted_commit_request_uses_github_context_and_release_files() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();
	initialize_git_repo(root);
	fs::create_dir_all(root.join("packages/pkg-a")).unwrap();
	fs::write(
		root.join("packages/pkg-a/package.json"),
		"{\"version\":\"1.2.3\"}\n",
	)
	.unwrap();

	let prepared = PreparedReleaseCommit {
		message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: Some("Release notes".to_string()),
		},
		tracked_paths: vec![PathBuf::from("packages/pkg-a/package.json")],
	};

	let request = build_hosted_commit_request_for_github(
		root,
		&prepared,
		false,
		"monochange/example-repo",
		Some("release/pr-1"),
		None,
	)
	.unwrap();

	assert_eq!(request.provider, "github");
	assert_eq!(request.owner, "monochange");
	assert_eq!(request.repository, "example-repo");
	assert_eq!(request.branch, "release/pr-1");
	assert_eq!(request.subject, "chore(release): prepare release");
	assert_eq!(request.body, "Release notes");
	assert_eq!(request.files.len(), 1);
	assert_eq!(request.files[0].path, "packages/pkg-a/package.json");
	assert_eq!(
		request.files[0].content.as_deref(),
		Some("{\"version\":\"1.2.3\"}\n")
	);
	assert!(!request.base_commit.is_empty());
	assert!(!request.dry_run);
}

#[test]
fn build_hosted_commit_request_marks_deleted_release_files() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();
	initialize_git_repo(root);
	let prepared = PreparedReleaseCommit {
		message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		tracked_paths: vec![PathBuf::from(".changeset/removed.md")],
	};

	let request = build_hosted_commit_request_for_github(
		root,
		&prepared,
		true,
		"monochange/example-repo",
		None,
		Some("release/fallback"),
	)
	.unwrap();

	assert_eq!(request.branch, "release/fallback");
	assert_eq!(request.body, "");
	assert_eq!(request.files.len(), 1);
	assert_eq!(request.files[0].path, ".changeset/removed.md");
	assert_eq!(request.files[0].content, None);
	assert!(request.dry_run);
}

#[test]
fn build_hosted_commit_request_reports_missing_github_repository() {
	let tmp = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tmp.path();
	initialize_git_repo(root);
	let prepared = PreparedReleaseCommit {
		message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		tracked_paths: Vec::new(),
	};

	let _guard = crate::tests::TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	temp_env::with_var("GITHUB_REPOSITORY", None::<&str>, || {
		let error = match build_hosted_commit_request(root, &prepared, true) {
			Ok(_) => panic!("expected missing repository error"),
			Err(error) => error.to_string(),
		};
		assert!(error.contains("hosted CommitRelease requires GITHUB_REPOSITORY"));
	});
}

#[test]
fn build_hosted_commit_request_reports_bad_repository_and_file_read_errors() {
	let tmp = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tmp.path();
	initialize_git_repo(root);
	let prepared = PreparedReleaseCommit {
		message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		tracked_paths: Vec::new(),
	};
	let error = match build_hosted_commit_request_for_github(
		root,
		&prepared,
		true,
		"monochange",
		None,
		Some("release/pr"),
	) {
		Ok(_) => panic!("expected bad repository error"),
		Err(error) => error.to_string(),
	};
	assert!(error.contains("GITHUB_REPOSITORY must use `owner/repo` format"));

	fs::create_dir_all(root.join("release-dir"))
		.unwrap_or_else(|error| panic!("create release dir: {error}"));
	let prepared = PreparedReleaseCommit {
		message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		tracked_paths: vec![PathBuf::from("release-dir")],
	};
	let error = match build_hosted_commit_request_for_github(
		root,
		&prepared,
		true,
		"monochange/example-repo",
		None,
		Some("release/pr"),
	) {
		Ok(_) => panic!("expected file read error"),
		Err(error) => error.to_string(),
	};
	assert!(error.contains("read hosted commit file `release-dir`"));
}

#[test]
fn git_current_branch_reports_process_and_status_errors() {
	let missing_root = PathBuf::from("/definitely/missing/monochange/git/root");
	let error = match git_current_branch(&missing_root) {
		Ok(branch) => panic!("expected missing root error, got {branch}"),
		Err(error) => error.to_string(),
	};
	assert!(error.contains("failed to resolve current branch"));

	let tmp = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let error = match git_current_branch(tmp.path()) {
		Ok(branch) => panic!("expected non-git root error, got {branch}"),
		Err(error) => error.to_string(),
	};
	assert_eq!(error, "config error: failed to resolve current branch");
}

#[test]
fn build_hosted_commit_request_falls_back_to_current_branch() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();
	initialize_git_repo(root);
	let prepared = PreparedReleaseCommit {
		message: CommitMessage {
			subject: "chore(release): prepare release".to_string(),
			body: None,
		},
		tracked_paths: Vec::new(),
	};

	let request = build_hosted_commit_request_for_github(
		root,
		&prepared,
		true,
		"monochange/example-repo",
		Some(""),
		Some(""),
	)
	.unwrap();

	assert_eq!(request.branch, "release-branch");
}

fn hosted_commit_request_fixture() -> HostedCommitRequest {
	HostedCommitRequest {
		provider: "github",
		owner: "monochange".to_string(),
		repository: "monochange".to_string(),
		branch: "feat/release".to_string(),
		base_commit: "0123456789abcdef0123456789abcdef01234567".to_string(),
		subject: "chore(release): prepare release".to_string(),
		body: "Release notes".to_string(),
		files: vec![HostedCommitFile {
			path: "CHANGELOG.md".to_string(),
			content: Some("# Changelog".to_string()),
		}],
		dry_run: false,
	}
}

fn with_env_var<T>(name: &str, value: Option<&str>, run: impl FnOnce() -> T) -> T {
	let _guard = crate::tests::TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock: {error}"));
	temp_env::with_var(name, value, run)
}

#[test]
fn hosted_commit_bearer_token_uses_token_auth_and_reports_missing_token() {
	with_env_var("MONOCHANGE_TOKEN", Some("monochange-secret"), || {
		let options = HostedCommitOptions {
			auth: monochange_core::HostedCommitAuth::Token,
			url: None,
			oidc_audience: None,
		};
		assert_eq!(
			hosted_commit_bearer_token(&options).unwrap_or_else(|error| panic!("token: {error}")),
			"monochange-secret",
		);
	});

	with_env_var("MONOCHANGE_TOKEN", None, || {
		let options = HostedCommitOptions {
			auth: monochange_core::HostedCommitAuth::Token,
			url: None,
			oidc_audience: None,
		};
		let error = match hosted_commit_bearer_token(&options) {
			Ok(token) => panic!("missing token should fail, got {token}"),
			Err(error) => error.to_string(),
		};
		assert!(error.contains("requires MONOCHANGE_TOKEN"));
	});
}

#[test]
fn send_hosted_commit_request_posts_json_and_parses_response() {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("local address: {error}"));
	let server = std::thread::spawn(move || {
		let (mut stream, _) = listener
			.accept()
			.unwrap_or_else(|error| panic!("accept request: {error}"));
		stream
			.set_read_timeout(Some(Duration::from_millis(500)))
			.unwrap_or_else(|error| panic!("set read timeout: {error}"));
		let mut buffer = [0_u8; 8192];
		use std::io::Read as _;
		use std::io::Write as _;
		let bytes_read = stream
			.read(&mut buffer)
			.unwrap_or_else(|error| panic!("read request: {error}"));
		let request = String::from_utf8_lossy(&buffer[..bytes_read]);
		assert!(request.contains("POST /api/release-commits HTTP/1.1"));
		assert!(request.contains("authorization: Bearer monochange-secret"));
		assert!(request.contains("chore(release): prepare release"));
		let body = "{\"commit\":\"abc123\",\"status\":\"completed\"}";
		let response = format!(
			"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
			body.len(),
		);
		stream
			.write_all(response.as_bytes())
			.unwrap_or_else(|error| panic!("write response: {error}"));
	});

	with_env_var("MONOCHANGE_TOKEN", Some("monochange-secret"), || {
		let options = HostedCommitOptions {
			auth: monochange_core::HostedCommitAuth::Token,
			url: Some(format!("http://{address}/")),
			oidc_audience: None,
		};
		let response = send_hosted_commit_request(&hosted_commit_request_fixture(), &options)
			.unwrap_or_else(|error| panic!("hosted response: {error}"));
		assert_eq!(response.commit.as_deref(), Some("abc123"));
		assert_eq!(response.status.as_deref(), Some("completed"));
	});

	server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));
}

fn single_request_server(response_body: &'static str) -> (String, std::thread::JoinHandle<String>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("local address: {error}"));
	let server = std::thread::spawn(move || {
		let (mut stream, _) = listener
			.accept()
			.unwrap_or_else(|error| panic!("accept request: {error}"));
		stream
			.set_read_timeout(Some(Duration::from_millis(500)))
			.unwrap_or_else(|error| panic!("set read timeout: {error}"));
		let mut buffer = [0_u8; 8192];
		use std::io::Read as _;
		use std::io::Write as _;
		let bytes_read = stream
			.read(&mut buffer)
			.unwrap_or_else(|error| panic!("read request: {error}"));
		let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
		let response = format!(
			"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
			response_body.len(),
		);
		stream
			.write_all(response.as_bytes())
			.unwrap_or_else(|error| panic!("write response: {error}"));
		request
	});
	(format!("http://{address}"), server)
}

fn single_status_request_server(
	status: &'static str,
	response_body: &'static str,
) -> (String, std::thread::JoinHandle<String>) {
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind mock server: {error}"));
	let address = listener
		.local_addr()
		.unwrap_or_else(|error| panic!("local address: {error}"));
	let server = std::thread::spawn(move || {
		let (mut stream, _) = listener
			.accept()
			.unwrap_or_else(|error| panic!("accept request: {error}"));
		stream
			.set_read_timeout(Some(Duration::from_millis(500)))
			.unwrap_or_else(|error| panic!("set read timeout: {error}"));
		let mut buffer = [0_u8; 8192];
		use std::io::Read as _;
		use std::io::Write as _;
		let bytes_read = stream
			.read(&mut buffer)
			.unwrap_or_else(|error| panic!("read request: {error}"));
		let request = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
		let response = format!(
			"HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response_body}",
			response_body.len(),
		);
		stream
			.write_all(response.as_bytes())
			.unwrap_or_else(|error| panic!("write response: {error}"));
		request
	});
	(format!("http://{address}"), server)
}

#[test]
fn send_hosted_commit_request_reports_request_http_and_json_errors() {
	let request = hosted_commit_request_fixture();
	let listener = std::net::TcpListener::bind("127.0.0.1:0")
		.unwrap_or_else(|error| panic!("bind dropped server: {error}"));
	let dropped_url = format!(
		"http://{}",
		listener
			.local_addr()
			.unwrap_or_else(|error| panic!("dropped server address: {error}"))
	);
	drop(listener);

	with_env_var("MONOCHANGE_TOKEN", Some("monochange-secret"), || {
		let options = HostedCommitOptions {
			auth: monochange_core::HostedCommitAuth::Token,
			url: Some(dropped_url),
			oidc_audience: None,
		};
		let error = match send_hosted_commit_request(&request, &options) {
			Ok(_) => panic!("expected request failure"),
			Err(error) => error.to_string(),
		};
		assert!(error.contains("hosted CommitRelease request failed"));
	});

	let (url, server) = single_status_request_server("500 Internal Server Error", "server down");
	with_env_var("MONOCHANGE_TOKEN", Some("monochange-secret"), || {
		let options = HostedCommitOptions {
			auth: monochange_core::HostedCommitAuth::Token,
			url: Some(url),
			oidc_audience: None,
		};
		let error = match send_hosted_commit_request(&request, &options) {
			Ok(_) => panic!("expected HTTP failure"),
			Err(error) => error.to_string(),
		};
		assert!(error.contains("hosted CommitRelease failed with HTTP 500"));
	});
	server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));

	let (url, server) = single_request_server("not-json");
	with_env_var("MONOCHANGE_TOKEN", Some("monochange-secret"), || {
		let options = HostedCommitOptions {
			auth: monochange_core::HostedCommitAuth::Token,
			url: Some(url),
			oidc_audience: None,
		};
		let error = match send_hosted_commit_request(&request, &options) {
			Ok(_) => panic!("expected invalid JSON failure"),
			Err(error) => error.to_string(),
		};
		assert!(error.contains("hosted CommitRelease response was invalid JSON"));
	});
	server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));
}

#[test]
fn github_actions_oidc_token_reports_missing_token_and_http_error() {
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Oidc,
		url: None,
		oidc_audience: Some("monochange.dev".to_string()),
	};
	let _guard = crate::tests::TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	temp_env::with_vars(
		[
			(
				"ACTIONS_ID_TOKEN_REQUEST_URL",
				Some("http://127.0.0.1/oidc"),
			),
			("ACTIONS_ID_TOKEN_REQUEST_TOKEN", None::<&str>),
		],
		|| {
			let error = match github_actions_oidc_token(&options) {
				Ok(token) => panic!("missing request token should fail, got {token}"),
				Err(error) => error.to_string(),
			};
			assert!(error.contains("requires ACTIONS_ID_TOKEN_REQUEST_TOKEN"));
		},
	);

	let (url, server) = single_status_request_server("403 Forbidden", "forbidden");
	temp_env::with_vars(
		[
			("ACTIONS_ID_TOKEN_REQUEST_URL", Some(url.as_str())),
			("ACTIONS_ID_TOKEN_REQUEST_TOKEN", Some("github-token")),
		],
		|| {
			let error = match github_actions_oidc_token(&options) {
				Ok(token) => panic!("OIDC HTTP error should fail, got {token}"),
				Err(error) => error.to_string(),
			};
			assert!(error.contains("GitHub Actions OIDC request failed with HTTP 403"));
		},
	);
	server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));
}

#[test]
fn hosted_commit_release_returns_dry_run_report_without_posting() {
	let tmp = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tmp.path();
	initialize_git_repo(root);
	let manifest = minimal_manifest_with_target("pkg-a", "1.2.3");
	let context = CliContext {
		root: root.to_path_buf(),
		dry_run: true,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Token,
		url: Some("http://127.0.0.1:9".to_string()),
		oidc_audience: None,
	};

	temp_env::with_var("GITHUB_REPOSITORY", Some("monochange/monochange"), || {
		let report = hosted_commit_release(root, &context, None, &manifest, true, &options)
			.unwrap_or_else(|error| panic!("hosted dry run: {error}"));
		assert_eq!(report.status, "dry_run");
		assert!(report.commit.is_none());
		assert!(report.dry_run);
		assert!(report.subject.contains("chore(release): prepare release"));
	});
}

#[test]
fn hosted_commit_bearer_token_auto_prefers_oidc_when_actions_env_exists() {
	let (url, server) = single_request_server("{\"value\":\"oidc-token\"}");
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Auto,
		url: None,
		oidc_audience: Some("custom-audience".to_string()),
	};
	temp_env::with_vars(
		[
			("ACTIONS_ID_TOKEN_REQUEST_URL", Some(url.as_str())),
			(
				"ACTIONS_ID_TOKEN_REQUEST_TOKEN",
				Some("actions-request-token"),
			),
			("MONOCHANGE_TOKEN", Some("fallback-token")),
		],
		|| {
			let token = hosted_commit_bearer_token(&options)
				.unwrap_or_else(|error| panic!("oidc token: {error}"));
			assert_eq!(token, "oidc-token");
		},
	);
	let request = server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));
	assert!(request.contains("GET /?audience=custom-audience HTTP/1.1"));
	assert!(request.contains("authorization: Bearer actions-request-token"));
}

#[test]
fn hosted_commit_bearer_token_oidc_auth_uses_github_actions_token() {
	let (url, server) = single_request_server("{\"value\":\"oidc-token\"}");
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Oidc,
		url: None,
		oidc_audience: Some("monochange.dev".to_string()),
	};
	let _guard = crate::tests::TEST_ENV_LOCK
		.lock()
		.unwrap_or_else(|error| panic!("test env lock poisoned: {error}"));
	temp_env::with_vars(
		[
			("ACTIONS_ID_TOKEN_REQUEST_URL", Some(url.as_str())),
			("ACTIONS_ID_TOKEN_REQUEST_TOKEN", Some("github-token")),
		],
		|| {
			let token = hosted_commit_bearer_token(&options)
				.unwrap_or_else(|error| panic!("OIDC token: {error}"));
			assert_eq!(token, "oidc-token");
		},
	);
	server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));
}

#[test]
fn hosted_commit_bearer_token_auto_falls_back_to_monochange_token() {
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Auto,
		url: None,
		oidc_audience: None,
	};
	temp_env::with_vars(
		[
			("ACTIONS_ID_TOKEN_REQUEST_URL", None::<&str>),
			("ACTIONS_ID_TOKEN_REQUEST_TOKEN", None::<&str>),
			("MONOCHANGE_TOKEN", Some("fallback-token")),
		],
		|| {
			let token = hosted_commit_bearer_token(&options)
				.unwrap_or_else(|error| panic!("fallback token: {error}"));
			assert_eq!(token, "fallback-token");
		},
	);
}

#[test]
fn github_actions_oidc_token_reports_http_and_json_errors() {
	let (error_url, error_server) = single_request_server("not-json");
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Oidc,
		url: None,
		oidc_audience: None,
	};
	temp_env::with_vars(
		[
			("ACTIONS_ID_TOKEN_REQUEST_URL", Some(error_url.as_str())),
			(
				"ACTIONS_ID_TOKEN_REQUEST_TOKEN",
				Some("actions-request-token"),
			),
		],
		|| {
			let error = match github_actions_oidc_token(&options) {
				Ok(token) => panic!("expected invalid json error, got token {token}"),
				Err(error) => error.to_string(),
			};
			assert!(error.contains("response was invalid JSON"));
		},
	);
	error_server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));

	temp_env::with_vars(
		[
			("ACTIONS_ID_TOKEN_REQUEST_URL", None::<&str>),
			(
				"ACTIONS_ID_TOKEN_REQUEST_TOKEN",
				Some("actions-request-token"),
			),
		],
		|| {
			let error = match github_actions_oidc_token(&options) {
				Ok(token) => panic!("missing OIDC URL should fail, got {token}"),
				Err(error) => error.to_string(),
			};
			assert!(error.contains("requires ACTIONS_ID_TOKEN_REQUEST_URL"));
		},
	);
}

#[test]
fn hosted_commit_release_posts_in_non_dry_run_and_defaults_status() {
	let tmp = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	let root = tmp.path();
	initialize_git_repo(root);
	let manifest = minimal_manifest_with_target("pkg-a", "1.2.3");
	let context = CliContext {
		root: root.to_path_buf(),
		dry_run: false,
		quiet: false,
		show_diff: false,
		inputs: BTreeMap::new(),
		last_step_inputs: BTreeMap::new(),
		prepared_release: None,
		prepared_file_diffs: Vec::new(),
		release_manifest_path: None,
		release_requests: Vec::new(),
		release_results: Vec::new(),
		release_request: None,
		release_request_result: None,
		release_commit_report: None,
		package_publish_report: None,
		rate_limit_report: None,
		issue_comment_plans: Vec::new(),
		issue_comment_results: Vec::new(),
		changeset_policy_evaluation: None,
		changeset_diagnostics: None,
		retarget_report: None,
		step_outputs: BTreeMap::new(),
		command_logs: Vec::new(),
	};
	let (url, server) = single_request_server("{\"commit\":\"abc123\"}");
	let options = HostedCommitOptions {
		auth: monochange_core::HostedCommitAuth::Token,
		url: Some(url),
		oidc_audience: None,
	};

	temp_env::with_vars(
		[
			("GITHUB_REPOSITORY", Some("monochange/monochange")),
			("MONOCHANGE_TOKEN", Some("monochange-secret")),
		],
		|| {
			let report = hosted_commit_release(root, &context, None, &manifest, true, &options)
				.unwrap_or_else(|error| panic!("hosted commit release: {error}"));
			assert_eq!(report.commit.as_deref(), Some("abc123"));
			assert_eq!(report.status, "completed");
			assert!(!report.dry_run);
		},
	);

	let request = server
		.join()
		.unwrap_or_else(|error| panic!("mock server join: {error:?}"));
	assert!(request.contains("POST /api/release-commits HTTP/1.1"));
	assert!(request.contains("authorization: Bearer monochange-secret"));
}
