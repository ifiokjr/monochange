# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-30)

### Added

#### Improve lint and check progress output

Add beautiful interactive progress reporting for `mc check` and `mc lint`.

- Introduced `LintProgressReporter` trait in `monochange_core::lint` with 14 lifecycle hooks from planning through summary.
- Added `NoopLintProgressReporter` for silent / backward-compatible operation.
- Updated `Linter::lint_workspace` to emit planning, suite, file, rule, and summary events to the reporter.
- Created `HumanLintProgressReporter` in `monochange` that writes animated spinners, suite-level progress, fix tracking, and a styled summary to stderr.
- Enhanced `format_check_report` to list which files were fixed when `--fix` is active.
- Respects `NO_COLOR` and `MONOCHANGE_NO_PROGRESS` environment variables.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #281](https://github.com/monochange/monochange/pull/281) _Introduced in:_ [`a9eec58`](https://github.com/monochange/monochange/commit/a9eec586e72d0a8704d08b7552523e0bd85ed20d) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Configure changeset lint rules

Add configurable changeset lint rules under `[lints.rules]` for summaries, section headings, bump-specific requirements, and changelog-type-specific requirements.

Rules can target built-in or custom changeset types with dynamic ids like `changesets/types/breaking` and `changesets/types/unicorns`, while unknown type ids are rejected during configuration loading.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #326](https://github.com/monochange/monochange/pull/326) _Introduced in:_ [`285fd69`](https://github.com/monochange/monochange/commit/285fd697b99502e9d95716413c533251307f0010) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

### Changed

#### add attestation policy configuration

Add first-class package and release attestation policy settings. Package publish settings now support `publish.attestations.require_registry_provenance`, inherited from ecosystem publish settings and overridable per package.

When registry provenance is required, built-in release publishing fails before invoking registry commands unless trusted publishing is enabled, the current CI/OIDC identity is verifiable, the provider/registry capability matrix reports registry-native provenance support, and the built-in publisher can require that provenance. npm release publishes add `--provenance` only when this policy is enabled.

GitHub release asset attestation intent is modeled under `[source.releases.attestations]` with `require_github_artifact_attestations`, which is accepted only for GitHub sources.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #339](https://github.com/monochange/monochange/pull/339) _Introduced in:_ [`2849f7b`](https://github.com/monochange/monochange/commit/2849f7b771c24b2e3459450661d24c7f36c91a9c) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#315](https://github.com/monochange/monochange/issues/315)

#### Add cargo-binstall metadata

Add cargo-binstall metadata so `cargo binstall monochange` can resolve the GitHub release archive layout.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #293](https://github.com/monochange/monochange/pull/293) _Introduced in:_ [`497f8c0`](https://github.com/monochange/monochange/commit/497f8c010a534fcac6e3ed26bb21c220c54e7a5e) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add initial changelog headers

Changelog targets now support `initial_header` in `[defaults.changelog]`, `[package.<id>.changelog]`, and `[group.<id>.changelog]`. monochange renders the header only when creating a changelog from empty content, preserving existing preambles on later releases.

When no custom header is configured, the selected changelog format provides a built-in default header.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #344](https://github.com/monochange/monochange/pull/344) _Introduced in:_ [`ad6c84b`](https://github.com/monochange/monochange/commit/ad6c84b8de13a5849b262a4db2e2ac87710944f2)

#### Add changelog section thresholds

`monochange` changelog rendering can now hide or collapse sections based on each section's configured priority. This lets you keep high-signal sections expanded while moving low-priority notes into collapsible markdown blocks or omitting them entirely.

Add the new workspace setting under `[changelog.section_thresholds]`:

```toml
[changelog.section_thresholds]
collapse = 50
ignored = 100
```

With that configuration:

- sections with `priority < 50` stay fully expanded
- sections with `priority >= 50` render inside markdown `<details>` blocks
- sections with `priority > 100` are omitted from the rendered changelog

**Before:** every configured `changelog.sections` entry rendered normally once it had entries.

```toml
[changelog.sections]
feat = { heading = "Added", priority = 20 }
docs = { heading = "Documentation", priority = 40 }
other = { heading = "Other", priority = 50 }
```

```md
## 1.2.3

### Added

- ship a new release workflow

### Other

- internal cleanup note
```

**After:** lower-priority sections can collapse automatically.

```toml
[changelog.sections]
feat = { heading = "Added", priority = 20 }
docs = { heading = "Documentation", priority = 40 }
other = { heading = "Other", priority = 50 }

[changelog.section_thresholds]
collapse = 50
ignored = 100
```

```md
## 1.2.3

### Added

- ship a new release workflow

<details>
<summary><strong>Other</strong></summary>

- internal cleanup note

</details>
```

This release also updates the generated init config and workspace config annotations so the new thresholds are documented where `monochange.toml` is authored.

> **Breaking change for Rust library consumers** — `monochange_core::ReleaseNotesSection` and `monochange_core::ChangelogSettings` now carry the new changelog-threshold metadata, so manual struct literals must include the added fields.

**Before (`monochange_core`):**

```rust
ReleaseNotesSection {
    title: "Documentation".to_string(),
    entries: vec!["- update migration guide".to_string()],
}

ChangelogSettings {
    templates,
    sections,
    types,
}
```

**After:**

```rust
ReleaseNotesSection {
    title: "Documentation".to_string(),
    collapsed: true,
    entries: vec!["- update migration guide".to_string()],
}

ChangelogSettings {
    templates,
    sections,
    section_thresholds,
    types,
}
```

