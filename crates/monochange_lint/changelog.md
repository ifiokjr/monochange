## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-26)

### Added

#### feat

Add beautiful interactive progress reporting for `mc check` and `mc lint`.

- Introduced `LintProgressReporter` trait in `monochange_core::lint` with 14 lifecycle hooks from planning through summary.
- Added `NoopLintProgressReporter` for silent / backward-compatible operation.
- Updated `Linter::lint_workspace` to emit planning, suite, file, rule, and summary events to the reporter.
- Created `HumanLintProgressReporter` in `monochange` that writes animated spinners, suite-level progress, fix tracking, and a styled summary to stderr.
- Enhanced `format_check_report` to list which files were fixed when `--fix` is active.
- Respects `NO_COLOR` and `MONOCHANGE_NO_PROGRESS` environment variables.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #281](https://github.com/monochange/monochange/pull/281) _Introduced in:_ [`a9eec58`](https://github.com/monochange/monochange/commit/a9eec586e72d0a8704d08b7552523e0bd85ed20d)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #259](https://github.com/monochange/monochange/pull/259) _Introduced in:_ [`c698d29`](https://github.com/monochange/monochange/commit/c698d29ffabefab418a0a750a06de3b1d6074561) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Added

#### add mc check command and lint rule integration

Add `mc check` CLI command that combines workspace validation with lint rule enforcement. Supports `--fix` for autofix, `--ecosystem` for filtering, and `--format` for output selection. The Validate step also runs lint rules and accepts a `fix` input. Includes the full `monochange_lint` crate with 5 Cargo and 6 NPM lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #207](https://github.com/monochange/monochange/pull/207) _Introduced in:_ [`a650862`](https://github.com/monochange/monochange/commit/a650862f2dc69b6538f6403bab5b66079f9c1304) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add lint include/exclude path filters with optional gitignore override

`[lints]` now supports workspace-level path filters for manifest linting.

**New options:**

- `include` — optional glob patterns that opt manifest paths into linting
- `exclude` — glob patterns that remove matching manifest paths from linting
- `disable_gitignore` — opt back into linting gitignored manifests when needed

By default, monochange now skips gitignored manifests during linting while still allowing repositories to explicitly opt them back in.

The repository config and bundled linting docs now show how to exclude `examples/**` when example manifests would otherwise trigger false positives.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #248](https://github.com/monochange/monochange/pull/248) _Introduced in:_ [`b45cb78`](https://github.com/monochange/monochange/commit/b45cb787a75746a95799f957f01d92020d27f72f) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #228](https://github.com/monochange/monochange/pull/228) _Introduced in:_ [`94f06a0`](https://github.com/monochange/monochange/commit/94f06a057150d26e5f330e2e49a08f71eb12fc92) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/monochange/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/monochange/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/monochange/monochange/issues/230)
