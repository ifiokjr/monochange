# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.3.3](https://github.com/monochange/monochange/releases/tag/v0.3.3) (2026-05-06)

### Fixed

#### preserve GitHub OIDC environment variables in devenv

The development environment's `devenv.yaml` now keeps the GitHub Actions and OIDC identity variables that monochange needs to detect trusted publishing when running inside `devenv shell`. Previously, `strip: env` removed these variables and caused built-in publishing to fail with "No supported CI provider identity was detected."

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #386](https://github.com/monochange/monochange/pull/386) _Introduced in:_ [`fd1a798`](https://github.com/monochange/monochange/commit/fd1a798e57234fc465c33537077ec6acf0a47db8)

## [0.3.2](https://github.com/monochange/monochange/releases/tag/v0.3.2) (2026-05-06)

### Changed

- No package-specific changes were recorded; `monochange_gitlab` was updated to 0.3.2 as part of group `main`.

## [0.3.1](https://github.com/monochange/monochange/releases/tag/v0.3.1) (2026-05-05)

### Fixed

#### Preserve rendered changelog metadata in release records

Release records now store full changelog metadata so publish flows reconstructed from git history can use the rendered release notes instead of falling back to minimal release bodies.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #356](https://github.com/monochange/monochange/pull/356) _Introduced in:_ [`6f38c00`](https://github.com/monochange/monochange/commit/6f38c003a77fcc4a95e33ae1c344340bbcce1017)

#### Preserve configured changelog sections for scalar change types

Configured changelog types now take precedence over scalar bump names so generated release notes retain their intended sections. Local telemetry JSONL writes now append complete event lines to avoid malformed records during concurrent command runs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #363](https://github.com/monochange/monochange/pull/363) _Introduced in:_ [`8c8c9dc`](https://github.com/monochange/monochange/commit/8c8c9dc98f6a95d2c8a2d55fb986a66c08f29312)

#### Ensure draft releases use proper title fallback

When a release manifest is reconstructed from git history (e.g. during `release-post-merge`), `rendered_title` may be empty. In that case, `build_release_requests` now falls back to `tag_name` for the release name across all providers (GitHub, GitLab, Gitea). This prevents draft releases from appearing with a generic "Draft" title and ensures they display the actual version tag instead.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #369](https://github.com/monochange/monochange/pull/369) _Introduced in:_ [`eef785e`](https://github.com/monochange/monochange/commit/eef785ee2123acca25fb5715f3487568ecaffdf0)

#### Filter placeholder publish reports to packages that need action

`mc placeholder-publish` now hides already-published and skipped packages from the default report so dry runs focus on packages that still need placeholder publishing, and real runs focus on packages that were published or failed.

Pass `--show-all` to include the full package report when auditing every selected package.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #372](https://github.com/monochange/monochange/pull/372) _Introduced in:_ [`26f20e6`](https://github.com/monochange/monochange/commit/26f20e6347429e57bc94aea06a40eec81f85c54d)

#### Publish packages in dependency order without readiness artifacts

Package publishing now derives release work directly from prepared release or `HEAD` release state, orders internal publish-relevant dependencies before dependents, and rejects publish-relevant dependency cycles while allowing development-only cycles.

The publish order now works like this:

1. Build the selected publish requests from the prepared release or `HEAD` release state.
2. Materialize the workspace dependency graph.
3. Consider only dependencies where **both packages are part of the selected publish set**.
4. Ignore development dependency edges.
5. Topologically sort the publish requests so dependencies are emitted before dependents.

So for a tree like:

```text
core        # no dependencies
utils       # depends on core
api         # depends on utils
app         # depends on core, utils, api
```

the publish order becomes:

```text
core
utils
api
app
```

If multiple packages are independent at the same depth, their order is deterministic by package id, registry, and version.

A package with no selected dependencies is eligible first. A package is not published until all of its selected publish-relevant dependencies have been ordered before it. Dependencies outside the selected publish set do not block ordering. Development-only cycles are ignored. Runtime, build, peer, workspace, and unknown dependency cycles fail before publishing anything, with a cycle diagnostic.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e)

