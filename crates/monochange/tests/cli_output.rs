use std::fs;
use std::path::Path;
use std::process::Command;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;
use insta_cmd::get_cargo_bin;
use tempfile::tempdir;

fn cli() -> Command {
	let mut command = Command::new(get_cargo_bin("mc"));
	command.env("NO_COLOR", "1");
	command
}

macro_rules! apply_common_filters {
	() => {
		let _filters = {
			let mut settings = insta::Settings::clone_current();
			settings.add_filter(r"/var/folders/[^\s]+?/T/[^/\s]+", "[ROOT]");
			settings.add_filter(r"/tmp/[^/\s]+", "[ROOT]");
			settings.add_filter(r"/home/runner/work/_temp/[^/\s]+", "[ROOT]");
			settings.add_filter(r"\b[A-Z]:\\[^\s]+?\\Temp\\[^\\\s]+", "[ROOT]");
			settings.add_filter(r"SourceOffset\(\d+\)", "SourceOffset([OFFSET])");
			settings.add_filter(r"length: \d+", "length: [LEN]");
			settings.add_filter(r"@ bytes \d+\.\.\d+", "@ bytes [OFFSET]..[END]");
			settings.bind_to_scope()
		};
	};
}

#[test]
fn validate_cli_succeeds_for_valid_workspace() {
	apply_common_filters!();
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../fixtures/cargo/workspace-versioned-grouped-release");
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_root, tempdir.path());

	assert_cmd_snapshot!(
		cli().current_dir(tempdir.path()).arg("validate"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	workspace validation passed for .
	
	----- stderr -----
	"###
	);
}

#[test]
fn discover_cli_json_reports_relative_paths_and_stable_ids() {
	apply_common_filters!();
	let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/mixed");

	assert_cmd_snapshot!(
		cli()
			.current_dir(&fixture_root)
			.arg("discover")
			.arg("--format")
			.arg("json"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	{
	  "dependencies": [
	    {
	      "direct": true,
	      "from": "dart:dart/mobile_sdk/pubspec.yaml",
	      "kind": "runtime",
	      "to": "npm:packages/web-sdk/package.json"
	    },
	    {
	      "direct": true,
	      "from": "deno:deno/tool/deno.json",
	      "kind": "runtime",
	      "to": "npm:packages/web-sdk/package.json"
	    },
	    {
	      "direct": true,
	      "from": "npm:packages/web-sdk/package.json",
	      "kind": "runtime",
	      "to": "cargo:cargo/sdk-core/Cargo.toml"
	    }
	  ],
	  "packages": [
	    {
	      "ecosystem": "cargo",
	      "id": "cargo:cargo/sdk-core/Cargo.toml",
	      "manifestPath": "cargo/sdk-core/Cargo.toml",
	      "name": "sdk-core",
	      "publishState": "public",
	      "version": "1.0.0",
	      "versionGroup": "sdk",
	      "workspaceRoot": "."
	    },
	    {
	      "ecosystem": "dart",
	      "id": "dart:dart/mobile_sdk/pubspec.yaml",
	      "manifestPath": "dart/mobile_sdk/pubspec.yaml",
	      "name": "mobile_sdk",
	      "publishState": "public",
	      "version": "1.0.0",
	      "versionGroup": null,
	      "workspaceRoot": "."
	    },
	    {
	      "ecosystem": "deno",
	      "id": "deno:deno/tool/deno.json",
	      "manifestPath": "deno/tool/deno.json",
	      "name": "deno-tool",
	      "publishState": "public",
	      "version": "1.0.0",
	      "versionGroup": null,
	      "workspaceRoot": "."
	    },
	    {
	      "ecosystem": "npm",
	      "id": "npm:packages/web-sdk/package.json",
	      "manifestPath": "packages/web-sdk/package.json",
	      "name": "web-sdk",
	      "publishState": "public",
	      "version": "1.0.0",
	      "versionGroup": "sdk",
	      "workspaceRoot": "."
	    }
	  ],
	  "versionGroups": [
	    {
	      "id": "sdk",
	      "members": [
	        "cargo:cargo/sdk-core/Cargo.toml",
	        "npm:packages/web-sdk/package.json"
	      ],
	      "mismatchDetected": false
	    }
	  ],
	  "warnings": [],
	  "workspaceRoot": "."
	}
	
	----- stderr -----
	"###
	);
}

#[test]
fn change_cli_writes_requested_file_contents() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_release_fixture(tempdir.path());
	let output_path = tempdir.path().join("feature.md");

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("change")
			.arg("--package")
			.arg("core")
			.arg("--bump")
			.arg("minor")
			.arg("--reason")
			.arg("document cli snapshots")
			.arg("--output")
			.arg(&output_path),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	wrote change file feature.md
	
	----- stderr -----
	"###
	);

	let change_file =
		fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("change file: {error}"));
	assert_snapshot!(change_file, @r###"
---
core: minor
---

#### document cli snapshots
"###);
}

