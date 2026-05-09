use std::fs;

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

#[test]
fn release_target_and_title_helpers_cover_provider_and_skip_paths() {
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

	let targets = build_release_targets(
		&configuration,
		&[package],
		&plan,
		&[PathBuf::from(".changeset/feature.md")],
	);
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

#[test]
fn release_manifest_and_source_helpers_cover_provider_specific_paths() {
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

	let dry_requests_error = publish_source_release_requests(&gitea, &[])
		.err()
		.unwrap_or_else(|| {
			panic!("expected publishing gitea release requests without auth to fail")
		});
	assert!(dry_requests_error.to_string().contains("GITEA_TOKEN"));

	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
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
	)
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
fn validate_release_record_file_skips_rebuild_when_targets_match() {
	let tmp = tempdir().unwrap();
	let root = tmp.path();

	let manifest = minimal_manifest_with_target("pkg-a", "1.0.0");
	let path = write_release_record_file(root, None, &manifest).unwrap();
	assert!(path.is_file());

	let first_content = fs::read_to_string(&path).unwrap();
	let validated = validate_release_record_file(root, None, &manifest).unwrap();
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
	let validated = validate_release_record_file(root, None, &manifest).unwrap();
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