> _Owner:_ Ifiok Jr. _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`4426b99`](https://github.com/monochange/monochange/commit/4426b9916791ceff82957f61837be1e681988c9a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Refactor changeset linting into LintSuite architecture

Move changeset linting out of workspace config loading and into the `LintSuite`/`LintRuleRunner` framework alongside cargo, npm, and dart linting. This ensures `mc check` and `mc validate` surface changeset lint errors consistently.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add post-merge release automation

- Add `release-pr-manual-merge-blocker` job to CI that fails on PRs from `monochange/release/*` branches, forcing the `/merge` slash-command workflow
- Protect the `release-pr` job with `environment: publisher` so branch-protection rules apply
- Introduce a `release-post-merge` job that runs `PublishRelease` and `CommentReleasedIssues` steps after a release PR merges
- Add `from-ref` support to `PublishRelease` and `CommentReleasedIssues` for discovering the release record from the merge commit when `prepared_release` context is unavailable
- Add `auto-close-issues` flag to `CommentReleasedIssues` that closes released issues not already closed by a PR reference
- Store `changesets` in `ReleaseRecord` so post-merge steps can resolve related issues without access to the deleted changeset files
- Update `plan_released_issue_comments` to include all issue relationships and set `close` state appropriately
- Update `comment_released_issues_with_client` to PATCH issue state to `"closed"` when `plan.close` is `true`
- Add dedicated composite actions: `publish-release` and `comment-released-issues`
- Add `publish-release` and `comment-released-issues` CLI step definitions to `monochange.init.toml`

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #282](https://github.com/monochange/monochange/pull/282) _Introduced in:_ [`014491d`](https://github.com/monochange/monochange/commit/014491ddb0de1a562bd0ca6552bba9646baf7f42) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Fix CLI help colors

Fix `--help` (`-h`) color output and unify CLI color palette.

- `mc --help` now emits ANSI colors in terminal emulators, matching `mc help <command>` behavior
- Extract shared `cli_theme` module so clap built-in help and custom `mc help` renderer use identical colors:
  - bright cyan for headers and accents
  - bright white for usage
  - bright yellow for flags and literals
  - bright magenta for placeholders
  - bright green for valid/code snippets
  - bright red for errors
  - bright black (gray) for muted text
- Explicitly opt in to `ColorChoice::Auto` on the `Command` builder
- Preserve plain text output in test and CI modes so existing snapshots stay stable

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #267](https://github.com/monochange/monochange/pull/267) _Introduced in:_ [`370d5a1`](https://github.com/monochange/monochange/commit/370d5a1d4655c14cf4340cec7886ddc8aa7bbd51) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Group CLI help commands consistently

Make `mc -h`, `mc --help`, and `mc help` render the same command overview so users see consistent help no matter which entry point they use.

The overview now separates built-in commands, generated `step:*` commands, and user-defined `monochange.toml` commands. Generated step commands are always listed, and detailed command help includes richer descriptions for step commands such as `step:publish-release`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #348](https://github.com/monochange/monochange/pull/348) _Introduced in:_ [`33e82e4`](https://github.com/monochange/monochange/commit/33e82e4df24e7c0a36af70f7a397bbadbf5ff9dd)

#### Add colored CLI help

Add beautiful colored CLI help with detailed examples

The `mc help <command>` subcommand now renders detailed, formatted help with bordered headers, colored sections, multiple examples per command, tips, and cross-references. Running `mc help` shows an overview listing all commands. The standard `--help` flags also use ANSI colors via an anstyle theme. All colors respect NO_COLOR and TTY detection.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #265](https://github.com/monochange/monochange/pull/265) _Introduced in:_ [`8890d77`](https://github.com/monochange/monochange/commit/8890d77e8d54f81f8807588192441a3cd46bfbb8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Expose resolved configuration in CLI templates

Configured CLI workflows can now reference the resolved workspace configuration and paths without adding a dedicated setup step. Templates receive `config`, `project_root`, and `config_path` by default, so later steps can read values such as `{{ config.packages.0.id }}` while regular workflow execution stays quiet unless a step intentionally emits output.

The new built-in config step also makes the generated command available for CI and debugging:

```bash
mc step:config
```

It prints JSON containing the canonical project root, the `monochange.toml` path, and the resolved configuration:

```json
{
	"projectRoot": "/workspace/repo",
	"configPath": "/workspace/repo/monochange.toml",
	"config": {
		"packages": []
	}
}
```

This gives scripts and GitHub Actions a stable command for inspecting the exact configuration that `monochange` loaded while preserving the no-output default for configured workflow steps.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #343](https://github.com/monochange/monochange/pull/343) _Introduced in:_ [`fc627b3`](https://github.com/monochange/monochange/commit/fc627b38392bc32dc9402a64a5bcc95572a94c3c)

#### Document ecosystem-level trusted publishing inheritance

Trusted publishing stays an ecosystem-level publish setting that packages inherit by default:

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[package.legacy.publish]
trusted_publishing = false
```

Use `[ecosystems.<name>.publish.trusted_publishing]` for shared repository, workflow, and environment metadata. Package-level publish settings override the ecosystem defaults for package-specific workflows or opt-outs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #336](https://github.com/monochange/monochange/pull/336) _Introduced in:_ [`263a8d0`](https://github.com/monochange/monochange/commit/263a8d0bc0f61029392cbf45733d0a2fb24b8773) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Sync documented workflow commands with generated config

Fix the generated `mc init` configuration so it no longer defines the reserved `[cli.validate]` command, restores the documented provider `release-pr` workflow command, and syncs the repository workflow examples with the config-defined commands documented in the README and guides.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #345](https://github.com/monochange/monochange/pull/345) _Introduced in:_ [`13e594b`](https://github.com/monochange/monochange/commit/13e594b62c751b3d6f2779446314d6d283c7e35b)

#### Document supported ecosystem capabilities

The documentation now includes a dedicated ecosystem guide that compares Cargo, npm-family, Deno, Dart / Flutter, and Python support across discovery, manifest updates, lockfile handling, and built-in registry publishing. Python is documented as a supported release-planning ecosystem with uv workspace discovery, Poetry and PEP 621 `pyproject.toml` parsing, Python dependency normalization, manifest version rewrites, internal dependency rewrites, and inferred `uv lock` / `poetry lock --no-update` lockfile commands.

The guide also clarifies ecosystem publishing boundaries, including canonical public registry support and the external-mode escape hatch for private registries or custom publication flows.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #307](https://github.com/monochange/monochange/pull/307) _Introduced in:_ [`11c628c`](https://github.com/monochange/monochange/commit/11c628cd2afb7c9509c31a8cfc043be63a9f2a75) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add Go ecosystem support

monochange now discovers and manages Go modules from `go.mod` files in single-module and multi-module repositories.

**Configuration:**

```toml
[defaults]
package_type = "go"

[package.api]
path = "api"

[package.shared]
path = "shared"

[ecosystems.go]
enabled = true
```

**What it discovers:**

- Go modules by scanning for `go.mod` files
- Multi-module monorepos with separate modules in subdirectories
- Module paths, including major version suffixes (`/v2`, `/v3`)
- Cross-module `require` directives as dependency edges
- Indirect dependencies marked as development dependencies

**Version management:**

- Go versions come from git tags, not manifest files — the adapter reports `None` for `current_version` and stores the module path as metadata for tag resolution
- Updates `require` directives in `go.mod` when cross-module dependencies change
- Preserves `replace`, `exclude`, `retract` directives and comments
- Adds `v` prefix to version strings automatically when missing

**Lockfile commands:**

- Infers `go mod tidy` for all Go modules (updates both `go.mod` and `go.sum`)
- Configurable via `[ecosystems.go].lockfile_commands`

**Key design decisions:**

- Module names are derived from the last non-version segment of the module path (`github.com/org/repo/api/v2` → `api`)
- The full module path and relative directory path are stored as metadata for downstream tag resolution
- Parse errors during discovery are treated as warnings, not hard errors

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #156](https://github.com/monochange/monochange/pull/156) _Introduced in:_ [`519f841`](https://github.com/monochange/monochange/commit/519f841929c6a06d5b3a578b206982d2d6cc1548) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#133](https://github.com/monochange/monochange/issues/133)

#### Add Python ecosystem support

monochange now discovers and manages Python packages from uv workspaces, Poetry projects, and standalone `pyproject.toml` files.

**Configuration:**

```toml
[defaults]
package_type = "python"

[package.core]
path = "packages/core"

[ecosystems.python]
enabled = true
lockfile_commands = [{ command = "uv lock" }]
```

**What it discovers:**

- uv workspaces via `[tool.uv.workspace].members` glob patterns
- Poetry projects via `[tool.poetry]` sections
- Standalone `pyproject.toml` files with PEP 621 `[project]` metadata

**Version management:**

- Reads and updates `[project].version` in `pyproject.toml`
- Parses PEP 440 versions and maps to semver (e.g., `1.2` → `1.2.0`)
- Updates dependency version constraints in `[project].dependencies`
- Handles `dynamic = ["version"]` gracefully (reports `None` for dynamic versions)

**Lockfile commands:**

- Infers `uv lock` for uv projects (detected by `uv.lock`)
- Infers `poetry lock --no-update` for Poetry projects (detected by `poetry.lock`)
- Configurable via `[ecosystems.python].lockfile_commands`

**Dependency extraction:**

- PEP 621 `[project].dependencies` and `[project.optional-dependencies]`
- Poetry `[tool.poetry.dependencies]` and `[tool.poetry.group.*.dependencies]`
- PEP 503 name normalization for cross-package dependency matching
- PEP 508 version specifier parsing with extras support

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #152](https://github.com/monochange/monochange/pull/152) _Introduced in:_ [`81b0882`](https://github.com/monochange/monochange/commit/81b0882525ab51d74b0e8cc2a0114aac0fdb3a7f) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#132](https://github.com/monochange/monochange/issues/132)

#### Add PyPI publishing support

Add first-class PyPI publishing support for Python packages, including PyPI as the built-in Python registry, placeholder package generation, `uv build` / `uv publish` command generation, PyPI JSON API publication checks, trusted-publisher setup guidance, and rate-limit planning coverage.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #322](https://github.com/monochange/monochange/pull/322) _Introduced in:_ [`afe5959`](https://github.com/monochange/monochange/commit/afe595944146c3ec4c9798098c92c031d4279e6a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Fix binary benchmark changeset fixtures

Update generated binary benchmark changesets to include summary headings so the PR benchmark fixtures pass changeset validation.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Handle large release commit messages reliably

Release commit creation now streams the generated commit message through standard input instead of passing the full release record as a command-line argument. This avoids operating-system argument length limits for large release records.

Git command spawning now reuses a stable path that contains `git`, and benchmark fixture git commands now use monochange's sanitized git command helper with automatic garbage collection disabled for deterministic synthetic history setup in CI.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #320](https://github.com/monochange/monochange/pull/320) _Introduced in:_ [`f41f985`](https://github.com/monochange/monochange/commit/f41f985288e1440b3b64c2fa9c1cda987925ef8a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Fix release merge blocker workflow

Replace the release PR merge blocker action with an inline shell guard so normal pull requests are not blocked by missing action dependencies.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Update repository URLs

Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Default CLI output to markdown

Default output format to markdown with termimad terminal rendering.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #263](https://github.com/monochange/monochange/pull/263) _Introduced in:_ [`020df1f`](https://github.com/monochange/monochange/commit/020df1f2d1bec0d8470fe1f4e734ee31e3e167bf) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Publish GitHub releases through drafts

- Add a boolean `draft` input to the built-in `PublishRelease` step so CLI commands can create hosted releases as drafts while preserving `[source.releases].draft` defaults.
- Update release automation to create draft GitHub releases, run the asset upload workflow against those drafts, then publish the drafts after assets are attached.
- Add a global `--jq` filter for JSON-producing commands so automation can extract release tags and other fields directly from `--format json` output.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #325](https://github.com/monochange/monochange/pull/325) _Introduced in:_ [`bee06fe`](https://github.com/monochange/monochange/commit/bee06fed90e50cda3de695b973f415dd162eec29) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add local-only telemetry events

Users can now opt in to local-only telemetry for CLI support and debugging without sending data over the network. By default, nothing is recorded. Setting `MC_TELEMETRY=local` writes OpenTelemetry-style JSON Lines events to the user state directory, while `MC_TELEMETRY_FILE=/path/to/telemetry.jsonl` writes to a chosen file.

Command:

```bash
MC_TELEMETRY=local mc validate
MC_TELEMETRY_FILE=/tmp/mc-telemetry.jsonl mc validate
```

The recorded events are limited to low-cardinality command and step metadata such as command name, step kind, duration, outcome, and sanitized error category. The local sink does not record package names, repository paths, repository URLs, branch names, tags, commit hashes, command strings, raw error messages, or release-note text.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #302](https://github.com/monochange/monochange/pull/302) _Introduced in:_ [`72a126c`](https://github.com/monochange/monochange/commit/72a126cf0789ffbf1c8866f043f307c9f570b088) _Last updated in:_ [`46abfea`](https://github.com/monochange/monochange/commit/46abfea87db6ffed185a0739f5e15c46532192d6) _Related issues:_ [#295](https://github.com/monochange/monochange/issues/295), [#296](https://github.com/monochange/monochange/issues/296), [#297](https://github.com/monochange/monochange/issues/297), [#298](https://github.com/monochange/monochange/issues/298), [#299](https://github.com/monochange/monochange/issues/299), [#300](https://github.com/monochange/monochange/issues/300)

#### Improve migration tools

Add `mc migrate audit` to report legacy release tooling, changelog providers, and CI migration signals before moving a repository to monochange.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #332](https://github.com/monochange/monochange/pull/332) _Introduced in:_ [`3f4c89b`](https://github.com/monochange/monochange/commit/3f4c89bd3813317f6a962c38116c74fb0f83e486) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Related issues:_ [#319](https://github.com/monochange/monochange/issues/319)

#### Publish CLI npm packages with trusted publishing

monochange's own CLI npm package workflow now publishes without `NODE_AUTH_TOKEN` or `NPM_TOKEN`. The publish job keeps the protected `publisher` environment and `id-token: write` permission so npm can use GitHub OIDC trusted publishing and produce provenance for the CLI packages.

**Before:**

```yaml
- name: publish cli npm packages
  env:
    NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
  run: node scripts/npm/publish-packages.mjs --packages-dir packages
```

**After:**

```yaml
- name: publish cli npm packages
  run: node scripts/npm/publish-packages.mjs --packages-dir packages
```

The publish script rejects long-lived npm token environment variables and verifies it is running from `monochange/monochange`'s `publish.yml` workflow with GitHub Actions OIDC context before invoking `npm publish --provenance`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #330](https://github.com/monochange/monochange/pull/330) _Introduced in:_ [`7b3ebab`](https://github.com/monochange/monochange/commit/7b3ebab32b002e8a48595553685d6aaf72434d61) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#309](https://github.com/monochange/monochange/issues/309)

#### Add provider trust context detection

The capability model distinguishes trusted-publishing support, CI identity detection, registry-side setup verification, setup automation, and registry-native provenance so future enforcement can avoid overstating unsupported provider or registry combinations.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #331](https://github.com/monochange/monochange/pull/331) _Introduced in:_ [`a9c24e5`](https://github.com/monochange/monochange/commit/a9c24e55bd72678f2a67af8fa470387afe722603) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#313](https://github.com/monochange/monochange/issues/313)

#### Add the publish bootstrap command

Add `mc publish-bootstrap --from <ref>` for release-record-scoped first-time package setup.

The command uses the release record to choose package ids, runs placeholder publishing for that release package set, supports `--dry-run`, and can write a JSON bootstrap result artifact with `--output <path>`. Documentation now recommends rerunning `mc publish-readiness` after bootstrap before planning or publishing packages.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #318](https://github.com/monochange/monochange/pull/318) _Introduced in:_ [`cadb6fc`](https://github.com/monochange/monochange/commit/cadb6fccaac2ff9107db8b03bf6156762bc5a9b2) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add readiness-backed publish planning

`mc publish-plan` now accepts `--readiness <path>` for normal package publish planning. The plan validates that the `mc publish-readiness` artifact matches the current release record and covers the selected package set, then limits rate-limit batches to package ids that are ready in both the artifact and a fresh local readiness check.

Placeholder publish planning continues to reject readiness artifacts and should be run with `mc publish-plan --mode placeholder`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #305](https://github.com/monochange/monochange/pull/305) _Introduced in:_ [`e80c2b8`](https://github.com/monochange/monochange/commit/e80c2b8f1fd1df155e4aa05df8977f245f89bbc5) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Harden publish planning guards

`mc publish-plan`, `mc publish`, and `mc placeholder-publish` now respect the current workspace publishability rules instead of trusting stale release metadata or exact placeholder versions.

For `mc publish-plan --format json`, cargo batches previously included crates with `publish = false`, and release-record entries could keep npm or other ecosystem packages in the plan even after publishing was disabled.

Now publish batches skip packages that are currently private or excluded in discovery, and they also skip packages whose effective publish settings are disabled in the workspace configuration.

For `mc placeholder-publish --dry-run --format json`, placeholder bootstrap checks previously only looked for the exact `0.0.0` version, so a package that already had `1.0.0` on the registry could still be treated as needing a placeholder release.

Now placeholder planning skips any package that already has **any** version on its registry, and npm `setupUrl` values now point at:

```text
https://www.npmjs.com/package/<package>/access
```

`mc publish-plan` also falls back to the crates.io sparse index when the crates.io API denies package lookups, which keeps rate-limit planning working in CI environments that return `403 Forbidden` from the API endpoint.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #264](https://github.com/monochange/monochange/pull/264) _Introduced in:_ [`e542f69`](https://github.com/monochange/monochange/commit/e542f694e15fe91a778c3a66dae66358fe0053b6) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add Cargo publish-readiness guards

Built-in crates.io publishing now fails readiness before registry mutation when the current `Cargo.toml` is not publishable: `publish = false`, `publish = [...]` without `crates-io`, missing `description`, or missing both `license` and `license-file`.

Workspace-inherited Cargo metadata is accepted, and already-published package versions remain non-blocking when the saved readiness artifact still matches current readiness.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #303](https://github.com/monochange/monochange/pull/303) _Introduced in:_ [`0527e2c`](https://github.com/monochange/monochange/commit/0527e2c253d37ee283b9116e83db2a23b03b42b8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add initial publish readiness command

Adds `mc publish-readiness` as a non-mutating preflight command for package registry publishing. The command reads a release record from `--from`, dry-runs registry publish checks for the selected package set, reports ready/already-published/unsupported package states, and can write a JSON readiness artifact with `--output`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #292](https://github.com/monochange/monochange/pull/292) _Introduced in:_ [`63cbbe7`](https://github.com/monochange/monochange/commit/63cbbe7c06b03c0f1ed215a4fc61e0a74b50e1c4) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Require publish readiness artifacts

Require real `mc publish` package-registry runs to pass a readiness artifact generated by `mc publish-readiness`.

`mc publish-readiness` JSON artifacts now include schema metadata, release-record commit metadata, and a deterministic package-set fingerprint. `PublishPackages` validates the artifact before registry mutation and rejects missing, blocked, stale, malformed, duplicate, or package-mismatched readiness artifacts while leaving `--dry-run` publish previews artifact-free.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #301](https://github.com/monochange/monochange/pull/301) _Introduced in:_ [`97337aa`](https://github.com/monochange/monochange/commit/97337aad65e1f9dfc4d97fd381592b3bd57bc30a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Harden publish readiness artifact freshness

Adds a publish input fingerprint to `mc publish-readiness` artifacts. `mc publish` and readiness-backed `mc publish-plan` now reject artifacts when workspace config, package manifests, lockfiles, or registry/tooling inputs changed after the artifact was written.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #324](https://github.com/monochange/monochange/pull/324) _Introduced in:_ [`69f221d`](https://github.com/monochange/monochange/commit/69f221dc1f9b4823e8aa98ebdea6b84aaa57baeb) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add publish resume support

Add package publish result artifacts plus `mc publish --resume <path>` for retrying incomplete registry publishing after partial failures.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #323](https://github.com/monochange/monochange/pull/323) _Introduced in:_ [`8bca357`](https://github.com/monochange/monochange/commit/8bca35730a78f61c22dc71e473bc67a77210c4c6) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Attest GitHub release archives

monochange's own GitHub release asset workflow now runs from tag or manual dispatch events instead of draft release creation events. This makes the workflow compatible with GitHub immutable releases, where assets should exist before the release is finalized and draft `release.created` events are not a reliable trigger.

**Before:**

```yaml
on:
  release:
    types: [created]
```

The workflow uploaded CLI archives and checksum files, but did not create first-class GitHub artifact attestations for the uploaded `.tar.gz` and `.zip` archives.

**After:**

```yaml
on:
  push:
    tags:
      - "v*"
  workflow_dispatch:
```

The release asset job now requests the minimum attestation permissions, downloads each uploaded archive back from the release, creates GitHub build-provenance attestations for those archive subjects, and verifies the attestations before triggering downstream package publishing.

Users can verify a published archive with:

```bash
gh attestation verify monochange-x86_64-unknown-linux-gnu-v1.2.3.tar.gz \
  --repo monochange/monochange
```

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #329](https://github.com/monochange/monochange/pull/329) _Introduced in:_ [`ebc26a2`](https://github.com/monochange/monochange/commit/ebc26a2b23eef84660d079fdb1d8d5ad68d3f20c) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#308](https://github.com/monochange/monochange/issues/308)

#### Add release branch policy enforcement

monochange can now enforce that release tags and registry publishing only run from commits that are reachable from configured release branches. This moves release-branch safety from ad hoc CI shell checks into reusable CLI behavior that works in detached CI checkouts.

**Before (`monochange.toml`):**

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"
```

Release workflows had to add their own branch guard scripts before running commands such as `mc tag-release` or `mc publish`.

**After (`monochange.toml`):**

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"

[source.releases]
branches = ["main", "release/*"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
```

`mc tag-release`, `PublishRelease`, and `PublishPackages` now reject release refs that are not reachable from one of the configured release branch patterns. `CommitRelease` remains usable on release-preparation branches by default, but projects can opt into the same guard with `enforce_for_commit = true`.

**Before (manual CI guard):**

```bash
git fetch origin main
# custom shell checks here
mc tag-release --from v1.2.0 --push
```

**After (CLI-native guard):**

```bash
mc step:verify-release-branch --from v1.2.0
mc tag-release --from v1.2.0 --push
```

The explicit `step:verify-release-branch` command is available for pipelines that want an early, named verification step, while mutation commands still enforce the policy internally when the relevant `enforce_for_*` setting is enabled.

**Before (`monochange_core::ProviderReleaseSettings`):**

```rust
ProviderReleaseSettings {
    enabled,
    draft,
    prerelease,
    generate_notes,
    source,
}
```

**After:**

```rust
ProviderReleaseSettings {
    enabled,
    draft,
    prerelease,
    generate_notes,
    source,
    branches,
    enforce_for_tags,
    enforce_for_publish,
    enforce_for_commit,
}
```

Callers constructing `ProviderReleaseSettings` directly should include the release branch policy fields or use `ProviderReleaseSettings::default()` to keep the default `main` branch policy.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #321](https://github.com/monochange/monochange/pull/321) _Introduced in:_ [`e888554`](https://github.com/monochange/monochange/commit/e888554ca981816b80f5135086e8b226ee8f0a20) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#310](https://github.com/monochange/monochange/issues/310)

#### Ignore changelog-only updates in affected checks

Release automation now treats configured changelog targets as release metadata instead of as ordinary package source changes. That means changelog-only updates no longer make `mc affected --verify` fail with an uncovered package error, and newly generated release notes are inserted above older release headings so the latest release stays at the top of each changelog.

Configured changelog targets are unchanged:

```toml
[package.core.changelog]
path = "crates/core/changelog.md"
```

Command used by CI and local verification:

```bash
mc affected --format json --verify --changed-paths crates/core/changelog.md
```

**Before (output):**

```json
{
	"status": "failed",
	"affectedPackageIds": ["core"],
	"matchedPaths": ["crates/core/changelog.md"],
	"uncoveredPackageIds": ["core"]
}
```

**After (output):**

```json
{
	"status": "not_required",
	"affectedPackageIds": [],
	"ignoredPaths": ["crates/core/changelog.md"],
	"matchedPaths": [],
	"uncoveredPackageIds": []
}
```

Generated changelog sections also stay in reverse-chronological order:

```md
# Changelog

## [0.3.0] - 2026-04-23

- latest release notes

## [0.2.0] - 2026-03-01

- previous release notes
```

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #278](https://github.com/monochange/monochange/pull/278) _Introduced in:_ [`61a0593`](https://github.com/monochange/monochange/commit/61a0593264c153d6174beb4124812f5055a194dc) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Tighten release PR CI guards

The built-in GitHub Actions release automation now treats a commit as a release commit only when `HEAD` itself matches the stored release record. That prevents ordinary commits from skipping `publish:check` just because an older release record exists somewhere in history.

Command used by the workflow:

```bash
mc release-record --from HEAD --format json
```

**Before (workflow behavior):**

```yaml
if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
else
echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
fi
```

Any reachable release record could make CI behave as if the current commit was the release commit.

**After:**

```yaml
resolved_commit="$(jq -r '.resolvedCommit' /tmp/release-record.json)"
record_commit="$(jq -r '.recordCommit' /tmp/release-record.json)"

if [ "$resolved_commit" = "$record_commit" ]; then
echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
else
echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
fi
```

With that guard in place:

- `publish:check` is skipped only for the actual release commit at `HEAD`
- the generated `release.yml` template uses the same detection logic
- the `release-pr` job now runs only on pushes to `main`
- the workflow passes `GH_TOKEN` to `mc release-pr` so the built-in GitHub provider can authenticate without extra wrapper scripting

> _Owner:_ Ifiok Jr. _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`8b73540`](https://github.com/monochange/monochange/commit/8b7354011d99194a74450ad6907bcff5978b8e28) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Commit release PR messages from a file

Commit generated release pull request messages from a temporary file and include detailed commit diagnostics when git cannot create the release commit.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #328](https://github.com/monochange/monochange/pull/328) _Introduced in:_ [`7c4f80a`](https://github.com/monochange/monochange/commit/7c4f80a217cf16716221d701ef3fe52c1ea65443) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add no-verify support to release automation

> **Breaking change** — library consumers that construct `monochange_core::CliStepDefinition::CommitRelease` or `OpenReleaseRequest`, or that call the exported git/provider release helpers directly, must now handle the new `no_verify` field/argument.

Release automation can now bypass local git hooks when creating the generated release commit and when pushing the release request branch. This is useful for CI-driven `mc release-pr` flows where repository hooks depend on tools that are not available in the runner environment.

**Before (`monochange.toml`):**

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "markdown"], default = "markdown" },
]
steps = [
	{ type = "CommitRelease", name = "create release commit" },
	{ type = "OpenReleaseRequest", name = "create the pr" },
]
```

**After:**

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "markdown"], default = "markdown" },
	{ name = "no_verify", type = "boolean", default = true },
]
steps = [
	{ type = "CommitRelease", name = "create release commit", inputs = { no_verify = "{{ inputs.no_verify }}" } },
	{ type = "OpenReleaseRequest", name = "create the pr", inputs = { no_verify = "{{ inputs.no_verify }}" } },
]
```

That keeps the `mc release-pr` invocation the same while making hook bypass explicit in config:

```bash
mc release-pr
```

For crate consumers, the step and git helper APIs now carry the same flag through the full release-request pipeline.

**Before (`monochange_core` / hosting adapters):**

```rust
// before
CliStepDefinition::CommitRelease { name, when, inputs }
CliStepDefinition::OpenReleaseRequest { name, when, inputs }

