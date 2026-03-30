use std::path::PathBuf;

use monochange_core::GitHubConfiguration;
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

use crate::build_release_requests;

#[test]
fn build_release_requests_uses_matching_monochange_changelog_bodies() {
	let github = GitHubConfiguration {
		owner: "ifiokjr".to_string(),
		repo: "monochange".to_string(),
		releases: GitHubReleaseSettings::default(),
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
		Some("## 1.2.0\n\nGrouped release for `sdk`.\n\n- add github publishing")
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
					title: "Changed".to_string(),
					entries: vec!["add github publishing".to_string()],
				}],
			},
			rendered: "## 1.2.0\n\nGrouped release for `sdk`.\n\n- add github publishing"
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
