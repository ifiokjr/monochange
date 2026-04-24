# Changelog

All notable changes to `monochange_core` will be documented in this file.

## [0.2.0](https://github.com/ifiokjr/monochange/releases/tag/v0.2.0) (2026-04-21)

### Breaking Change

#### replace changelog_sections with [changelog] section

The `changelog_sections` array-based config and `[release_notes]` section have been replaced with a new top-level `[changelog]` section that separates concerns into types (changeset type → section + bump routing) and sections (display headings with priorities).

**Before** (`[release_notes]` and `changelog_sections`):

```toml
[release_notes]
change_templates = ["- {{ summary }}"]

[package.core]
changelog_sections = [
	{ name = "Security", types = ["security"], bump = "patch" },
]
```

**After** (`[changelog]` with sections and types):

```toml
[changelog]
templates = ["- {{ summary }}"]

[changelog.sections.security]
heading = "Security"
priority = 40

[changelog.types.security]
section = "security"
bump = "patch"

[package.core]
excluded_changelog_types = [] # filter inherited types instead of overriding
```

Key changes:

- `[release_notes]` → `[changelog]` with `templates` instead of `change_templates`
- `changelog_sections` arrays → `[changelog.sections]` (keyed map) + `[changelog.types]` (keyed map)
- Per-package/group overrides → `excluded_changelog_types` (filters workspace defaults instead of complete replacement)
- Type keys must be lowercase, start with a letter, and contain at most one underscore
- Built-in defaults provide 13 sections and 13 types when `[changelog]` is empty
- Bump-severity shorthand types (`minor`, `patch`, `major`, `none`) each get their own title-case section heading (Minor, Patch, Major, None) by default, instead of routing to semantic sections like Added/Fixed

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #259](https://github.com/ifiokjr/monochange/pull/259) _Introduced in:_ [`c698d29`](https://github.com/ifiokjr/monochange/commit/c698d29ffabefab418a0a750a06de3b1d6074561) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Added

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/ifiokjr/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/ifiokjr/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/ifiokjr/monochange/issues/208)

#### add MCP tools for change analysis and validation

Introduces two new MCP tools to help agents generate and validate changesets programmatically:

**`monochange_analyze_changes`**

Analyzes git diffs and suggests granular changeset structure. Supports multiple detection modes:

```json
{
	"path": "/path/to/repo",
	"frame": "main...feature-branch",
	"detection_level": "signature",
	"max_suggestions": 10
}
```

The tool automatically detects:

- Which packages have changes
- What type of artifact each package is (library, app, CLI)
- Semantic changes (new functions, modified components, etc.)
- Appropriate grouping based on configurable thresholds

**`monochange_validate_changeset`**

Validates that a changeset accurately describes the actual code changes:

```json
{
	"path": "/path/to/repo",
	"changeset_path": ".changeset/feature.md"
}
```

Checks:

- Does the summary match the actual diff content?
- Is the bump level appropriate for the change type?
- Are there undocumented API changes?

**Before:**

Agents had to manually inspect diffs and decide what changesets to create.

**After:**

```bash
# Start MCP server
mc mcp

# Then use the analyze_changes tool to get suggestions
# for all packages with modifications
```

These tools integrate with the new `monochange_analysis` crate to provide intelligent, context-aware changeset recommendations.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #206](https://github.com/ifiokjr/monochange/pull/206) _Introduced in:_ [`a417022`](https://github.com/ifiokjr/monochange/commit/a417022f80f93d61add00b8087e0f80102a9fd52) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add per-crate Codecov coverage flags and crate-specific coverage badges

monochange now uploads one Codecov coverage flag per public crate while keeping the existing workspace-wide upload.

**Before:**

- Codecov only received the overall workspace LCOV upload
- crate READMEs linked their coverage badge to the shared repository-wide Codecov page
- Codecov patch coverage enforced a 100% target for PR status checks

**After:**

- CI splits the workspace LCOV report into one upload per public crate using a Codecov flag named after the crate
- each published crate README now points its coverage badge at that crate’s own Codecov flag page, for example `?flag=monochange_core`
- the repository keeps the overall workspace coverage upload and lowers the Codecov patch coverage status target to 95%

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #255](https://github.com/ifiokjr/monochange/pull/255) _Introduced in:_ [`26e13ff`](https://github.com/ifiokjr/monochange/commit/26e13fff071e93dc32fe071a5771232c980ebd46) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add ecosystem-specific semantic analysis for MCP changeset workflows