git_commit_paths_command(root, &message)
git_push_branch_command(root, branch)
```

**After:**

```rust
// after
CliStepDefinition::CommitRelease { name, when, no_verify, inputs }
CliStepDefinition::OpenReleaseRequest { name, when, no_verify, inputs }

git_commit_paths_command(root, &message, no_verify)
git_push_branch_command(root, branch, no_verify)
```

Provider-facing release helpers in `monochange_hosting`, `monochange_github`, `monochange_gitlab`, and `monochange_gitea` now forward that flag so a single `no_verify` choice applies consistently to commit creation and branch push operations.

> _Owner:_ Ifiok Jr. _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`8b73540`](https://github.com/monochange/monochange/commit/8b7354011d99194a74450ad6907bcff5978b8e28) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Consolidate affected-package configuration

> **Breaking change** — affected-package policy now lives in `[changesets.affected]`. Configurations using the previous `[changesets.verify]` section must rename it, and configurations using the hosted-source affected-package policy section (`[source.bot.changesets]` in older configs, or `[source.affected]` in prerelease configs) must move `enabled`, `required`, `skip_labels`, `comment_on_failure`, `changed_paths`, and `ignored_paths` into `[changesets.affected]`.

Move affected-package policy settings into the changesets configuration:

```toml
[changesets.affected]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["Cargo.toml", "Cargo.lock"]
ignored_paths = ["**/tests/**"]
```

The Rust configuration model now exposes `ChangesetSettings::affected` with `ChangesetAffectedSettings`; the previous `ChangesetSettings::verify`, `SourceConfiguration::bot`, `SourceConfiguration::affected`, `ProviderChangesetBotSettings`, `ProviderAffectedSettings`, and `ProviderBotSettings` types or fields have been removed.

The `mc affected` policy command now reports `skipped` when it runs on a generated release pull request branch whose name starts with `source.pull_requests.branch_prefix`, allowing CI to ignore those branches.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #340](https://github.com/monochange/monochange/pull/340) _Introduced in:_ [`d5d8856`](https://github.com/monochange/monochange/commit/d5d8856b1522cd4ad70eeb06abd4d33ad7f0c9b6)

#### Expose built-in CLI steps as commands

Expose built-in CLI steps as immutable `step:*` commands and move default workflows into generated config.

Rename the `AffectedPackages` revision input from `since` to `from`, so the generated command now accepts `mc step:affected-packages --from <ref>`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #290](https://github.com/monochange/monochange/pull/290) _Introduced in:_ [`ec09ba2`](https://github.com/monochange/monochange/commit/ec09ba234cdc81fb5468292651a7c7968ffe7677) _Last updated in:_ [`72fca4d`](https://github.com/monochange/monochange/commit/72fca4d87f53053dd103e577f871581031a5088d)

#### enforce trusted publishing before registry publish commands

Packages with effective `publish.trusted_publishing = true` now fail before monochange invokes a built-in registry publish command unless the current environment exposes a verifiable CI/OIDC identity.

For GitHub Actions trusted publishing, monochange verifies the configured repository, workflow, optional environment, and `id-token: write` OIDC request variables. npm packages also reject long-lived token variables such as `NPM_TOKEN` and `NODE_AUTH_TOKEN` so trusted publishing cannot silently fall back to token-based publishing.

Use the same package configuration as before:

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.trusted_publishing]
workflow = "publish.yml"
environment = "publisher"
```