#### Make release workspace publishing preserve Cargo verification

`monochange_test_helpers` is now publishable so crates that use the shared helpers in their dev-dependencies can still pass Cargo's normal publish verification. `monochange_core` no longer dev-depends on the helper crate: its integration-style discovery filter coverage now lives in the unpublished `monochange_integration_tests` crate, preventing a dependency cycle between the published core crate and the test helper crate.

Package publishing keeps Cargo verification enabled and still runs JavaScript registry tooling without inherited `LD_LIBRARY_PATH`, preserving PNPM support while avoiding Nix/devenv library-path leakage into system Node.js launchers.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #368](https://github.com/monochange/monochange/pull/368) _Introduced in:_ [`b79eef1`](https://github.com/monochange/monochange/commit/b79eef170a01234b69b2b83c8ebd4ef946a079ac)

#### Use `GITHUB_TOKEN` for Git Data API to create verified commits

The `release-pr` workflow now passes `GITHUB_COMMIT_TOKEN` (set to `secrets.GITHUB_TOKEN`) specifically for Git Database API operations (blob, tree, commit creation, and ref updates). This allows GitHub to automatically sign commits with the `web-flow` GPG key, producing verified commits on release pull requests.

The `GH_TOKEN` (PAT) continues to be used for all other GitHub API operations like pull request creation and updates.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #371](https://github.com/monochange/monochange/pull/371) _Introduced in:_ [`3770b48`](https://github.com/monochange/monochange/commit/3770b48bab6b41c80086a0d3e2e4e6a9a7540c39)

### Other

#### Resolve git identity from token for release PR commits

The `release-pr` workflow now queries the GitHub API for the authenticated user's `id`, `login`, and `name`, then constructs the standard GitHub noreply email (`{id}+{login}@users.noreply.github.com`) for `git config user.email`. This replaces the previous hardcoded `github-actions[bot]` identity, so release PR commits are properly attributed to the account that owns the `RELEASE_PR_MERGE_TOKEN`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #367](https://github.com/monochange/monochange/pull/367) _Introduced in:_ [`920bf04`](https://github.com/monochange/monochange/commit/920bf04ba34aa7050e0dc6a9be5c488c9431d085)

#### Use the current monochange CLI when publishing release tags

The publish workflow now builds the `mc` binary from the workflow commit before checking out the release tag. Publish jobs still operate on the requested release tag's files and release state, but they execute the current workflow version of `mc` so post-release publishing fixes apply when rerunning publication for an older tag.

The workflow keeps full branch and tag history available after switching to the release tag so publish-time release branch reachability checks still work. The release workflow also dispatches `publish.yml` at the current workflow commit, allowing a fixed publish workflow to publish an older release tag.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #366](https://github.com/monochange/monochange/pull/366) _Introduced in:_ [`9bb5ca9`](https://github.com/monochange/monochange/commit/9bb5ca9ca5315f60a1079a55470f7b77ff8e3ea2) _Related issues:_ [#364](https://github.com/monochange/monochange/issues/364)

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-30)

### Testing

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

### Changed

#### Update repository URLs

Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

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

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### Added

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

#### align provider and hosting release examples with package publication metadata

The hosting/provider crates in this PR all moved together around the same outward shape change: `ReleaseManifest` now carries `package_publications`, and the provider-facing examples and compatibility fixtures now show that field consistently.

**Before:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    released_packages: vec!["workflow-core".to_string()],
    changed_files: Vec::new(),
    ..todo!()
};
```

**After:**

```rust
let manifest = ReleaseManifest {
    release_targets: vec![target],
    released_packages: vec!["workflow-core".to_string()],
    package_publications: Vec::new(),
    changed_files: Vec::new(),
    ..todo!()
};
```

`monochange_github` updates its public example to match the new manifest shape, while `monochange_hosting`, `monochange_gitlab`, and `monochange_gitea` now exercise the same field in their compatibility coverage instead of lagging behind `monochange_core`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`62801a7`](https://github.com/monochange/monochange/commit/62801a789eca1186717abc5619407d59aa4584b6) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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