`monochange_analyze_changes` and `monochange_validate_changeset` now return real semantic analysis for Cargo, npm, Deno, and Dart/Flutter packages instead of placeholder results.

**Before (`monochange_analyze_changes`):**

```json
{
	"ok": true,
	"summary": "Analysis complete - review suggested changesets for each package",
	"analysis": {
		"package_changes": {}
	}
}
```

**After (`monochange_analyze_changes`):**

```json
{
	"ok": true,
	"summary": "Analyzed 1 package(s) and found 6 semantic change(s)",
	"analysis": {
		"packageAnalyses": {
			"core": {
				"semanticChanges": [
					{
						"category": "public_api",
						"kind": "modified",
						"itemPath": "greet"
					},
					{
						"category": "dependency",
						"kind": "added",
						"itemPath": "tracing"
					}
				]
			}
		}
	}
}
```

`monochange_validate_changeset` now validates authored changesets against that semantic diff and can flag stale symbol references or underspecified summaries across all supported ecosystems.

Current analyzer coverage includes:

- Cargo public Rust API diffs plus `Cargo.toml` dependency and manifest metadata changes
- npm-family JS/TS exported symbol diffs plus `package.json` exports, commands, dependency, and script changes
- Deno JS/TS exported symbol diffs plus `deno.json` exports, import aliases, task, and compiler-option changes
- Dart and Flutter public `lib/` API diffs plus `pubspec.yaml` executables, dependency, environment, and plugin-platform changes

**Before (`monochange_validate_changeset`):**

```json
{
	"ok": true,
	"valid": true,
	"issues": []
}
```

**After (`monochange_validate_changeset`):**

```json
{
	"ok": false,
	"valid": false,
	"lifecycle_status": "stale",
	"issues": [
		{
			"severity": "error",
			"message": "changeset references `OldGreeter` but that item was not found in the current semantic diff"
		}
	]
}
```

`monochange_core` now exposes shared semantic-analysis contracts and diff record types so ecosystem crates can own their analyzers without moving parser logic into the CLI crate.

`@monochange/skill` now documents the semantic-analysis-backed MCP workflows and the expanded cross-ecosystem validation guidance for assistant consumers.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #247](https://github.com/ifiokjr/monochange/pull/247) _Introduced in:_ [`8c96c8f`](https://github.com/ifiokjr/monochange/commit/8c96c8f0a3b9d44bf30148b5a83067d7ce3ab62b) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#243](https://github.com/ifiokjr/monochange/issues/243) _Related issues:_ [#244](https://github.com/ifiokjr/monochange/issues/244)

#### add core linting types

Add `monochange_core::lint` module with the foundational types for the linting system:

- `LintSeverity` (Off, Warning, Error) — rule severity levels
- `LintCategory` (Style, Correctness, Performance, Suspicious, BestPractice) — rule classification
- `LintRule` — rule definition with id, name, description, and autofixable flag
- `LintResult`, `LintLocation` — individual findings with file location and byte spans
- `LintFix`, `LintEdit` — autofix suggestions with span-based replacements
- `LintRuleConfig` — flexible configuration supporting simple severity or detailed options
- `LintReport` — aggregated results with error/warning counts
- `LintContext` — rule input with workspace root, manifest path, and file contents
- `LintRuleRunner` trait — executable rule interface with `rule()`, `applies_to()`, and `run()`
- `LintRuleRegistry` — rule registration and discovery

Also adds `lints` field to `EcosystemSettings` for per-ecosystem lint configuration and `Lint` variant to `CliStepDefinition` with `format`, `fix`, and `ecosystem` inputs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #207](https://github.com/ifiokjr/monochange/pull/207) _Introduced in:_ [`a650862`](https://github.com/ifiokjr/monochange/commit/a650862f2dc69b6538f6403bab5b66079f9c1304) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add lint include/exclude path filters with optional gitignore override

`[lints]` now supports workspace-level path filters for manifest linting.

**New options:**

- `include` — optional glob patterns that opt manifest paths into linting
- `exclude` — glob patterns that remove matching manifest paths from linting
- `disable_gitignore` — opt back into linting gitignored manifests when needed

By default, monochange now skips gitignored manifests during linting while still allowing repositories to explicitly opt them back in.

The repository config and bundled linting docs now show how to exclude `examples/**` when example manifests would otherwise trigger false positives.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #248](https://github.com/ifiokjr/monochange/pull/248) _Introduced in:_ [`b45cb78`](https://github.com/ifiokjr/monochange/commit/b45cb787a75746a95799f957f01d92020d27f72f) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add package publication targets and publish step definitions

