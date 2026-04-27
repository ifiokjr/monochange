# Changelog

All notable changes to `monochange_graph` will be documented in this file.

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-27)

### Testing

#### test

Add property-based testing with proptest to `monochange_core` and `monochange_semver`. Add mutation-testing-killing tests to `monochange_graph` and `monochange_config` to close coverage gaps identified by `cargo-mutants`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #266](https://github.com/monochange/monochange/pull/266) _Introduced in:_ [`9ef1044`](https://github.com/monochange/monochange/commit/9ef1044ccdde303143453d6880cc68df374c0c13) _Last updated in:_ [`3a638bd`](https://github.com/monochange/monochange/commit/3a638bdb089b3a0d2c5968116e035ec052498542)

### Changed

#### test

Add additional property-based tests across crates.

- `monochange_config`: proptests for `render_changelog_path_template` with `{{package_dir}}`, `{{package_name}}`, unknown placeholder passthrough, and idempotence. Remove unused `minijinja` dependency.
- `monochange_core`: proptests for `strip_json_comments` on string literal preservation, line/block comment removal, idempotence, and comment-free input preservation.
- `monochange_graph`: proptests for `trigger_priority` ordering, `propagation_is_suppressed` correctness, and `NormalizedGraph` reverse edge correctness.
- `monochange_semver`: proptests for `merge_severities` idempotence and Major severity absorbing all values.
- Fix clippy `needless_borrow` lint in `monochange_github` and `indexing_slicing` lint in `monochange_graph` proptests.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #268](https://github.com/monochange/monochange/pull/268) _Introduced in:_ [`015c48d`](https://github.com/monochange/monochange/commit/015c48d77818b6e195d740986d7fa571867ad9b5)

#### Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### Added

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/monochange/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/monochange/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/monochange/monochange/issues/208)

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
