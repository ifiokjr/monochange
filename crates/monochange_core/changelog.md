# Changelog

All notable changes to `monochange_core` will be documented in this file.

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-29)

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

### Testing

#### Add property-based and mutation tests

Add property-based testing with proptest to `monochange_core` and `monochange_semver`. Add mutation-testing-killing tests to `monochange_graph` and `monochange_config` to close coverage gaps identified by `cargo-mutants`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #266](https://github.com/monochange/monochange/pull/266) _Introduced in:_ [`9ef1044`](https://github.com/monochange/monochange/commit/9ef1044ccdde303143453d6880cc68df374c0c13) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add mutation regression coverage

Add cargo-mutants regression coverage for `monochange_analysis`, `monochange_config`, `monochange_core`, and `monochange_hosting`.

- `monochange_analysis`: add tests for `latest_workspace_release_tag` to ignore namespaced tags, `snapshot_files_from_working_tree` and `read_text_file_from_git_object` for medium/exact-size files, `detect_raw_pr_environment` to reject non-PR events across CI providers, `default_branch_name` with origin HEAD symbolic ref, `get_merge_base` returning actual merge base, and `ChangeFrame::changed_files` distinguishing working directory from staged-only.
- `monochange_config`: add fixture-backed tests and proptests for ecosystem versioned-file inheritance, explicit group bump inference, and changelog validation defaults to close mutation gaps in release-planning configuration loading.
- `monochange_core`: add fixture-backed tests for discovery filtering around parent `.git` directories outside the workspace root and block-comment stripping edge cases in JSON helper logic.
- `monochange_hosting`: add HTTP-mock tests for error paths in `get_json`, `get_optional_json`, `post_json`, `put_json`, and `patch_json` to kill mutants on status-code checks. Update `Cargo.toml` to include `src/*.rs` in the distribution manifest so `__tests.rs` builds correctly with `httpmock` as a dev-dependency.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #277](https://github.com/monochange/monochange/pull/277) _Introduced in:_ [`4c411d9`](https://github.com/monochange/monochange/commit/4c411d9efe84aaefbe2231ac16e4065249fc2a06) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

### Changed

#### Add additional property-based tests

Add additional property-based tests across crates.

- `monochange_config`: proptests for `render_changelog_path_template` with `{{package_dir}}`, `{{package_name}}`, unknown placeholder passthrough, and idempotence. Remove unused `minijinja` dependency.
- `monochange_core`: proptests for `strip_json_comments` on string literal preservation, line/block comment removal, idempotence, and comment-free input preservation.
- `monochange_graph`: proptests for `trigger_priority` ordering, `propagation_is_suppressed` correctness, and `NormalizedGraph` reverse edge correctness.
- `monochange_semver`: proptests for `merge_severities` idempotence and Major severity absorbing all values.
- Fix clippy `needless_borrow` lint in `monochange_github` and `indexing_slicing` lint in `monochange_graph` proptests.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #268](https://github.com/monochange/monochange/pull/268) _Introduced in:_ [`015c48d`](https://github.com/monochange/monochange/commit/015c48d77818b6e195d740986d7fa571867ad9b5) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### add attestation policy configuration

Add first-class package and release attestation policy settings. Package publish settings now support `publish.attestations.require_registry_provenance`, inherited from ecosystem publish settings and overridable per package.

When registry provenance is required, built-in release publishing fails before invoking registry commands unless trusted publishing is enabled, the current CI/OIDC identity is verifiable, the provider/registry capability matrix reports registry-native provenance support, and the built-in publisher can require that provenance. npm release publishes add `--provenance` only when this policy is enabled.

GitHub release asset attestation intent is modeled under `[source.releases.attestations]` with `require_github_artifact_attestations`, which is accepted only for GitHub sources.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #339](https://github.com/monochange/monochange/pull/339) _Introduced in:_ [`2849f7b`](https://github.com/monochange/monochange/commit/2849f7b771c24b2e3459450661d24c7f36c91a9c) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#315](https://github.com/monochange/monochange/issues/315)

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

#### Handle large release commit messages reliably

Release commit creation now streams the generated commit message through standard input instead of passing the full release record as a command-line argument. This avoids operating-system argument length limits for large release records.

Git command spawning now reuses a stable path that contains `git`, and benchmark fixture git commands now use monochange's sanitized git command helper with automatic garbage collection disabled for deterministic synthetic history setup in CI.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #320](https://github.com/monochange/monochange/pull/320) _Introduced in:_ [`f41f985`](https://github.com/monochange/monochange/commit/f41f985288e1440b3b64c2fa9c1cda987925ef8a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Update repository URLs

Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Publish GitHub releases through drafts

- Add a boolean `draft` input to the built-in `PublishRelease` step so CLI commands can create hosted releases as drafts while preserving `[source.releases].draft` defaults.
- Update release automation to create draft GitHub releases, run the asset upload workflow against those drafts, then publish the drafts after assets are attached.
- Add a global `--jq` filter for JSON-producing commands so automation can extract release tags and other fields directly from `--format json` output.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #325](https://github.com/monochange/monochange/pull/325) _Introduced in:_ [`bee06fe`](https://github.com/monochange/monochange/commit/bee06fed90e50cda3de695b973f415dd162eec29) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add readiness-backed publish planning

`mc publish-plan` now accepts `--readiness <path>` for normal package publish planning. The plan validates that the `mc publish-readiness` artifact matches the current release record and covers the selected package set, then limits rate-limit batches to package ids that are ready in both the artifact and a fresh local readiness check.

Placeholder publish planning continues to reject readiness artifacts and should be run with `mc publish-plan --mode placeholder`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #305](https://github.com/monochange/monochange/pull/305) _Introduced in:_ [`e80c2b8`](https://github.com/monochange/monochange/commit/e80c2b8f1fd1df155e4aa05df8977f245f89bbc5) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Require publish readiness artifacts

Require real `mc publish` package-registry runs to pass a readiness artifact generated by `mc publish-readiness`.

`mc publish-readiness` JSON artifacts now include schema metadata, release-record commit metadata, and a deterministic package-set fingerprint. `PublishPackages` validates the artifact before registry mutation and rejects missing, blocked, stale, malformed, duplicate, or package-mismatched readiness artifacts while leaving `--dry-run` publish previews artifact-free.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #301](https://github.com/monochange/monochange/pull/301) _Introduced in:_ [`97337aa`](https://github.com/monochange/monochange/commit/97337aad65e1f9dfc4d97fd381592b3bd57bc30a) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add publish resume support

Add package publish result artifacts plus `mc publish --resume <path>` for retrying incomplete registry publishing after partial failures.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #323](https://github.com/monochange/monochange/pull/323) _Introduced in:_ [`8bca357`](https://github.com/monochange/monochange/commit/8bca35730a78f61c22dc71e473bc67a77210c4c6) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

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

#### Expose built-in CLI steps as commands

Expose built-in CLI steps as immutable `step:*` commands and move default workflows into generated config.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #290](https://github.com/monochange/monochange/pull/290) _Introduced in:_ [`ec09ba2`](https://github.com/monochange/monochange/commit/ec09ba234cdc81fb5468292651a7c7968ffe7677) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

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

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/monochange/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/monochange/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/monochange/monochange/issues/208)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/monochange/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #227](https://github.com/monochange/monochange/pull/227) _Introduced in:_ [`78af3c2`](https://github.com/monochange/monochange/commit/78af3c244a4090965b455e2879b33a160e28da77) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/monochange/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/monochange/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/monochange/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/monochange/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/monochange/monochange/issues/230)

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