#[test]
fn change_cli_writes_explicit_versions_when_requested() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_release_fixture(tempdir.path());
	let output_path = tempdir.path().join("versioned.md");

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("change")
			.arg("--package")
			.arg("core")
			.arg("--bump")
			.arg("major")
			.arg("--version")
			.arg("2.0.0")
			.arg("--reason")
			.arg("promote to stable")
			.arg("--output")
			.arg(&output_path),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	wrote change file versioned.md
	
	----- stderr -----
	"###
	);

	let change_file =
		fs::read_to_string(&output_path).unwrap_or_else(|error| panic!("change file: {error}"));
	assert_snapshot!(change_file, @r###"
---
core:
  bump: major
  version: "2.0.0"
---

#### promote to stable
"###);
}

#[test]
fn release_dry_run_cli_patches_parent_packages_when_dependencies_change() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_release_fixture(tempdir.path());

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--format")
			.arg("text"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	command `release` completed (dry-run)
	released packages: workflow-app, workflow-core
	release targets:
	- package app -> app/v1.0.1 (tag: false, release: false)
	- package core -> core/v1.1.0 (tag: false, release: false)
	changed files:
	- crates/app/Cargo.toml
	- crates/core/Cargo.toml
	
	----- stderr -----
	"###
	);
}

#[test]
fn release_dry_run_cli_uses_explicit_group_versions_from_member_changes() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_release_fixture(tempdir.path());
	write_file(
		tempdir.path().join(".changeset/feature.md"),
		r"---
core:
  bump: major
  version: 2.0.0
---

#### promote sdk to stable
",
	);

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--format")
			.arg("text"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	command `release` completed (dry-run)
	version: 2.0.0
	released packages: workflow-app, workflow-core
	release targets:
	- group sdk -> v2.0.0 (tag: true, release: true)
	changed files:
	- Cargo.toml
	- changelog.md
	- crates/app/Cargo.toml
	- crates/core/CHANGELOG.md
	- crates/core/Cargo.toml
	- group.toml
	
	----- stderr -----
	"###
	);
}