`monochange_core` now models package publication as first-class release metadata instead of leaving registry publication outside the release graph.

**Before:**

```rust
use monochange_core::CliStepDefinition;
use monochange_core::ReleaseManifest;

let step = CliStepDefinition::PublishRelease { /* ... */ };

let manifest = ReleaseManifest {
    release_targets: vec![],
    released_packages: vec![],
    changed_files: vec![],
    // no package publication metadata
    ..todo!()
};
```

**After:**

```rust
use monochange_core::CliStepDefinition;
use monochange_core::PackagePublicationTarget;
use monochange_core::PublishMode;
use monochange_core::RegistryKind;
use monochange_core::ReleaseManifest;

let step = CliStepDefinition::PublishPackages { /* ... */ };

let manifest = ReleaseManifest {
    package_publications: vec![PackagePublicationTarget {
        package: "core".to_string(),
        ecosystem: monochange_core::Ecosystem::Cargo,
        registry: Some(monochange_core::PublishRegistry::Builtin(
            RegistryKind::CratesIo,
        )),
        version: "1.2.3".to_string(),
        mode: PublishMode::Builtin,
        trusted_publishing: Default::default(),
    }],
    ..todo!()
};
```

New public types include:

- `PublishMode`
- `RegistryKind`
- `PublishRegistry`
- `PlaceholderSettings`
- `TrustedPublishingSettings`
- `PublishSettings`
- `PackagePublicationTarget`

`PackageDefinition`, `EcosystemSettings`, `ReleaseManifest`, and `ReleaseRecord` now all carry publish metadata, and the built-in CLI command set includes `placeholder-publish` and `publish` step definitions.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/ifiokjr/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/ifiokjr/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add a dedicated `mc versions` command

monochange now ships a dedicated `mc versions` command and `DisplayVersions` CLI step for rendering planned package and group versions without the rest of the release preview.

**Before:**

```bash
mc release --dry-run --format text
```

Rendered the full release summary, including release targets, changed files, and other follow-up details.

**After:**

```bash
mc versions --format text
mc versions --format markdown
mc versions --format json
```

This trims the output down to package and group version summaries only, without mutating manifests, changelogs, or consuming changesets.

`monochange_config` also now includes the built-in `versions` command in the default CLI command set returned from workspace configuration loading, so embedded callers and config-driven integrations see the same built-in command list as the `mc` binary.

You can also expose the same behavior from custom commands with `DisplayVersions`:

```toml
[cli.versions]
help_text = "Display planned package and group versions"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "text" },
]
steps = [{ type = "DisplayVersions" }]
```

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #229](https://github.com/ifiokjr/monochange/pull/229) _Introduced in:_ [`f148184`](https://github.com/ifiokjr/monochange/commit/f148184d69fc4dc8720cde8db22768a8c1def8f7) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add publish rate-limit planning and batched publish metadata

monochange can now plan package-registry publish work before mutating registries. The new default `mc publish-plan` command renders built-in rate-limit metadata, recommended publish batches, evidence links, and optional CI snippets for GitHub Actions or GitLab CI. `mc publish` and `mc placeholder-publish` now also accept repeated `--package` filters so the planned batches can be executed one window at a time.

**Before:**

```bash
# no dedicated planning command
mc publish --dry-run
mc placeholder-publish --dry-run
```

Built-in publish flows had no registry-window report and no first-class batch plan to hand off to CI.

**After:**

```bash
mc publish-plan --format json
mc publish-plan --mode placeholder --ci github-actions
mc publish --dry-run --package core --package web --format json
mc placeholder-publish --package core --format json
```

`mc publish-plan` now reports:

- per-registry publish windows
- `batches` with explicit package ids per window
- evidence URLs plus confidence levels
- optional CI snippets that expand to `mc publish --package ...` invocations

Representative JSON now includes a publish-rate-limit section alongside publish output:

```json
{
	"packagePublish": { "...": "..." },
	"publishRateLimits": {
		"windows": [
			{
				"registry": "pub_dev",
				"pending": 13,
				"batchesRequired": 2,
				"fitsSingleWindow": false
			}
		],
		"batches": [
			{
				"registry": "pub_dev",
				"batchIndex": 1,
				"totalBatches": 2,
				"packages": ["core", "web"]
			}
		]
	}
}
```

Built-in catalog coverage now includes `crates.io`, `npm`, `jsr`, and `pub.dev`, with confidence and evidence attached to each policy.

