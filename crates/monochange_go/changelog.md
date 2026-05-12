# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## [0.4.2](https://github.com/monochange/monochange/releases/tag/v0.4.2) (2026-05-10)

### Added

#### Order publish plans by dependencies

Order publish plans by workspace dependencies before applying registry rate-limit windows, and run CI publishing as one dependency-ordered publish operation.

This keeps dependent packages from publishing before their internal dependencies are available and adds realistic fixture coverage for non-alphabetical cargo dependency graphs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #364](https://github.com/monochange/monochange/pull/364) _Introduced in:_ [`67eae95`](https://github.com/monochange/monochange/commit/67eae951e6a35a9b4c7c6489e89cd4779e44234e) _Last updated in:_ [`2392845`](https://github.com/monochange/monochange/commit/2392845ec29289e3f219aca20ac343cf79ee965e)

## [0.4.1](https://github.com/monochange/monochange/releases/tag/v0.4.1) (2026-05-10)

### Changed

- No package-specific changes were recorded; `monochange_go` was updated to 0.4.1 as part of group `main`.

## [0.4.0](https://github.com/monochange/monochange/releases/tag/v0.4.0) (2026-05-09)

### Added

#### Consolidate adapter traits to remove ecosystem match arms

Replace hardcoded ecosystem and registry match arms in `workspace_ops`, `monochange_config`, and `monochange_publish` with adapter registry dispatch.

- Expand `EcosystemAdapter` in `monochange_core` with `load_configured`, `supported_versioned_file_kind`, and `validate_versioned_file`.
- Add `From<EcosystemType>` and `From<PackageType>` conversions for `Ecosystem`.
- Add `FromStr` for `Ecosystem` and extract `default_registry_kind_for_ecosystem` into `monochange_core`.
- Implement the new trait methods in all ecosystem adapter crates.
- Replace `discover_packages` body with `build_ecosystem_registry().discover_all(root)?`.
- Replace `discover_release_workspace` `load_configured` match arms with registry dispatch.
- Replace `path_is_supported_for_ecosystem` and `validate_ecosystem_version_readable` match arms in `monochange_config` with registry dispatch.
- Introduce `PublishAdapter` trait and `PublishCommandBuilder` in `monochange_publish` to replace `build_publish_command` registry match arms.
- Extract `default_registry_kind_for_ecosystem` mapping out of `package_publish.rs` into `monochange_core`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #429](https://github.com/monochange/monochange/pull/429) _Introduced in:_ [`271e554`](https://github.com/monochange/monochange/commit/271e55420154265e798a0de3adf26a64faba66c8)

#### Move ecosystem constants out of core and delegate validation to ecosystem crates

Each ecosystem crate now owns its own `default_dependency_version_prefix()`, `default_dependency_fields()`, and `validate_versioned_file()` functions. The `EcosystemType::default_prefix()` and `EcosystemType::default_fields()` methods on `monochange_core::EcosystemType` are deprecated in favor of the ecosystem crate equivalents. `monochange_config` versioned file validation now dispatches to ecosystem crate validators instead of embedding ecosystem-specific parsing logic in config.

Closes #137 Closes #138

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #411](https://github.com/monochange/monochange/pull/411) _Introduced in:_ [`57e2322`](https://github.com/monochange/monochange/commit/57e232282d41e89e70f3e5b34b3e07d8b2089fea) _Related issues:_ [#137](https://github.com/monochange/monochange/issues/137), [#138](https://github.com/monochange/monochange/issues/138)

#### Move Go placeholder manifests into monochange_go

Move Go placeholder `go.mod` generation out of `monochange::package_publish` and into `monochange_go`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #420](https://github.com/monochange/monochange/pull/420) _Introduced in:_ [`28e4c9d`](https://github.com/monochange/monochange/commit/28e4c9d1c31579456ec93dc59d59f683608240e4) _Related issues:_ [#419](https://github.com/monochange/monochange/issues/419)

### Fixed

#### Remove grouped release member summaries

Grouped release notes no longer include generated changed or synchronized member lists, keeping the release note summary focused on the group release itself.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #395](https://github.com/monochange/monochange/pull/395) _Introduced in:_ [`2d012ff`](https://github.com/monochange/monochange/commit/2d012ff900a612f4aed6e4d7034c8c876f50aeae) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### Testing

#### Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #428](https://github.com/monochange/monochange/pull/428) _Introduced in:_ [`b61cc3e`](https://github.com/monochange/monochange/commit/b61cc3e66989fd83ffb16a31568d2f46d7075216)

## [0.3.4](https://github.com/monochange/monochange/releases/tag/v0.3.4) (2026-05-06)

### Fixed

#### Preserve publish batch dependency order

Carry prior packages into later publish-plan batches so dependency-ordered publish requests remain available when registry rate limits split a release into multiple jobs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #389](https://github.com/monochange/monochange/pull/389) _Introduced in:_ [`12d3582`](https://github.com/monochange/monochange/commit/12d35826c3b0a8768bbf05c82b1e999a0e9ca30a)

#### Use npm for trusted npm publishing

Route trusted npm publishes through the npm CLI even in pnpm-managed workspaces so npm's OIDC trusted publishing flow can exchange the GitHub Actions identity for a short-lived publish credential. The release workflow also relies on devenv environment cleaning directly instead of the removed `strip:env` wrapper.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #388](https://github.com/monochange/monochange/pull/388) _Introduced in:_ [`72773bc`](https://github.com/monochange/monochange/commit/72773bc438167b55c26bb7c3f5dd9d7a21c99084)

## [0.3.3](https://github.com/monochange/monochange/releases/tag/v0.3.3) (2026-05-06)

### Fixed

#### preserve GitHub OIDC environment variables in devenv

The development environment's `devenv.yaml` now keeps the GitHub Actions and OIDC identity variables that monochange needs to detect trusted publishing when running inside `devenv shell`. Previously, `strip: env` removed these variables and caused built-in publishing to fail with "No supported CI provider identity was detected."

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #386](https://github.com/monochange/monochange/pull/386) _Introduced in:_ [`fd1a798`](https://github.com/monochange/monochange/commit/fd1a798e57234fc465c33537077ec6acf0a47db8)

## [0.3.2](https://github.com/monochange/monochange/releases/tag/v0.3.2) (2026-05-06)

### Changed

- No package-specific changes were recorded; `monochange_go` was updated to 0.3.2 as part of group `main`.

## [0.3.1](https://github.com/monochange/monochange/releases/tag/v0.3.1) (2026-05-05)

### Fixed

#### Preserve rendered changelog metadata in release records

Release records now store full changelog metadata so publish flows reconstructed from git history can use the rendered release notes instead of falling back to minimal release bodies.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #356](https://github.com/monochange/monochange/pull/356) _Introduced in:_ [`6f38c00`](https://github.com/monochange/monochange/commit/6f38c003a77fcc4a95e33ae1c344340bbcce1017)

#### Preserve configured changelog sections for scalar change types

Configured changelog types now take precedence over scalar bump names so generated release notes retain their intended sections. Local telemetry JSONL writes now append complete event lines to avoid malformed records during concurrent command runs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #363](https://github.com/monochange/monochange/pull/363) _Introduced in:_ [`8c8c9dc`](https://github.com/monochange/monochange/commit/8c8c9dc98f6a95d2c8a2d55fb986a66c08f29312)

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

### Changed

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