#[test]
fn release_dry_run_cli_json_exposes_group_owned_release_targets() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_release_fixture(tempdir.path());

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("release")
			.arg("--dry-run")
			.arg("--format")
			.arg("json"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	{
	  "command": "release",
	  "dryRun": true,
	  "version": "1.1.0",
	  "groupVersion": "1.1.0",
	  "releaseTargets": [
	    {
	      "id": "sdk",
	      "kind": "group",
	      "version": "1.1.0",
	      "tag": true,
	      "release": true,
	      "versionFormat": "primary",
	      "tagName": "v1.1.0",
	      "members": [
	        "core",
	        "app"
	      ]
	    }
	  ],
	  "releasedPackages": [
	    "workflow-app",
	    "workflow-core"
	  ],
	  "changedFiles": [
	    "Cargo.toml",
	    "changelog.md",
	    "crates/app/Cargo.toml",
	    "crates/core/CHANGELOG.md",
	    "crates/core/Cargo.toml",
	    "group.toml"
	  ],
	  "changelogs": [
	    {
	      "ownerId": "sdk",
	      "ownerKind": "group",
	      "path": "changelog.md",
	      "format": "monochange",
	      "notes": {
	        "title": "1.1.0",
	        "summary": [
	          "Grouped release for `sdk`.",
	          "Changed members: core",
	          "Synchronized members: app"
	        ],
	        "sections": [
	          {
	            "title": "Features",
	            "entries": [
	              "- **core**: add feature"
	            ]
	          }
	        ]
	      },
	      "rendered": "## 1.1.0\n\nGrouped release for `sdk`.\n\nChanged members: core\n\nSynchronized members: app\n\n### Features\n\n- **core**: add feature"
	    },
	    {
	      "ownerId": "core",
	      "ownerKind": "package",
	      "path": "crates/core/CHANGELOG.md",
	      "format": "monochange",
	      "notes": {
	        "title": "1.1.0",
	        "summary": [],
	        "sections": [
	          {
	            "title": "Features",
	            "entries": [
	              "- add feature"
	            ]
	          }
	        ]
	      },
	      "rendered": "## 1.1.0\n\n### Features\n\n- add feature"
	    }
	  ],
	  "changesets": [
	    {
	      "path": ".changeset/feature.md",
	      "summary": "add feature",
	      "details": null,
	      "targets": [
	        {
	          "id": "core",
	          "kind": "package",
	          "bump": "minor",
	          "origin": "direct-change",
	          "evidenceRefs": [],
	          "changeType": null
	        }
	      ],
	      "context": {
	        "provider": "generic_git",
	        "host": null,
	        "capabilities": {
	          "commitWebUrls": false,
	          "actorProfiles": false,
	          "reviewRequestLookup": false,
	          "relatedIssues": false,
	          "issueComments": false
	        },
	        "introduced": null,
	        "lastUpdated": null,
	        "relatedIssues": []
	      }
	    }
	  ],
	  "deletedChangesets": [],
	  "plan": {
	    "workspaceRoot": ".",
	    "decisions": [
	      {
	        "package": "cargo:crates/app/Cargo.toml",
	        "bump": "minor",
	        "trigger": "version-group-synchronization",
	        "plannedVersion": "1.1.0",
	        "reasons": [
	          "depends on `cargo:crates/core/Cargo.toml`",
	          "shares version group `sdk`"
	        ],
	        "upstreamSources": [
	          "cargo:crates/core/Cargo.toml"
	        ]
	      },
	      {
	        "package": "cargo:crates/core/Cargo.toml",
	        "bump": "minor",
	        "trigger": "direct-change",
	        "plannedVersion": "1.1.0",
	        "reasons": [
	          "add feature",
	          "shares version group `sdk`"
	        ],
	        "upstreamSources": [
	          "cargo:crates/core/Cargo.toml"
	        ]
	      }
	    ],
	    "groups": [
	      {
	        "id": "sdk",
	        "plannedVersion": "1.1.0",
	        "members": [
	          "cargo:crates/core/Cargo.toml",
	          "cargo:crates/app/Cargo.toml"
	        ],
	        "bump": "minor"
	      }
	    ],
	    "warnings": [],
	    "unresolvedItems": [],
	    "compatibilityEvidence": []
	  }
	}
	
	----- stderr -----
	"###
	);
}