Run release publishing from the configured CI workflow, or set `publish.trusted_publishing = false` on an individual package when that package intentionally uses a manual publishing path.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #338](https://github.com/monochange/monochange/pull/338) _Introduced in:_ [`71dc3d0`](https://github.com/monochange/monochange/commit/71dc3d0632403a3a79f07fc58c1e656788a75cbd) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#312](https://github.com/monochange/monochange/issues/312)

#### Document the generated CLI process

Document the new CLI process where `mc init` generates editable workflow commands in `monochange.toml` and every built-in step is available directly through immutable `mc step:*` commands. The docs now clarify reserved command names such as `validate` and recommend `mc step:affected-packages --verify` for direct changeset-policy checks.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #291](https://github.com/monochange/monochange/pull/291) _Introduced in:_ [`74ac16a`](https://github.com/monochange/monochange/commit/74ac16af949ab07644c9b583774a00da2d95a7be) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Prefer verified GitHub release PR commits

When `[source.pull_requests].verified_commits = true`, `mc release-pr` publishes a GitHub release pull request from GitHub Actions by asking the GitHub provider to recreate the release branch commit through the Git Database API and only moves the branch when GitHub marks the replacement commit as verified.

The setting defaults to `false`. If the API commit cannot be created, is not verified, or the branch changes before the replacement lands, monochange leaves the normal pushed git commit in place and continues with the release PR flow.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #306](https://github.com/monochange/monochange/pull/306) _Introduced in:_ [`f93547d`](https://github.com/monochange/monochange/commit/f93547d919cf5bcbffe61beb675fa053307520c8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

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

