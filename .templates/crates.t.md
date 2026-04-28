<!-- {@crateReadmeBadgeRow:"crate_name"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-{{ crate_name|replace("_", "__") }}-orange?logo=rust)](https://crates.io/crates/{{ crate_name }}) [![Docs.rs](https://img.shields.io/badge/docs.rs-{{ crate_name|replace("_", "__") }}-1f425f?logo=docs.rs)](https://docs.rs/{{ crate_name }}/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag={{ crate_name }})](https://codecov.io/gh/monochange/monochange?flag={{ crate_name }}) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<!-- {@monochangeCrateDocs} -->

`monochange` is the top-level entry point for the workspace.

Reach for this crate when you want one API and CLI surface that discovers packages across Cargo, npm/pnpm/Bun, Deno, Dart/Flutter, and Python workspaces, exposes top-level commands from `monochange.toml`, and runs configured CLI commands from those definitions.

## Why use it?

- coordinate one config-defined CLI across several package ecosystems
- expose discovery, change creation, and release preparation as both commands and library calls
- connect configuration loading, package discovery, graph propagation, and semver evidence in one place

## Best for

- shipping the `mc` CLI in CI or local release tooling
- embedding the full end-to-end planner instead of wiring the lower-level crates together yourself
- generating starter config with `mc init` and then evolving the CLI command surface over time

## Key commands

```bash
mc init
mc skill -a pi -y
mc discover --format json
mc change --package monochange --bump patch --reason "describe the change"
mc release --dry-run --format json
mc mcp
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- load config-defined `[cli.*]` workflow commands from `monochange.toml`
- expose hardcoded binary commands such as `init`, `validate`, `check`, `analyze`, `mcp`, `help`, and `version`
- generate immutable `mc step:*` commands from the built-in step schemas
- resolve change input files
- render discovery and release command output in text or JSON
- execute configured workflow commands plus built-in MCP commands
- preview or publish provider releases from prepared release data
- evaluate pull-request changeset policy from CI-supplied changed paths and labels
- expose JSON-first MCP tools for assistant workflows

<!-- {/monochangeCrateDocs} -->

<!-- {@monochangeCoreCrateDocs} -->

`monochange_core` is the shared vocabulary for the `monochange` workspace.

Reach for this crate when you are building ecosystem adapters, release planners, or custom automation and need one set of types for packages, dependency edges, version groups, change signals, and release plans.

## Why use it?

- avoid redefining package and release domain models in each crate
- share one error and result surface across discovery, planning, and command layers
- pass normalized workspace data between adapters and planners without extra translation

## Best for

- implementing new ecosystem adapters against the shared `EcosystemAdapter` contract
- moving normalized package or release data between crates without custom conversion code
- depending on the workspace domain model without pulling in discovery or planning behavior

## What it provides

- normalized package and dependency records
- version-group definitions and planned group outcomes
- change signals and compatibility assessments
- changelog formats, changelog targets, structured release-note types, release-manifest types, source-automation config types, and changeset-policy evaluation types
- shared error and result types

## Example

```rust
use monochange_core::render_release_notes;
use monochange_core::ChangelogFormat;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;

let notes = ReleaseNotesDocument {
    title: "1.2.3".to_string(),
    summary: vec!["Grouped release for `sdk`.".to_string()],
    sections: vec![ReleaseNotesSection {
        title: "Features".to_string(),
        entries: vec!["- add keep-a-changelog output".to_string()],
        collapsed: false,
    }],
};

let rendered = render_release_notes(ChangelogFormat::KeepAChangelog, &notes);

assert!(rendered.contains("## 1.2.3"));
assert!(rendered.contains("### Features"));
assert!(rendered.contains("- add keep-a-changelog output"));
```

<!-- {/monochangeCoreCrateDocs} -->

<!-- {@monochangeAnalysisCrateDocs} -->

`monochange_analysis` turns git diff context into artifact-aware changeset suggestions.

Reach for this crate when you want to classify changed packages as libraries, applications, CLI tools, or mixed artifacts and then extract the most user-facing parts of the diff.

## Why use it?

- convert raw changed files into package-centric semantic summaries
- use different heuristics for libraries, applications, and CLI tools
- reuse one analysis pipeline across CLI, MCP, and CI automation

## Best for

- suggesting changeset boundaries before writing `.changeset/*.md` files
- analyzing pull-request or branch diffs in assistant workflows
- experimenting with artifact-aware release note generation

## Public entry points

- `ChangeFrame::detect(root)` selects the git frame to analyze
- `detect_artifact_type(package_path)` classifies a package as a library, application, CLI tool, or mixed artifact
- `analyze_changes(root, frame, config)` returns package analyses and suggested changesets

## Scope

- git-aware frame detection
- artifact classification
- semantic diff extraction
- adaptive suggestion grouping

<!-- {/monochangeAnalysisCrateDocs} -->

<!-- {@monochangeCargoCrateDocs} -->

`monochange_cargo` discovers Cargo packages and surfaces Rust-specific release evidence.

Reach for this crate when you want to scan Cargo workspaces into normalized `monochange_core` records and optionally feed Rust semver evidence into release planning.

## Why use it?

- discover Cargo workspaces and standalone crates with one adapter
- normalize crate manifests and dependency edges for the shared planner
- attach Rust semver evidence through `RustSemverProvider`

## Best for

- building Cargo-aware discovery flows without the full CLI
- feeding Rust semver evidence into release planning
- converting Cargo workspace structure into shared `monochange_core` records

## Public entry points

- `discover_cargo_packages(root)` discovers Cargo workspaces and standalone crates
- `CargoAdapter` exposes the shared adapter interface
- `RustSemverProvider` parses explicit Rust semver evidence from change input

## Scope

- Cargo workspace glob expansion
- crate manifest parsing
- normalized dependency extraction
- Rust semver provider integration for release planning

<!-- {/monochangeCargoCrateDocs} -->

<!-- {@monochangeConfigCrateDocs} -->

`monochange_config` parses and validates the inputs that drive planning and release commands.

Reach for this crate when you need to load `monochange.toml`, resolve package references, or turn `.changeset/*.md` files into validated change signals for the planner.

## Why use it?

- centralize config parsing and validation rules in one place
- resolve package references against discovered workspace packages
- keep CLI command definitions, version groups, and change files aligned with the planner's expectations

## Best for

- validating configuration before handing it to planning code
- parsing and resolving change files in custom automation
- keeping package-reference rules consistent across tools

## Public entry points

- `load_workspace_configuration(root)` loads and validates `monochange.toml`
- `load_change_signals(root, changes_dir, packages)` parses markdown change files into change signals
- `resolve_package_reference(reference, workspace_root, packages)` maps package names, ids, and paths to discovered packages
- `apply_version_groups(packages, configuration)` attaches configured version groups to discovered packages

## Responsibilities

- load `monochange.toml`
- validate version groups and CLI commands
- resolve package references against discovered packages
- parse change-input files, evidence, release-note `type` / `details` fields, changelog paths, changelog format overrides, source-provider config, changeset-bot policy config, and command release/manifest/policy steps

## Example

```rust
use monochange_config::load_workspace_configuration;
use monochange_core::ChangelogFormat;

let root = std::env::temp_dir().join("monochange-config-changelog-format-docs");
let _ = std::fs::remove_dir_all(&root);
std::fs::create_dir_all(root.join("crates/core")).unwrap();
std::fs::write(
    root.join("crates/core/Cargo.toml"),
    "[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
)
.unwrap();
std::fs::write(
    root.join("monochange.toml"),
    r#"
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/CHANGELOG.md"
format = "keep_a_changelog"

[package.core]
path = "crates/core"
"#,
)
.unwrap();

let configuration = load_workspace_configuration(&root).unwrap();
let package = configuration.package_by_id("core").unwrap();

assert_eq!(configuration.defaults.changelog_format, ChangelogFormat::KeepAChangelog);
assert_eq!(package.changelog.as_ref().unwrap().format, ChangelogFormat::KeepAChangelog);
assert_eq!(package.changelog.as_ref().unwrap().path, std::path::PathBuf::from("crates/core/CHANGELOG.md"));

let _ = std::fs::remove_dir_all(&root);
```

<!-- {/monochangeConfigCrateDocs} -->

<!-- {@monochangeGraphCrateDocs} -->

`monochange_graph` turns normalized workspace data into release decisions.

Reach for this crate when you already have discovered packages, dependency edges, configuration, and change signals and need to calculate propagated bumps, synchronized version groups, and final release-plan output.

## Why use it?

- calculate release impact across direct and transitive dependents
- keep version groups synchronized during planning
- produce one deterministic release plan from normalized input data

## Best for

- embedding release-planning logic in custom automation or other tools
- computing the exact set of packages that need to move after a change
- separating planning logic from ecosystem-specific discovery code

## Public entry points

- `NormalizedGraph` builds adjacency and reverse-dependency views over package data
- `build_release_plan(workspace_root, packages, dependency_edges, defaults, version_groups, change_signals, providers)` computes the release plan

## Responsibilities

- build reverse dependency views
- propagate release impact across direct and transitive dependents
- synchronize version groups
- calculate planned group versions

<!-- {/monochangeGraphCrateDocs} -->

<!-- {@monochangeGithubCrateDocs} -->

`monochange_github` turns `monochange` release manifests into GitHub automation requests.

Reach for this crate when you want to preview or publish GitHub releases and release pull requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive GitHub release payloads and release-PR bodies from `monochange`'s structured release manifest
- keep GitHub automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building GitHub release automation on top of `mc release`
- previewing would-be GitHub releases and release PRs in CI before publishing
- converting grouped or package release targets into repository automation payloads

## Public entry points

- `build_release_requests(config, manifest)` converts a release manifest into GitHub release requests
- `publish_release_requests(requests)` publishes requests through the GitHub API via `octocrab`
- `build_release_pull_request_request(config, manifest)` converts a release manifest into a GitHub release-PR request
- `publish_release_pull_request(root, request, tracked_paths)` creates or updates a release PR through `git` and the GitHub API

## Example

```rust
use monochange_core::ProviderBotSettings;
use monochange_core::ProviderMergeRequestSettings;
use monochange_core::ProviderReleaseSettings;
use monochange_core::SourceConfiguration;
use monochange_core::SourceProvider;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use monochange_core::VersionFormat;
use monochange_github::build_release_requests;

let manifest = ReleaseManifest {
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
        members: vec!["core".to_string(), "app".to_string()],
        rendered_title: "1.2.0 (2026-04-06)".to_string(),
        rendered_changelog_title: "[1.2.0](https://example.com) (2026-04-06)".to_string(),
    }],
    released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
    package_publications: Vec::new(),
    changed_files: Vec::new(),
    changesets: Vec::new(),
    changelogs: Vec::new(),
    deleted_changesets: Vec::new(),
    plan: ReleaseManifestPlan {
        workspace_root: std::path::PathBuf::from("."),
        decisions: Vec::new(),
        groups: Vec::new(),
        warnings: Vec::new(),
        unresolved_items: Vec::new(),
        compatibility_evidence: Vec::new(),
    },
};
let github = SourceConfiguration {
    provider: SourceProvider::GitHub,
    owner: "monochange".to_string(),
    repo: "monochange".to_string(),
    host: None,
    api_url: None,
    releases: ProviderReleaseSettings::default(),
    pull_requests: ProviderMergeRequestSettings::default(),
    bot: ProviderBotSettings::default(),
};

let requests = build_release_requests(&github, &manifest);

assert_eq!(requests.len(), 1);
assert_eq!(requests[0].tag_name, "v1.2.0");
assert_eq!(requests[0].repository, "monochange/monochange");
```

<!-- {/monochangeGithubCrateDocs} -->

<!-- {@monochangeGiteaCrateDocs} -->

`monochange_gitea` turns `monochange` release manifests into Gitea automation requests.

Reach for this crate when you want to preview or publish Gitea releases and release pull requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive Gitea release payloads and release-PR bodies from `monochange`'s structured release manifest
- keep Gitea automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building Gitea release automation on top of `mc release`
- previewing would-be Gitea releases and release PRs in CI before publishing
- self-hosted Gitea instances that need the same release workflow as GitHub or GitLab

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a pull-request payload for the release
- `validate_source_configuration(source)` validates Gitea-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeGiteaCrateDocs} -->

<!-- {@monochangeGitlabCrateDocs} -->

`monochange_gitlab` turns `monochange` release manifests into GitLab automation requests.

Reach for this crate when you want to preview or publish GitLab releases and merge requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive GitLab release payloads and merge-request bodies from `monochange`'s structured release manifest
- keep GitLab automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building GitLab release automation on top of `mc release`
- previewing would-be GitLab releases and merge requests in CI before publishing
- self-hosted GitLab instances that need the same release workflow as GitHub

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a merge-request payload for the release
- `validate_source_configuration(source)` validates GitLab-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeGitlabCrateDocs} -->

<!-- {@monochangeHostingCrateDocs} -->

`monochange_hosting` packages the shared git and HTTP plumbing used by hosted source providers.

Reach for this crate when you are implementing GitHub, Gitea, or GitLab release adapters and want one place for release-body rendering, change-request branch naming, JSON requests, and git branch orchestration.

## Why use it?

- keep provider adapters focused on provider-specific payloads instead of repeated plumbing
- share one markdown rendering path for release bodies and release pull requests
- reuse one set of blocking HTTP helpers with consistent error messages

## Best for

- implementing or testing hosted source adapters
- generating release pull request bodies from prepared manifests
- staging, committing, and pushing release branches through shared wrappers

## Public entry points

- `release_body(source, manifest, target)` resolves the outward release body for a target
- `release_pull_request_body(manifest)` renders the provider change-request body
- `release_pull_request_branch(prefix, command)` normalizes the change-request branch name
- `get_json`, `post_json`, `patch_json`, and `put_json` wrap provider API requests
- `git_checkout_branch`, `git_stage_paths`, `git_commit_paths`, and `git_push_branch` wrap shared git operations

<!-- {/monochangeHostingCrateDocs} -->

<!-- {@monochangeNpmCrateDocs} -->

`monochange_npm` discovers npm-family packages and normalizes them for shared planning.

Reach for this crate when you want one adapter for npm, pnpm, and Bun workspaces that emits `monochange_core` package and dependency records.

## Why use it?

- discover several JavaScript package-manager layouts with one crate
- normalize workspace metadata into the same graph used by the rest of `monochange`
- capture dependency edges from `package.json` and `pnpm-workspace.yaml`

## Best for

- scanning JavaScript or TypeScript monorepos into normalized package records
- supporting npm, pnpm, and Bun with one discovery surface
- feeding JS workspace topology into shared planning code

## Public entry points

- `discover_npm_packages(root)` discovers npm, pnpm, and Bun workspaces plus standalone packages
- `NpmAdapter` exposes the shared adapter interface

## Scope

- `package.json` workspaces
- `pnpm-workspace.yaml`
- Bun lockfile detection
- normalized dependency extraction

<!-- {/monochangeNpmCrateDocs} -->

<!-- {@monochangeDenoCrateDocs} -->

`monochange_deno` discovers Deno packages and workspace members for the shared planner.

Reach for this crate when you need to scan `deno.json` or `deno.jsonc` files, expand Deno workspaces, and normalize Deno dependencies into `monochange_core` records.

## Why use it?

- discover Deno workspaces and standalone packages with one adapter
- normalize manifest and dependency data for cross-ecosystem release planning
- include Deno-specific import and dependency extraction in the shared graph

## Best for

- scanning Deno repos without adopting the full workspace CLI
- turning `deno.json` metadata into shared package and dependency records
- mixing Deno packages into a broader cross-ecosystem monorepo plan

## Public entry points

- `discover_deno_packages(root)` discovers Deno workspaces and standalone packages
- `DenoAdapter` exposes the shared adapter interface

## Scope

- `deno.json` and `deno.jsonc`
- workspace glob expansion
- normalized dependency and import extraction

<!-- {/monochangeDenoCrateDocs} -->

<!-- {@monochangeDartCrateDocs} -->

`monochange_dart` discovers Dart and Flutter packages for the shared planner.

Reach for this crate when you need to scan `pubspec.yaml` files, expand Dart or Flutter workspaces, and normalize package metadata into `monochange_core` records.

## Why use it?

- cover both pure Dart and Flutter package layouts with one adapter
- normalize pubspec metadata and dependency edges for shared release planning
- detect Flutter packages without maintaining a separate discovery path

## Best for

- scanning Dart or Flutter monorepos into normalized workspace records
- reusing the same planning pipeline for mobile and non-mobile packages
- discovering Flutter packages without a dedicated Flutter-only adapter layer

## Public entry points

- `discover_dart_packages(root)` discovers Dart and Flutter workspaces plus standalone packages
- `DartAdapter` exposes the shared adapter interface

## Scope

- `pubspec.yaml` workspace expansion
- Dart package parsing
- Flutter package detection
- normalized dependency extraction

<!-- {/monochangeDartCrateDocs} -->

<!-- {@monochangePythonCrateDocs} -->

`monochange_python` discovers Python packages for the shared planner.

Reach for this crate when you need to scan uv workspaces, Poetry projects, and standalone `pyproject.toml` packages, then normalize package metadata and dependency edges into `monochange_core` records.

## Why use it?

- cover uv workspaces, Poetry projects, and standalone PEP 621 packages with one adapter
- normalize Python names and dependency edges for shared release planning
- infer package-manager lockfile refresh commands without directly mutating fragile lockfiles

## Best for

- scanning Python monorepos into normalized workspace records
- adding Python package versions and dependency edges to a mixed-language release plan
- refreshing `uv.lock` or `poetry.lock` through native package-manager commands after manifest updates

## Public entry points

- `discover_python_packages(root)` discovers uv workspace members plus standalone Python packages
- `PythonAdapter` exposes the shared adapter interface

## Scope

- uv workspace member expansion
- `pyproject.toml` parsing for PEP 621 `[project]` and Poetry `[tool.poetry]` metadata
- PEP 503-style dependency name normalization
- PEP 440 version parsing into the shared semantic-version model when possible
- dependency extraction from PEP 621 runtime and optional dependencies
- dependency extraction from Poetry runtime dependencies and dependency groups
- version and internal dependency rewrites for `pyproject.toml`
- lockfile command inference for `uv.lock` and `poetry.lock`

<!-- {/monochangePythonCrateDocs} -->

<!-- {@monochangeSemverCrateDocs} -->

`monochange_semver` merges requested bumps with compatibility evidence.

Reach for this crate when you need deterministic severity calculations for direct changes, propagated dependent changes, or ecosystem-specific compatibility providers.

## Why use it?

- combine manual change requests with provider-generated compatibility assessments
- share one bump-merging strategy across the workspace
- implement custom `CompatibilityProvider` integrations for ecosystem-specific evidence

## Best for

- computing release severities outside the full planner
- plugging ecosystem-specific compatibility logic into shared planning
- reusing the workspace's bump-merging rules in custom tools

## Responsibilities

- collect compatibility assessments from providers
- merge bump severities deterministically
- calculate direct and propagated bump severities
- provide a shared abstraction for ecosystem-specific compatibility providers

## Example

```rust
use monochange_core::BumpSeverity;
use monochange_semver::direct_release_severity;
use monochange_semver::merge_severities;

let merged = merge_severities(BumpSeverity::Patch, BumpSeverity::Minor);
let direct = direct_release_severity(Some(BumpSeverity::Minor), None);

assert_eq!(merged, BumpSeverity::Minor);
assert_eq!(direct, BumpSeverity::Minor);
```

<!-- {/monochangeSemverCrateDocs} -->

<!-- {@monochangeTestHelpersCrateDocs} -->

`monochange_test_helpers` packages the shared fixture, snapshot, git, and RMCP helpers used across the workspace test suite.

Reach for this crate when you are writing integration or fixture-heavy tests that need scenario workspaces, command snapshots, or temporary git repositories.

## Why use it?

- keep tests focused on behavior instead of tempdir and setup boilerplate
- share consistent fixture loading across crates
- reuse snapshot and git helpers in integration suites

## Best for

- copying fixture workspaces into temp directories
- writing git-backed integration tests
- configuring `insta` snapshots and RMCP content assertions

## Public entry points

- `copy_directory` and `copy_directory_skip_git` clone fixture trees into temp workspaces
- `git`, `git_output`, and `git_output_trimmed` run test git commands
- `snapshot_settings()` configures shared snapshot behavior
- `fixture_path!`, `setup_fixture!`, and `setup_scenario_workspace!` locate and materialize test fixtures

<!-- {/monochangeTestHelpersCrateDocs} -->

<!-- {@monochangeTelemtryCrateDocs} -->

`monochange_telemtry` provides local-only telemetry primitives for the `monochange` CLI.

Reach for this crate when you need the reusable event sink, event payloads, and privacy-preserving error classification that power opt-in local JSONL telemetry. The crate intentionally keeps transport simple: it appends OpenTelemetry-style JSON Lines records to a local file and does not send telemetry over the network.

## Why use it?

- keep telemetry capture separate from CLI orchestration and package discovery
- share one local JSONL event schema across command and step instrumentation
- classify errors into low-cardinality categories without exposing raw error text
- make telemetry writes best-effort so observability cannot change command outcomes

## Best for

- embedding monochange's local telemetry sink in the CLI runtime
- smoke-testing event schemas without provisioning a backend
- building future telemetry commands, exporters, or redaction tests on top of a small public API

## Public entry points

- `TelemetrySink::from_env()` resolves `MC_TELEMETRY` and `MC_TELEMETRY_FILE` into either a disabled sink or a local JSONL sink
- `TelemetrySink::capture_command(...)` writes `command_run` events
- `TelemetrySink::capture_step(...)` writes `command_step` events
- `CommandTelemetry`, `StepTelemetry`, and `TelemetryOutcome` describe the stable event payloads

## Privacy boundaries

The crate only accepts low-cardinality command metadata, booleans, counts, durations, enum outcomes, and sanitized `error_kind` values. It does not collect package names, paths, repository URLs, branch names, refs, commit hashes, shell command strings, environment values, changeset text, release notes, issue or pull request IDs, or raw errors.

## Example

```rust
use monochange_telemtry::CommandTelemetry;
use monochange_telemtry::TelemetryOutcome;
use monochange_telemtry::TelemetrySink;
use std::time::Duration;

let sink = TelemetrySink::Disabled;
sink.capture_command(CommandTelemetry {
    command_name: "validate",
    dry_run: false,
    show_diff: false,
    progress_format: "auto",
    step_count: 1,
    duration: Duration::from_millis(42),
    outcome: TelemetryOutcome::Success,
    error: None,
});
```

<!-- {/monochangeTelemtryCrateDocs} -->