#[test]
fn verify_cli_json_reports_failure_comment() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_changeset_policy_fixture(tempdir.path(), false);

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("affected")
			.arg("--format")
			.arg("json")
			.arg("--changed-paths")
			.arg("crates/core/src/lib.rs"),
		@r####"
	success: true
	exit_code: 0
	----- stdout -----
	{
	  "status": "failed",
	  "required": true,
	  "enforce": false,
	  "summary": "changeset verification failed: attached changesets do not cover 1 changed package",
	  "comment": "### MonoChange changeset verification failed\n\nchangeset verification failed: attached changesets do not cover 1 changed package\n\nChanged package paths:\n- `crates/core/src/lib.rs`\n\nAffected packages:\n- `core`\n\nErrors:\n- changed packages are not covered by attached changesets: core\n\nAllowed skip labels:\n- `no-changeset-required`\n\nHow to fix:\n- add or update a `.changeset/*.md` file so it references every changed package or owning group\n- for example: `mc change --package <id> --bump patch --reason \"describe the change\"`\n- or apply one of the configured skip labels when no release note is required",
	  "labels": [],
	  "matchedSkipLabels": [],
	  "changedPaths": [
	    "crates/core/src/lib.rs"
	  ],
	  "matchedPaths": [
	    "crates/core/src/lib.rs"
	  ],
	  "ignoredPaths": [],
	  "changesetPaths": [],
	  "affectedPackageIds": [
	    "core"
	  ],
	  "coveredPackageIds": [],
	  "uncoveredPackageIds": [
	    "core"
	  ],
	  "errors": [
	    "changed packages are not covered by attached changesets: core"
	  ]
	}
	
	----- stderr -----
	"####
	);
}

#[test]
fn release_pr_workflow_reports_dry_run_pull_request_preview() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_release_pr_workflow_fixture(tempdir.path());

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("release-pr")
			.arg("--dry-run"),
		@r"
	success: true
	exit_code: 0
	----- stdout -----
	command `release-pr` completed (dry-run)
	version: 1.1.0
	released packages: workflow-app, workflow-core
	release targets:
	- group sdk -> v1.1.0 (tag: true, release: true)
	release request:
	- dry-run ifiokjr/monochange monochange/release/release-pr -> main via github
	changed files:
	- Cargo.toml
	- changelog.md
	- crates/app/Cargo.toml
	- crates/core/CHANGELOG.md
	- crates/core/Cargo.toml
	- group.toml

	----- stderr -----
	"
	);
}