#### add `mc subagents` for repo-local agent generation

`monochange` now ships `mc subagents` so repositories can generate repo-local agent definitions for Claude, VS Code, Copilot, Pi, Codex, and Cursor.

Before:

```bash
mc assist pi
```

After:

```bash
mc help subagents
mc subagents pi
mc subagents --all
```

Generated agent instructions are now CLI-first and prefer `mc`, then `monochange`, then `npx -y @monochange/cli`, while still generating optional MCP configuration for supported hosts.

`monochange_config` now reserves the `subagents` command name so workspace-defined commands cannot shadow the built-in generator.

`@monochange/skill` now teaches host agents to use `mc help subagents` / `mc subagents` instead of the removed `mc assist` flow.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #254](https://github.com/monochange/monochange/pull/254) _Introduced in:_ [`1b2b412`](https://github.com/monochange/monochange/commit/1b2b41237b194c608c3e521daefd3f15c2729f91) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### Add `mc skill` for project-local monochange skill installation through the upstream `skills add` workflow.

Before:

```bash
npm install -g @monochange/skill
monochange-skill --copy ~/.pi/agent/skills/monochange
```

After:

```bash
mc help skill
mc skill
mc skill -a pi -y
mc skill --list
```

`mc skill` auto-detects `npx`, `pnpm dlx`, or `bunx`, forwards the remaining native `skills add` flags, and installs the monochange skill source into the current project by default.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #257](https://github.com/monochange/monochange/pull/257) _Introduced in:_ [`0a6937a`](https://github.com/monochange/monochange/commit/0a6937ac81dadbc6252e5eac60fd692335cee3a5) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/monochange/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/monochange/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/monochange/monochange/issues/208)

