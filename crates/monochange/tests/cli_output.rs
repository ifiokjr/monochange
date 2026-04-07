use std::fs;
use std::path::Path;
use std::process::Command;

use insta::assert_snapshot;
use insta_cmd::assert_cmd_snapshot;
use insta_cmd::get_cargo_bin;
use tempfile::tempdir;

mod test_support;
use test_support::{copy_directory, fixture_path};

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
			settings.add_filter(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}", "[DATETIME]");
			settings.add_filter(r"\d{4}-\d{2}-\d{2}", "[DATE]");
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
fn change_cli_help_documents_package_and_group_targeting_rules() {
	apply_common_filters!();

	assert_cmd_snapshot!(
		cli().arg("change").arg("--help"),
		@r#"
	success: true
	exit_code: 0
	----- stdout -----
	Create a change file for one or more packages

	Usage: mc change [OPTIONS]

	Options:
	      --dry-run            Run the command in dry-run mode when supported
	  -i, --interactive        Select packages, bumps, and options interactively
	      --package <PACKAGE>  Package or group to include in the change
	      --bump <BUMP>        Requested semantic version bump [default: patch] [possible values: none, patch, minor, major]
	      --version <VERSION>  Pin an explicit version for this release
	      --reason <REASON>    Short release-note summary for this change
	      --type <TYPE>        Optional release-note type such as `security` or `note`
	      --details <DETAILS>  Optional multi-line release-note details
	      --output <PATH>      Write the generated change file to a specific path
	  -h, --help               Print help

	Examples:
	  mc change --package sdk-core --bump patch --reason "fix panic"
	  mc change --package sdk-core --bump minor --reason "add API" --output .changeset/sdk-core.md
	  mc change --package sdk --bump minor --reason "coordinated release"

	Rules:
	  - Prefer configured package ids in change files whenever a leaf package changed.
	  - Use a group id only when the change is intentionally owned by the whole group.
	  - Dependents and grouped members are propagated automatically during planning.
	  - Legacy manifest paths may still resolve during migration, but declared ids are the stable interface.


	----- stderr -----
	"#
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
	copy_directory(&fixture_path("cli-output/ungrouped-basic"), tempdir.path());
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

# document cli snapshots
"###);
}

#[test]
fn change_cli_writes_explicit_versions_when_requested() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("cli-output/ungrouped-basic"), tempdir.path());
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

# promote to stable
"###);
}

#[test]
fn release_dry_run_cli_patches_parent_packages_when_dependencies_change() {
	apply_common_filters!();
	let tempdir = tempdir().unwrap_or_else(|error| panic!("tempdir: {error}"));
	copy_directory(&fixture_path("cli-output/ungrouped-basic"), tempdir.path());

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
	copy_directory(
		&fixture_path("cli-output/group-explicit-version"),
		tempdir.path(),
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
	copy_directory(&fixture_path("cli-output/group-basic"), tempdir.path());

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
	      ],
	      "renderedTitle": "1.1.0 ([DATE])",
	      "renderedChangelogTitle": "1.1.0 ([DATE])"
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
	        "title": "1.1.0 ([DATE])",
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
	      "rendered": "## 1.1.0 ([DATE])\n\nGrouped release for `sdk`.\n\nChanged members: core\n\nSynchronized members: app\n\n### Features\n\n- **core**: add feature"
	    },
	    {
	      "ownerId": "core",
	      "ownerKind": "package",
	      "path": "crates/core/CHANGELOG.md",
	      "format": "monochange",
	      "notes": {
	        "title": "1.1.0 ([DATE])",
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
	      "rendered": "## 1.1.0 ([DATE])\n\n### Features\n\n- add feature"
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
	copy_directory(
		&fixture_path("cli-output/changeset-policy-no-changeset"),
		tempdir.path(),
	);

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
	copy_directory(
		&fixture_path("cli-output/release-pr-workflow"),
		tempdir.path(),
	);

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
	copy_directory(
		&fixture_path("cli-output/release-manifest-workflow"),
		tempdir.path(),
	);

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
	      ],
	      "renderedTitle": "1.1.0 ([DATE])",
	      "renderedChangelogTitle": "1.1.0 ([DATE])"
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
	        "title": "1.1.0 ([DATE])",
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
	      "rendered": "## 1.1.0 ([DATE])\n\nGrouped release for `sdk`.\n\nChanged members: core\n\nSynchronized members: app\n\n### Features\n\n- **core**: add feature"
	    },
	    {
	      "ownerId": "core",
	      "ownerKind": "package",
	      "path": "crates/core/CHANGELOG.md",
	      "format": "monochange",
	      "notes": {
	        "title": "1.1.0 ([DATE])",
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
	      "rendered": "## 1.1.0 ([DATE])\n\n### Features\n\n- add feature"
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
	copy_directory(
		&fixture_path("cli-output/ungrouped-no-changeset"),
		tempdir.path(),
	);

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
	copy_directory(&fixture_path("cli-output/group-basic"), tempdir.path());

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
	copy_directory(
		&fixture_path("cli-output/multiple-groups-validation"),
		tempdir.path(),
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