#### add `publish.rate_limits.enforce` to workspace configuration

`monochange_config` and `monochange_core` now model per-ecosystem and per-package publish rate-limit enforcement so teams can decide whether planned overages should warn or block.

**Before (`monochange.toml`):**

```toml
[ecosystems.dart.publish]
mode = "builtin"
```

**After:**

```toml
[ecosystems.dart.publish]
mode = "builtin"

[ecosystems.dart.publish.rate_limits]
enforce = true
```

When `enforce = true`, built-in publish commands stop before running a package set that requires more than one known registry window. This lets CI fail early and lets teams split the work into planned follow-up batches instead of discovering throttling halfway through a release.

#### extend public publish planning types in `monochange_core`

`monochange_core` now exposes rate-limit settings and batch metadata for callers that build their own CLI or automation around monochange release plans.

**Before (`monochange_core`):**

```rust
pub struct PublishSettings {
	pub enabled: bool,
	pub mode: PublishMode,
	pub registry: Option<PublishRegistry>,
	pub trusted_publishing: TrustedPublishingSettings,
	pub placeholder: PlaceholderSettings,
}

pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub warnings: Vec<String>,
}
```

**After:**

```rust
pub struct PublishSettings {
	pub enabled: bool,
	pub mode: PublishMode,
	pub registry: Option<PublishRegistry>,
	pub trusted_publishing: TrustedPublishingSettings,
	pub rate_limits: PublishRateLimitSettings,
	pub placeholder: PlaceholderSettings,
}

pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub batches: Vec<PublishRateLimitBatch>,
	pub warnings: Vec<String>,
}
```

This keeps config parsing, runtime enforcement, dry-run JSON, and library consumers aligned around the same publish-batch model.

#### fix standalone npm package discovery ids in `monochange_npm`

`monochange_npm` now normalizes standalone package ids relative to the discovery root so repositories that contain multiple standalone `package.json` files no longer collapse distinct packages into one record.

**Before:**

- two standalone npm packages under different directories could share the same internal `npm:package.json` id
- the adapter could keep only one package depending on filesystem traversal order

**After:**

- standalone npm packages get stable root-relative ids such as `npm:packages/docs/package.json`
- publish planning and other discovery-driven flows now keep both packages consistently across platforms

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #225](https://github.com/ifiokjr/monochange/pull/225) _Introduced in:_ [`98013cb`](https://github.com/ifiokjr/monochange/commit/98013cb86d5644a7327dc2ee5803d747d4a0372c) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### move crate include lists into published manifests

The published library crates in this workspace now declare their `include` file lists in each crate's own `Cargo.toml` instead of inheriting that setting from `[workspace.package]`.

**Before (`crates/monochange_core/Cargo.toml`):**

```toml
[package]
include = { workspace = true }
readme = "readme.md"
```

The package contents depended on the root workspace manifest carrying:

```toml
[workspace.package]
include = ["src/**/*.rs", "Cargo.toml", "readme.md"]
```

**After:**

```toml
[package]
include = ["src/**/*.rs", "Cargo.toml", "readme.md"]
readme = "readme.md"
```

This keeps each published crate self-contained when packaging, auditing, or updating manifest metadata and avoids relying on a shared workspace-level `include` definition for crates.io package contents.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #227](https://github.com/ifiokjr/monochange/pull/227) _Introduced in:_ [`78af3c2`](https://github.com/ifiokjr/monochange/commit/78af3c244a4090965b455e2879b33a160e28da77) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### centralize manifest lint configuration and split lint suites by ecosystem

monochange now configures manifest linting from a top-level `[lints]` section instead of per-ecosystem `[ecosystems.<name>.lints]` tables, and the runtime engine now loads lint suites from ecosystem crates instead of hard-coding Cargo and npm behavior in `monochange_lint`.

**Before (`monochange.toml`):**

```toml
[ecosystems.cargo.lints]
"cargo/internal-dependency-workspace" = "error"

[ecosystems.npm.lints]
"npm/workspace-protocol" = "error"
```

**After:**

```toml
[lints]
use = ["cargo/recommended", "npm/recommended"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

**New CLI surface:**

```bash
# before
mc check --ecosystem cargo --format json

# after
mc check --ecosystem cargo --only cargo/internal-dependency-workspace --format json
mc lint list
mc lint explain cargo/internal-dependency-workspace
mc lint new cargo/no-path-dependencies
```

**Library-facing changes:**

```rust
// before
let configuration = load_workspace_configuration(root)?;
let cargo_rules = configuration.cargo.lints.clone();