#### Add advanced Dart workspace and Flutter lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #238](https://github.com/monochange/monochange/pull/238) _Introduced in:_ [`acb4faa`](https://github.com/monochange/monochange/commit/acb4faae2361ac75d01f20824ed85259ed5139a0) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#233](https://github.com/monochange/monochange/issues/233)

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/monochange/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/monochange/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/monochange/monochange/issues/230)

#### Add the first Dart metadata and publishability lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #236](https://github.com/monochange/monochange/pull/236) _Introduced in:_ [`f4669e3`](https://github.com/monochange/monochange/commit/f4669e3110c68bb6384bc985e412822ce2a3ffe9) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#231](https://github.com/monochange/monochange/issues/231)

#### Add Dart SDK constraint and dependency hygiene lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #237](https://github.com/monochange/monochange/pull/237) _Introduced in:_ [`1192d15`](https://github.com/monochange/monochange/commit/1192d1576156c89804373f1ea6b69f94d887f255) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#232](https://github.com/monochange/monochange/issues/232)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #206](https://github.com/monochange/monochange/pull/206) _Introduced in:_ [`a417022`](https://github.com/monochange/monochange/commit/a417022f80f93d61add00b8087e0f80102a9fd52) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### Add a package-scoped `mc analyze` CLI command for release-aware semantic analysis.