#[test]
fn release_manifest_workflow_writes_manifest_json() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_manifest_workflow_fixture(tempdir.path());

	assert_cmd_snapshot!(
		cli()
			.current_dir(tempdir.path())
			.arg("release-manifest")
			.arg("--dry-run"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	command `release-manifest` completed (dry-run)
	version: 1.1.0
	released packages: workflow-app, workflow-core
	release targets:
	- group sdk -> v1.1.0 (tag: true, release: true)
	release manifest: .monochange/release-manifest.json
	changed files:
	- Cargo.toml
	- changelog.md
	- crates/app/Cargo.toml
	- crates/core/CHANGELOG.md
	- crates/core/Cargo.toml
	- group.toml
	
	----- stderr -----
	"###
	);

	let manifest_path = tempdir.path().join(".monochange/release-manifest.json");
	let manifest = fs::read_to_string(&manifest_path)
		.unwrap_or_else(|error| panic!("read manifest {}: {error}", manifest_path.display()));
	assert_snapshot!(manifest, @r###"
	{
	  "command": "release-manifest",
	  "dryRun": true,
	  "version": "1.1.0",
	  "groupVersion": "1.1.0",
	  "releaseTargets": [
	    {
	      "id": "sdk",
	      "kind": "group",
	      "version": "1.1.0",
	      "tag": true,
	      "release": true,
	      "versionFormat": "primary",
	      "tagName": "v1.1.0",
	      "members": [
	        "core",
	        "app"
	      ]
	    }
	  ],
	  "releasedPackages": [
	    "workflow-app",
	    "workflow-core"
	  ],
	  "changedFiles": [
	    "Cargo.toml",
	    "changelog.md",
	    "crates/app/Cargo.toml",
	    "crates/core/CHANGELOG.md",
	    "crates/core/Cargo.toml",
	    "group.toml"
	  ],
	  "changelogs": [
	    {
	      "ownerId": "sdk",
	      "ownerKind": "group",
	      "path": "changelog.md",
	      "format": "monochange",
	      "notes": {
	        "title": "1.1.0",
	        "summary": [
	          "Grouped release for `sdk`.",
	          "Changed members: core",
	          "Synchronized members: app"
	        ],
	        "sections": [
	          {
	            "title": "Features",
	            "entries": [
	              "- **core**: add feature"
	            ]
	          }
	        ]
	      },
	      "rendered": "## 1.1.0\n\nGrouped release for `sdk`.\n\nChanged members: core\n\nSynchronized members: app\n\n### Features\n\n- **core**: add feature"
	    },
	    {
	      "ownerId": "core",
	      "ownerKind": "package",
	      "path": "crates/core/CHANGELOG.md",
	      "format": "monochange",
	      "notes": {
	        "title": "1.1.0",
	        "summary": [],
	        "sections": [
	          {
	            "title": "Features",
	            "entries": [
	              "- add feature"
	            ]
	          }
	        ]
	      },
	      "rendered": "## 1.1.0\n\n### Features\n\n- add feature"
	    }
	  ],
	  "changesets": [
	    {
	      "path": ".changeset/feature.md",
	      "summary": "add feature",
	      "details": null,
	      "targets": [
	        {
	          "id": "core",
	          "kind": "package",
	          "bump": "minor",
	          "origin": "direct-change",
	          "evidenceRefs": [],
	          "changeType": null
	        }
	      ],
	      "context": {
	        "provider": "generic_git",
	        "host": null,
	        "capabilities": {
	          "commitWebUrls": false,
	          "actorProfiles": false,
	          "reviewRequestLookup": false,
	          "relatedIssues": false,
	          "issueComments": false
	        },
	        "introduced": null,
	        "lastUpdated": null,
	        "relatedIssues": []
	      }
	    }
	  ],
	  "deletedChangesets": [],
	  "plan": {
	    "workspaceRoot": ".",
	    "decisions": [
	      {
	        "package": "cargo:crates/app/Cargo.toml",
	        "bump": "minor",
	        "trigger": "version-group-synchronization",
	        "plannedVersion": "1.1.0",
	        "reasons": [
	          "depends on `cargo:crates/core/Cargo.toml`",
	          "shares version group `sdk`"
	        ],
	        "upstreamSources": [
	          "cargo:crates/core/Cargo.toml"
	        ]
	      },
	      {
	        "package": "cargo:crates/core/Cargo.toml",
	        "bump": "minor",
	        "trigger": "direct-change",
	        "plannedVersion": "1.1.0",
	        "reasons": [
	          "add feature",
	          "shares version group `sdk`"
	        ],
	        "upstreamSources": [
	          "cargo:crates/core/Cargo.toml"
	        ]
	      }
	    ],
	    "groups": [
	      {
	        "id": "sdk",
	        "plannedVersion": "1.1.0",
	        "members": [
	          "cargo:crates/core/Cargo.toml",
	          "cargo:crates/app/Cargo.toml"
	        ],
	        "bump": "minor"
	      }
	    ],
	    "warnings": [],
	    "unresolvedItems": [],
	    "compatibilityEvidence": []
	  }
	}
	"###);
}

#[test]
fn release_cli_reports_missing_changesets_cleanly() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_ungrouped_release_fixture(tempdir.path());
	fs::remove_file(tempdir.path().join(".changeset/feature.md"))
		.unwrap_or_else(|error| panic!("remove changeset: {error}"));

	assert_cmd_snapshot!(
		cli().current_dir(tempdir.path()).arg("release"),
		@r###"
	success: false
	exit_code: 1
	----- stdout -----
	
	----- stderr -----
	config error: no markdown changesets found under .changeset
	"###
	);
}

