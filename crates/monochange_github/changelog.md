## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-26)

### Testing

#### add changelog section thresholds for collapsed and ignored sections

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

> _Owner:_ Ifiok Jr. _Introduced in:_ [`4426b99`](https://github.com/monochange/monochange/commit/4426b9916791ceff82957f61837be1e681988c9a)

### Changed

#### add post-merge release automation and release PR merge guards

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #282](https://github.com/monochange/monochange/pull/282) _Introduced in:_ [`014491d`](https://github.com/monochange/monochange/commit/014491ddb0de1a562bd0ca6552bba9646baf7f42)

#### Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740)

#### add `no_verify` support to automated release commit and release request steps

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

> _Owner:_ Ifiok Jr. _Introduced in:_ [`8b73540`](https://github.com/monochange/monochange/commit/8b7354011d99194a74450ad6907bcff5978b8e28)

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