The command defaults `--release-ref` to the most recent tag for the selected package or its owning version group, compares `main -> head` for first releases when no prior tag exists, and supports text or JSON output for package-focused review workflows.

`monochange_config` now reserves the built-in `analyze` command name so workspace CLI definitions cannot collide with the new built-in subcommand.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #256](https://github.com/monochange/monochange/pull/256) _Introduced in:_ [`75e3329`](https://github.com/monochange/monochange/commit/75e33295d17248f4daca6e7bc83988855a033a08) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add visual status summary to benchmark CI comment sections

`monochange` benchmark PR comments now show an at-a-glance status summary inside each collapsed `<details>` section, so reviewers can see improvements and regressions without expanding anything.

**Before:**

- benchmark PR comments rendered every fixture table and phase timing table fully expanded
- scrolling to later fixtures required paging through the entire earlier benchmark output
- when sections were collapsed, there was no way to tell if a fixture improved or regressed without expanding it

**After:**

- each benchmark fixture renders as a collapsed section with a summary line showing emoji indicators
- per-command status: 🟢 improved · 🔴 regressed · ⚪ flat (for hyperfine tables with relative data)
- phase-level status: 🟢 phases improved · 🔴 phases regressed (for tables without relative comparison data)
- 🚨 over budget shown when any phase exceeds its configured budget
- reviewers can expand only the fixture tables they need while keeping the rest of the comment compact

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #258](https://github.com/monochange/monochange/pull/258) _Introduced in:_ [`d1fa746`](https://github.com/monochange/monochange/commit/d1fa7467bb8bc207939cbf10a907c5dc8fe725d4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #255](https://github.com/monochange/monochange/pull/255) _Introduced in:_ [`26e13ff`](https://github.com/monochange/monochange/commit/26e13fff071e93dc32fe071a5771232c980ebd46) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #247](https://github.com/monochange/monochange/pull/247) _Introduced in:_ [`8c96c8f`](https://github.com/monochange/monochange/commit/8c96c8f0a3b9d44bf30148b5a83067d7ce3ab62b) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#243](https://github.com/monochange/monochange/issues/243) _Related issues:_ [#244](https://github.com/monochange/monochange/issues/244)

#### expand the agent-facing harness around diagnostics, lint metadata, and repo guidance

`monochange` now exposes more of its review and planning surface directly through the repo and MCP so assistants can work from structured data instead of shell-only conventions.

**Before:**

- assistants could call the MCP server for validation, discovery, change creation, release previews, and affected-package checks
- lint metadata and changeset diagnostics still depended on `mc lint ...` and `mc diagnostics --format json`
- the repo guidance lived mostly in `AGENTS.md` and scattered docs without a dedicated plans directory or top-level architecture map

**After:**

- MCP now includes `monochange_diagnostics`, `monochange_lint_catalog`, and `monochange_lint_explain`
- the packaged skill and assistant setup docs now list the full MCP surface, including semantic analysis tools
- the repository now keeps an explicit `ARCHITECTURE.md` map plus `docs/plans/` for active plans, completed plans, and tech-debt tracking
- `docs:check` now verifies that the agent-facing docs stay aligned with the live MCP tool surface, and `lint:architecture` checks that provider/ecosystem dispatch stays inside the documented allowlist

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #251](https://github.com/monochange/monochange/pull/251) _Introduced in:_ [`47847db`](https://github.com/monochange/monochange/commit/47847db5d8e98e9b8284e72e5f94c184473b4ffd) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

#### add built-in package publishing and placeholder bootstrap commands

monochange can now publish package artifacts directly from its own release state instead of leaving registry publication entirely to external scripts.

**Before:**

```bash
mc release --dry-run --format json
mc publish-release --dry-run --format json
```

`mc publish-release` only handled hosted/provider releases such as GitHub releases. Package registry publication still had to be wired separately.

**After:**

```bash
mc placeholder-publish --format text
mc publish --format text
mc publish-release --format json
```

- `mc placeholder-publish` checks each built-in package registry and publishes a placeholder `0.0.0` package only when the package does not exist yet
- `mc publish` reads monochange release state and runs the built-in registry publish flow for supported public registries
- npm workspaces that use `pnpm` now publish with `pnpm publish`, and trusted-publishing setup runs through `pnpm exec npm trust ...`

**Before (`mc release --dry-run --format json`):**

```json
{
	"manifest": {
		"releaseTargets": [{ "id": "core", "version": "1.2.3" }]
	}
}
```

**After:**

```json
{
	"manifest": {
		"releaseTargets": [{ "id": "core", "version": "1.2.3" }],
		"packagePublications": [
			{
				"package": "core",
				"ecosystem": "cargo",
				"registry": "crates_io",
				"mode": "builtin",
				"version": "1.2.3"
			}
		]
	}
}
```

Built-in publishing also reports trusted-publishing status in text, markdown, and JSON output, including manual setup URLs when a registry still needs trust configured.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/monochange/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### improve npm and Deno semantic analysis with parser-backed JS/TS export extraction

`monochange_analyze_changes` and `monochange_validate_changeset` now use parser-backed JavaScript and TypeScript export extraction for npm and Deno packages instead of relying primarily on line-based export scanning.

This improves semantic diff accuracy for cases such as:

- multiline named export blocks
- namespace re-exports like `export * as toolkit from "./toolkit"`
- anonymous default exports
- more complex TypeScript and module export syntax

The MCP output shape stays the same, but the semantic evidence for npm and Deno packages is now more robust and closer to the actual module structure.

This work also extracts the shared JavaScript and TypeScript export-analysis logic into the new `monochange_ecmascript` crate, so npm and Deno keep their ecosystem-specific manifest analysis while reusing one parser-backed module analyzer.