// after
let configuration = load_workspace_configuration(root)?;
let lint_settings = configuration.lints.clone();
```

```rust
// new shared contracts in monochange_core::lint
pub trait LintSuite: Send + Sync {
	fn suite_id(&self) -> &'static str;
	fn rules(&self) -> Vec<Box<dyn LintRuleRunner>>;
	fn presets(&self) -> Vec<LintPreset>;
	fn collect_targets(
		&self,
		workspace_root: &Path,
		configuration: &WorkspaceConfiguration,
	) -> MonochangeResult<Vec<LintTarget>>;
}
```

This release also adds two new support crates:

- `monochange_linting` — authoring helpers for lint-rule metadata and suite construction
- `monochange_lint_testing` — snapshot-friendly helpers for lint reports and autofix output

The Cargo and npm suites now live in `monochange_cargo::lints` and `monochange_npm::lints`, so ecosystem-specific parsing and rule behavior stay with their ecosystem adapters.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #228](https://github.com/ifiokjr/monochange/pull/228) _Introduced in:_ [`94f06a0`](https://github.com/ifiokjr/monochange/commit/94f06a057150d26e5f330e2e49a08f71eb12fc92) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Refactor

#### align crate docs and readability with the workspace style guide

This pass improves readability and documentation consistency across the workspace without changing release behavior or public APIs.

**What changed:**

- extracted shared crate-level docs into `.templates/crates.t.md` and reused them from Rust `lib.rs` module docs and crate readmes
- added missing readmes and module docs for `monochange_analysis`, `monochange_hosting`, and `monochange_test_helpers`
- rewrote a few nested control-flow sections into flatter early-return or `match`-based forms in `monochange`, `monochange_config`, `monochange_gitea`, `monochange_gitlab`, `monochange_npm`, and the shared test helpers
- replaced duplicated fixture-copy helpers in `monochange_cargo` and `monochange_core` tests with the shared `monochange_test_helpers::copy_directory` utility

**Before:**

```rust
if let Some(existing_pr) = &existing {
    if content_matches {
        // skipped response
    } else {
        // update response
    }
} else {
    // create response
}
```

**After:**

```rust
match existing {
    Some(existing_pr) if content_matches => {
        // skipped response
    }
    Some(existing_pr) => {
        // update response
    }
    None => {
        // create response
    }
}
```

The result is more consistent crate documentation, less duplicated prose, and flatter control flow in a few high-traffic code paths.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/ifiokjr/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/ifiokjr/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/ifiokjr/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/ifiokjr/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/ifiokjr/monochange/issues/230)

## [0.1.0](https://github.com/ifiokjr/monochange/releases/tag/v0.1.0) (2026-04-13)

### Breaking changes

#### 🚀 Initial public release of monochange

**monochange** is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems. It is designed from the ground up to support the modern, AI-driven development landscape where agents and automation play a central role in software delivery.

##### What is monochange?

In today's agent-driven development environment, managing releases across diverse package ecosystems (Rust, JavaScript/TypeScript, Dart, Python, etc.) becomes increasingly complex. monochange provides a unified, programmatic interface for:

- **Change tracking**: Structured changesets that capture intent across multiple packages
- **Release planning**: Automated versioning and changelog generation
- **Multi-ecosystem support**: Native handling of Cargo, NPM, Dart, Deno, and more
- **CI/CD integration**: Seamless workflows for Gitea, GitHub, and GitLab
- **Graph-based dependency analysis**: Understanding package relationships across your monorepo

##### Why monochange matters for AI-driven workflows

As development teams increasingly rely on AI agents to generate code, manage dependencies, and orchestrate releases, monochange provides the structured foundation these agents need to operate effectively. It transforms release management from a manual, error-prone process into a deterministic, automatable workflow.

##### What's included in this release

This first release includes:

- Core changeset management engine
- Multi-ecosystem package detection and versioning
- Hosting provider integrations (Gitea, GitHub, GitLab)
- Semantic versioning utilities
- Configurable release workflows
- CLI tooling for validation and release orchestration

For complete feature details, architecture overview, and usage examples, see the [documentation](https://docs.rs/monochange).

> _Owner:_ Ifiok Jr. _Introduced in:_ [`4542b5a`](https://github.com/ifiokjr/monochange/commit/4542b5aee8b63a86c7ffc0ea9436090162a18056)