#[test]
fn release_cli_writes_group_changelog_and_skips_packages_without_changelogs() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	seed_group_release_fixture(tempdir.path());

	assert_cmd_snapshot!(
		cli().current_dir(tempdir.path()).arg("release"),
		@r###"
	success: true
	exit_code: 0
	----- stdout -----
	command `release` completed
	version: 1.1.0
	released packages: workflow-app, workflow-core
	release targets:
	- group sdk -> v1.1.0 (tag: true, release: true)
	changed files:
	- Cargo.toml
	- changelog.md
	- crates/app/Cargo.toml
	- crates/core/CHANGELOG.md
	- crates/core/Cargo.toml
	- group.toml
	deleted changesets:
	- .changeset/feature.md
	
	----- stderr -----
	"###
	);

	let root_changelog = fs::read_to_string(tempdir.path().join("changelog.md"))
		.unwrap_or_else(|error| panic!("group changelog: {error}"));
	let core_changelog = fs::read_to_string(tempdir.path().join("crates/core/CHANGELOG.md"))
		.unwrap_or_else(|error| panic!("core changelog: {error}"));
	let workspace_manifest = fs::read_to_string(tempdir.path().join("Cargo.toml"))
		.unwrap_or_else(|error| panic!("workspace manifest: {error}"));
	let group_versioned_file = fs::read_to_string(tempdir.path().join("group.toml"))
		.unwrap_or_else(|error| panic!("group versioned file: {error}"));

	assert!(root_changelog.contains("Grouped release for `sdk`."));
	assert!(root_changelog.contains("Changed members: core"));
	assert!(root_changelog.contains("Synchronized members: app"));
	assert!(core_changelog.contains("## 1.1.0"));
	assert!(core_changelog.contains("- add feature"));
	assert!(!tempdir.path().join("crates/app/CHANGELOG.md").exists());
	assert!(!tempdir.path().join("crates/app/changelog.md").exists());
	assert!(workspace_manifest.contains("version = \"1.1.0\""));
	assert!(group_versioned_file.contains("version = \"1.1.0\""));
}

#[test]
fn validate_cli_rejects_packages_in_multiple_groups() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	write_file(
		tempdir.path().join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
	);
	write_file(
		tempdir.path().join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[package.core]
path = "crates/core"

[group.sdk]
packages = ["core"]

[group.cli]
packages = ["core"]
"#,
	);

	assert_cmd_snapshot!(
		cli().current_dir(tempdir.path()).arg("validate"),
		@r###"
	success: false
	exit_code: 1
	----- stdout -----
	
	----- stderr -----
	error: package `core` belongs to multiple groups: `cli` and `sdk`
	--> monochange.toml
	labels:
	- first group membership @ bytes [OFFSET]..[END]
	- conflicting group membership @ bytes [OFFSET]..[END]
	help: move the package into exactly one [group.<id>] declaration
	"###
	);
}

fn copy_directory(source: &Path, destination: &Path) {
	fs::create_dir_all(destination)
		.unwrap_or_else(|error| panic!("create destination {}: {error}", destination.display()));
	for entry in fs::read_dir(source)
		.unwrap_or_else(|error| panic!("read dir {}: {error}", source.display()))
	{
		let entry = entry.unwrap_or_else(|error| panic!("dir entry: {error}"));
		let source_path = entry.path();
		let destination_path = destination.join(entry.file_name());
		let file_type = entry
			.file_type()
			.unwrap_or_else(|error| panic!("file type {}: {error}", source_path.display()));
		if file_type.is_dir() {
			copy_directory(&source_path, &destination_path);
		} else if file_type.is_file() {
			if let Some(parent) = destination_path.parent() {
				fs::create_dir_all(parent)
					.unwrap_or_else(|error| panic!("create parent {}: {error}", parent.display()));
			}
			fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
				panic!(
					"copy {} -> {}: {error}",
					source_path.display(),
					destination_path.display()
				)
			});
		}
	}
}

fn seed_ungrouped_release_fixture(root: &Path) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
workflow-core = { path = "./crates/core", version = "1.0.0" }
workflow-app = { path = "./crates/app", version = "1.0.0" }
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		r#"
[package]
name = "workflow-core"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(
		root.join("crates/app/Cargo.toml"),
		r#"
[package]
name = "workflow-app"
version = { workspace = true }
edition = "2021"

[dependencies]
workflow-core = { workspace = true }
"#,
	);
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"

[package.core]
path = "crates/core"

[package.app]
path = "crates/app"

[ecosystems.cargo]
enabled = true
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add feature
",
	);
}

fn seed_group_release_fixture(root: &Path) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"

[workspace.package]
version = "1.0.0"