The monochange skill documentation now also teaches the new-package rule: the first changeset for a newly introduced published package or crate should use a `major` bump for that new package entry.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #250](https://github.com/monochange/monochange/pull/250) _Introduced in:_ [`0dd8460`](https://github.com/monochange/monochange/commit/0dd846060614b2de9d3b2dfb5c1337075774b167) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#247](https://github.com/monochange/monochange/issues/247), [#249](https://github.com/monochange/monochange/issues/249)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #229](https://github.com/monochange/monochange/pull/229) _Introduced in:_ [`f148184`](https://github.com/monochange/monochange/commit/f148184d69fc4dc8720cde8db22768a8c1def8f7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #225](https://github.com/monochange/monochange/pull/225) _Introduced in:_ [`98013cb`](https://github.com/monochange/monochange/commit/98013cb86d5644a7327dc2ee5803d747d4a0372c) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add `mc tag-release` for post-merge release PR workflows

monochange now ships a first-class `mc tag-release` command for the long-running release PR flow.

**Before:**

```bash
mc release-record --from HEAD --format json
mc publish
```

That let CI detect a merged monochange release commit and publish package registries from the durable `ReleaseRecord`, but monochange did not have a built-in command to create and push the release tag set after merge.

**After:**

```bash
mc release-record --from HEAD --format json
mc tag-release --from HEAD
mc publish
```

`mc tag-release` reads the durable `ReleaseRecord` on the merged release commit, creates the declared tag set on that commit, and pushes those tags to `origin`.

**Before (generated GitHub Actions `release.yml`):**

```yaml
- name: prepare and open release PR
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: mc commit-release
```

**After:**

```yaml
- name: refresh release PR
  if: steps.release_record.outputs.is_release_commit != 'true'
  env:
    GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
  run: mc release-pr

- name: create release tags
  if: steps.release_record.outputs.is_release_commit == 'true'
  run: mc tag-release --from HEAD

- name: publish packages
  if: steps.release_record.outputs.is_release_commit == 'true'
  run: mc publish
```

The generated GitHub workflow now refreshes the release PR on normal `main` pushes, then switches into post-merge tagging and package publication when `HEAD` is already the merged monochange release commit.

The bundled `@monochange/cli` documentation now describes this post-merge tagging flow as part of the recommended release PR workflow.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #220](https://github.com/monochange/monochange/pull/220) _Introduced in:_ [`cf5e581`](https://github.com/monochange/monochange/commit/cf5e58113adcda077dfff7c3dd8f5e7598e411d8) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#209](https://github.com/monochange/monochange/issues/209)

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

### Fixed

#### align publish rate-limit plans with pending registry work

`mc publish`, `mc placeholder-publish`, and `mc publish-plan` now count only the package versions that are still missing from their registries when they build `publishRateLimits` output.

**Before:**

```bash
mc publish --dry-run --format json
mc placeholder-publish --dry-run --format json
mc publish-plan --format json
```

If some selected package versions were already present in their registries, the rate-limit report could still count them as pending work and recommend extra batches even though the publish command would skip them.

**After:**

```bash
mc publish --dry-run --format json
mc placeholder-publish --dry-run --format json
mc publish-plan --format json
```

The `publishRateLimits` report now shrinks automatically on reruns, partial publishes, and placeholder bootstrap flows where some packages already exist. That keeps advisory warnings, optional enforcement, and CI batch plans aligned with the actual work left to publish.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #240](https://github.com/monochange/monochange/pull/240) _Introduced in:_ [`63fbe0d`](https://github.com/monochange/monochange/commit/63fbe0de9825f3139386b7a25cf4821156813301) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### make manual trusted-publishing guidance more actionable

Improves CLI guidance for registries that still require manual trusted-publishing setup.

**Updated behavior:**

- manual trusted-publishing messages now point users to open the registry setup URL and match repository, workflow, and environment to the current GitHub context
- package-publish text and markdown output now include a concrete next step telling users to finish registry setup and rerun `mc publish`
- built-in publish prerequisite failures now tell users to complete registry setup and rerun the publish command

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #216](https://github.com/monochange/monochange/pull/216) _Introduced in:_ [`3ffb516`](https://github.com/monochange/monochange/commit/3ffb5165d643371be3315edf715a80b04f277144) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### improve trusted-publishing preflight diagnostics for manual registries

Improves trusted-publishing diagnostics for registries that still require manual setup.

**Updated behavior:**

- built-in publish preflight now validates the GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- manual-registry guidance now surfaces the resolved repository, workflow, and environment when monochange can infer them
- manual-registry errors now explain when the GitHub context is incomplete and point to the exact `publish.trusted_publishing.*` field that still needs configuration

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #218](https://github.com/monochange/monochange/pull/218) _Introduced in:_ [`85bc41f`](https://github.com/monochange/monochange/commit/85bc41f72766a34981e25cf1ad73442e9e80c267) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Refactor

#### Adopt `declare_lint_rule` for Cargo lint metadata and clarify when lint authors should use it.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #239](https://github.com/monochange/monochange/pull/239) _Introduced in:_ [`f30f62f`](https://github.com/monochange/monochange/commit/f30f62f26c5e846448a94b2aa08b5b84d147a27a) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#234](https://github.com/monochange/monochange/issues/234)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/monochange/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/monochange/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Testing

#### Fix CI race condition where tests that spawn `git` could fail under parallel `cargo llvm-cov` execution because skill command tests temporarily replace `PATH`. Capture the original `PATH` at process start and pass it explicitly to every git subprocess spawned by test helpers. Also reorder coverage job so Codecov uploads always complete before the patch threshold gate fails.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #262](https://github.com/monochange/monochange/pull/262) _Introduced in:_ [`184ab4f`](https://github.com/monochange/monochange/commit/184ab4fab3cf96f58b14f905a66511c6d0a469aa) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add fixture-first integration coverage for manual trust diagnostics

Adds fixture-based CLI coverage for manual-registry trusted-publishing diagnostics.

The new integration tests cover:

- resolved GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- missing workflow configuration guidance when monochange cannot resolve the GitHub workflow yet
- placeholder-publish dry-run output in both text and JSON formats

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #221](https://github.com/monochange/monochange/pull/221) _Introduced in:_ [`c7a0209`](https://github.com/monochange/monochange/commit/c7a0209392b81f70b5d51b0b777db40487b8ac29) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add trusted-publishing messaging test coverage

Adds regression coverage for trusted-publishing messaging in the `monochange` CLI and package-publish reporting.

The new tests cover:

- manual registry setup guidance rendering in text and markdown output
- preservation of explicit trusted-publishing context in manual-action outcomes

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #215](https://github.com/monochange/monochange/pull/215) _Introduced in:_ [`36c1d4e`](https://github.com/monochange/monochange/commit/36c1d4ec3c2daa675c233e388e161f339a77b6c2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

## [0.1.0](https://github.com/monochange/monochange/releases/tag/v0.1.0) (2026-04-13)

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

> _Owner:_ Ifiok Jr. _Introduced in:_ [`4542b5a`](https://github.com/monochange/monochange/commit/4542b5aee8b63a86c7ffc0ea9436090162a18056)
