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

- No package-specific changes were recorded; `monochange_python` was updated to 0.3.2 as part of group `main`.

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

#### Document supported ecosystem capabilities

The documentation now includes a dedicated ecosystem guide that compares Cargo, npm-family, Deno, Dart / Flutter, and Python support across discovery, manifest updates, lockfile handling, and built-in registry publishing. Python is documented as a supported release-planning ecosystem with uv workspace discovery, Poetry and PEP 621 `pyproject.toml` parsing, Python dependency normalization, manifest version rewrites, internal dependency rewrites, and inferred `uv lock` / `poetry lock --no-update` lockfile commands.

The guide also clarifies ecosystem publishing boundaries, including canonical public registry support and the external-mode escape hatch for private registries or custom publication flows.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #307](https://github.com/monochange/monochange/pull/307) _Introduced in:_ [`11c628c`](https://github.com/monochange/monochange/commit/11c628cd2afb7c9509c31a8cfc043be63a9f2a75) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

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