[workspace.dependencies]
workflow-core = { path = "./crates/core", version = "1.0.0" }
workflow-app = { path = "./crates/app", version = "1.0.0" }
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		r#"
[package]
name = "workflow-core"
version = { workspace = true }
edition = "2021"
"#,
	);
	write_file(
		root.join("crates/app/Cargo.toml"),
		r#"
[package]
name = "workflow-app"
version = { workspace = true }
edition = "2021"

[dependencies]
workflow-core = { workspace = true }
"#,
	);
	write_file(root.join("changelog.md"), "# Changelog\n");
	write_file(
		root.join("group.toml"),
		"[workspace.package]\nversion = \"1.0.0\"\n[workspace.dependencies]\nworkflow-core = { version = \"1.0.0\" }\nworkflow-app = { version = \"1.0.0\" }\n",
	);
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = false

[package.core]
path = "crates/core"
changelog = true

[package.app]
path = "crates/app"
changelog = false

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
versioned_files = [{ path = "group.toml", type = "cargo" }]
tag = true
release = true
version_format = "primary"

[ecosystems.cargo]
enabled = true

[cli.release]

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.release.steps]]
type = "PrepareRelease"
"#,
	);
	write_file(
		root.join(".changeset/feature.md"),
		r"---
core: minor
---

#### add feature
",
	);
}

fn seed_manifest_workflow_fixture(root: &Path) {
	seed_group_release_fixture(root);
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = false

[package.core]
path = "crates/core"
changelog = true

[package.app]
path = "crates/app"
changelog = false

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
versioned_files = [{ path = "group.toml", type = "cargo" }]
tag = true
release = true
version_format = "primary"

[ecosystems.cargo]
enabled = true

[cli.release-manifest]

[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"
"#,
	);
}

fn seed_changeset_policy_fixture(root: &Path, with_changeset: bool) {
	write_file(
		root.join("Cargo.toml"),
		r#"
[workspace]
members = ["crates/*"]
resolver = "2"
"#,
	);
	write_file(
		root.join("crates/core/Cargo.toml"),
		"[package]\nname = \"core\"\nversion = \"1.0.0\"\nedition = \"2021\"\n",
	);
	write_file(root.join("crates/core/src/lib.rs"), "pub fn core() {}\n");
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
package_type = "cargo"

[changesets.verify]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true

[package.core]
path = "crates/core"

[cli.affected]

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"

[[cli.affected.inputs]]
name = "since"
type = "string"

[[cli.affected.inputs]]
name = "verify"
type = "boolean"

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.steps]]
type = "AffectedPackages"
"#,
	);
	if with_changeset {
		write_file(
			root.join(".changeset/feature.md"),
			r"---
core: patch
---

#### add feature
",
		);
	}
}

fn seed_release_pr_workflow_fixture(root: &Path) {
	seed_group_release_fixture(root);
	write_file(
		root.join("monochange.toml"),
		r#"
[defaults]
parent_bump = "patch"
package_type = "cargo"
changelog = false

[package.core]
path = "crates/core"
changelog = true

[package.app]
path = "crates/app"
changelog = false

[group.sdk]
packages = ["core", "app"]
changelog = "changelog.md"
versioned_files = [{ path = "group.toml", type = "cargo" }]
tag = true
release = true
version_format = "primary"

[github]
owner = "ifiokjr"
repo = "monochange"

[github.pull_requests]
branch_prefix = "monochange/release"
base = "main"
labels = ["release", "automated"]

[ecosystems.cargo]
enabled = true

[cli.release-pr]

[[cli.release-pr.steps]]
type = "PrepareRelease"

[[cli.release-pr.steps]]
type = "OpenReleaseRequest"
"#,
	);
}

fn write_file(path: impl AsRef<Path>, content: &str) {
	let path = path.as_ref();
	if let Some(parent) = path.parent() {
		fs::create_dir_all(parent).unwrap_or_else(|error| panic!("create dir: {error}"));
	}
	fs::write(path, content)
		.unwrap_or_else(|error| panic!("write file {}: {error}", path.display()));
}
