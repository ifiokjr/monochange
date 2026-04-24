# Changelog

All notable changes to the `main` release group will be documented in this file.

## [0.2.0](https://github.com/ifiokjr/monochange/releases/tag/v0.2.0) (2026-04-21)

Grouped release for `main`.

Changed members: monochange, monochange_core, monochange_cargo, monochange_npm, monochange_config, monochange_deno, monochange_ecmascript, monochange_dart, monochange_graph, monochange_semver, monochange_github, monochange_gitlab, monochange_gitea, monochange_hosting, monochange_analysis, monochange_lint, monochange_linting, monochange_lint_testing, @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-x64-msvc, @monochange/cli-win32-arm64-msvc, @monochange/skill

### Added

> [!NOTE]
> _monochange_

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #258](https://github.com/ifiokjr/monochange/pull/258) _Introduced in:_ [`d1fa746`](https://github.com/ifiokjr/monochange/commit/d1fa7467bb8bc207939cbf10a907c5dc8fe725d4) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/ifiokjr/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/ifiokjr/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Fixed

> [!NOTE]
> _monochange_

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #240](https://github.com/ifiokjr/monochange/pull/240) _Introduced in:_ [`63fbe0d`](https://github.com/ifiokjr/monochange/commit/63fbe0de9825f3139386b7a25cf4821156813301) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### make manual trusted-publishing guidance more actionable

Improves CLI guidance for registries that still require manual trusted-publishing setup.

**Updated behavior:**

- manual trusted-publishing messages now point users to open the registry setup URL and match repository, workflow, and environment to the current GitHub context
- package-publish text and markdown output now include a concrete next step telling users to finish registry setup and rerun `mc publish`
- built-in publish prerequisite failures now tell users to complete registry setup and rerun the publish command

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #216](https://github.com/ifiokjr/monochange/pull/216) _Introduced in:_ [`3ffb516`](https://github.com/ifiokjr/monochange/commit/3ffb5165d643371be3315edf715a80b04f277144) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### improve trusted-publishing preflight diagnostics for manual registries

Improves trusted-publishing diagnostics for registries that still require manual setup.

**Updated behavior:**

- built-in publish preflight now validates the GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- manual-registry guidance now surfaces the resolved repository, workflow, and environment when monochange can infer them
- manual-registry errors now explain when the GitHub context is incomplete and point to the exact `publish.trusted_publishing.*` field that still needs configuration

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #218](https://github.com/ifiokjr/monochange/pull/218) _Introduced in:_ [`85bc41f`](https://github.com/ifiokjr/monochange/commit/85bc41f72766a34981e25cf1ad73442e9e80c267) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Testing

> [!NOTE]
> _monochange_

#### Fix CI race condition where tests that spawn `git` could fail under parallel `cargo llvm-cov` execution because skill command tests temporarily replace `PATH`. Capture the original `PATH` at process start and pass it explicitly to every git subprocess spawned by test helpers. Also reorder coverage job so Codecov uploads always complete before the patch threshold gate fails.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #262](https://github.com/ifiokjr/monochange/pull/262) _Introduced in:_ [`184ab4f`](https://github.com/ifiokjr/monochange/commit/184ab4fab3cf96f58b14f905a66511c6d0a469aa) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### add fixture-first integration coverage for manual trust diagnostics

Adds fixture-based CLI coverage for manual-registry trusted-publishing diagnostics.

The new integration tests cover:

- resolved GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- missing workflow configuration guidance when monochange cannot resolve the GitHub workflow yet
- placeholder-publish dry-run output in both text and JSON formats

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #221](https://github.com/ifiokjr/monochange/pull/221) _Introduced in:_ [`c7a0209`](https://github.com/ifiokjr/monochange/commit/c7a0209392b81f70b5d51b0b777db40487b8ac29) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### add trusted-publishing messaging test coverage

Adds regression coverage for trusted-publishing messaging in the `monochange` CLI and package-publish reporting.

The new tests cover:

- manual registry setup guidance rendering in text and markdown output
- preservation of explicit trusted-publishing context in manual-action outcomes

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #215](https://github.com/ifiokjr/monochange/pull/215) _Introduced in:_ [`36c1d4e`](https://github.com/ifiokjr/monochange/commit/36c1d4ec3c2daa675c233e388e161f339a77b6c2) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

## [0.1.0](https://github.com/ifiokjr/monochange/releases/tag/v0.1.0) (2026-04-13)

Grouped release for `main`.

Changed members: monochange, monochange_core, monochange_cargo, monochange_npm, monochange_config, monochange_deno, monochange_dart, monochange_graph, monochange_semver, monochange_github, monochange_gitlab, monochange_gitea, monochange_hosting

### Breaking changes

> [!NOTE]
> _main_

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
