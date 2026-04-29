# Changelog

All notable changes to the `main` release group will be documented in this file.

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-29)

Grouped release for `main`.

Changed members: monochange, monochange_core, monochange_cargo, monochange_npm, monochange_config, monochange_deno, monochange_dart, monochange_python, monochange_go, monochange_graph, monochange_semver, monochange_telemtry, monochange_github, monochange_gitlab, monochange_gitea, monochange_hosting, monochange_analysis, monochange_lint, @monochange/cli, @monochange/cli-darwin-arm64, @monochange/cli-darwin-x64, @monochange/cli-linux-arm64-gnu, @monochange/cli-linux-arm64-musl, @monochange/cli-linux-x64-gnu, @monochange/cli-linux-x64-musl, @monochange/cli-win32-x64-msvc, @monochange/cli-win32-arm64-msvc, @monochange/skill

Synchronized members: monochange_ecmascript, monochange_linting, monochange_lint_testing

> [!NOTE]
> _monochange_

#### Add cargo-binstall metadata

Add cargo-binstall metadata so `cargo binstall monochange` can resolve the GitHub release archive layout.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #293](https://github.com/monochange/monochange/pull/293) _Introduced in:_ [`497f8c0`](https://github.com/monochange/monochange/commit/497f8c010a534fcac6e3ed26bb21c220c54e7a5e) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

> [!NOTE]
> _monochange_

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

> [!NOTE]
> _monochange_

#### Add colored CLI help

Add beautiful colored CLI help with detailed examples

The `mc help <command>` subcommand now renders detailed, formatted help with bordered headers, colored sections, multiple examples per command, tips, and cross-references. Running `mc help` shows an overview listing all commands. The standard `--help` flags also use ANSI colors via an anstyle theme. All colors respect NO_COLOR and TTY detection.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #265](https://github.com/monochange/monochange/pull/265) _Introduced in:_ [`8890d77`](https://github.com/monochange/monochange/commit/8890d77e8d54f81f8807588192441a3cd46bfbb8) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

> [!NOTE]
> _monochange_

#### Fix binary benchmark changeset fixtures

Update generated binary benchmark changesets to include summary headings so the PR benchmark fixtures pass changeset validation.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

> [!NOTE]
> _monochange_

#### Fix release merge blocker workflow

Replace the release PR merge blocker action with an inline shell guard so normal pull requests are not blocked by missing action dependencies.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #337](https://github.com/monochange/monochange/pull/337) _Introduced in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

> [!NOTE]
> _monochange_

#### Default CLI output to markdown

Default output format to markdown with termimad terminal rendering.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #263](https://github.com/monochange/monochange/pull/263) _Introduced in:_ [`020df1f`](https://github.com/monochange/monochange/commit/020df1f2d1bec0d8470fe1f4e734ee31e3e167bf) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

> [!NOTE]
> _monochange_

#### Improve migration tools

Add `mc migrate audit` to report legacy release tooling, changelog providers, and CI migration signals before moving a repository to monochange.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #332](https://github.com/monochange/monochange/pull/332) _Introduced in:_ [`3f4c89b`](https://github.com/monochange/monochange/commit/3f4c89bd3813317f6a962c38116c74fb0f83e486) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Related issues:_ [#319](https://github.com/monochange/monochange/issues/319)

> [!NOTE]
> _monochange_

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

> [!NOTE]
> _monochange_

#### Add provider trust context detection

The capability model distinguishes trusted-publishing support, CI identity detection, registry-side setup verification, setup automation, and registry-native provenance so future enforcement can avoid overstating unsupported provider or registry combinations.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #331](https://github.com/monochange/monochange/pull/331) _Introduced in:_ [`a9c24e5`](https://github.com/monochange/monochange/commit/a9c24e55bd72678f2a67af8fa470387afe722603) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#313](https://github.com/monochange/monochange/issues/313)

> [!NOTE]
> _monochange_

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

> [!NOTE]
> _monochange_

#### Add initial publish readiness command

Adds `mc publish-readiness` as a non-mutating preflight command for package registry publishing. The command reads a release record from `--from`, dry-runs registry publish checks for the selected package set, reports ready/already-published/unsupported package states, and can write a JSON readiness artifact with `--output`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #292](https://github.com/monochange/monochange/pull/292) _Introduced in:_ [`63cbbe7`](https://github.com/monochange/monochange/commit/63cbbe7c06b03c0f1ed215a4fc61e0a74b50e1c4) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

> [!NOTE]
> _monochange_

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

> [!NOTE]
> _monochange_

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

> [!NOTE]
> _monochange_

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

> [!NOTE]
> _monochange_

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

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #258](https://github.com/monochange/monochange/pull/258) _Introduced in:_ [`d1fa746`](https://github.com/monochange/monochange/commit/d1fa7467bb8bc207939cbf10a907c5dc8fe725d4) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`3ed719e`](https://github.com/monochange/monochange/commit/3ed719e42d89d66b7db47528a69d1ecf1cdeada2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #240](https://github.com/monochange/monochange/pull/240) _Introduced in:_ [`63fbe0d`](https://github.com/monochange/monochange/commit/63fbe0de9825f3139386b7a25cf4821156813301) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### make manual trusted-publishing guidance more actionable

Improves CLI guidance for registries that still require manual trusted-publishing setup.

**Updated behavior:**

- manual trusted-publishing messages now point users to open the registry setup URL and match repository, workflow, and environment to the current GitHub context
- package-publish text and markdown output now include a concrete next step telling users to finish registry setup and rerun `mc publish`
- built-in publish prerequisite failures now tell users to complete registry setup and rerun the publish command

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #216](https://github.com/monochange/monochange/pull/216) _Introduced in:_ [`3ffb516`](https://github.com/monochange/monochange/commit/3ffb5165d643371be3315edf715a80b04f277144) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### improve trusted-publishing preflight diagnostics for manual registries

Improves trusted-publishing diagnostics for registries that still require manual setup.

**Updated behavior:**

- built-in publish preflight now validates the GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- manual-registry guidance now surfaces the resolved repository, workflow, and environment when monochange can infer them
- manual-registry errors now explain when the GitHub context is incomplete and point to the exact `publish.trusted_publishing.*` field that still needs configuration

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #218](https://github.com/monochange/monochange/pull/218) _Introduced in:_ [`85bc41f`](https://github.com/monochange/monochange/commit/85bc41f72766a34981e25cf1ad73442e9e80c267) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Testing

> [!NOTE]
> _monochange_

#### Fix CI race condition where tests that spawn `git` could fail under parallel `cargo llvm-cov` execution because skill command tests temporarily replace `PATH`. Capture the original `PATH` at process start and pass it explicitly to every git subprocess spawned by test helpers. Also reorder coverage job so Codecov uploads always complete before the patch threshold gate fails.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #262](https://github.com/monochange/monochange/pull/262) _Introduced in:_ [`184ab4f`](https://github.com/monochange/monochange/commit/184ab4fab3cf96f58b14f905a66511c6d0a469aa) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### add fixture-first integration coverage for manual trust diagnostics

Adds fixture-based CLI coverage for manual-registry trusted-publishing diagnostics.

The new integration tests cover:

- resolved GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- missing workflow configuration guidance when monochange cannot resolve the GitHub workflow yet
- placeholder-publish dry-run output in both text and JSON formats

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #221](https://github.com/monochange/monochange/pull/221) _Introduced in:_ [`c7a0209`](https://github.com/monochange/monochange/commit/c7a0209392b81f70b5d51b0b777db40487b8ac29) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

> [!NOTE]
> _monochange_

#### add trusted-publishing messaging test coverage

Adds regression coverage for trusted-publishing messaging in the `monochange` CLI and package-publish reporting.

The new tests cover:

- manual registry setup guidance rendering in text and markdown output
- preservation of explicit trusted-publishing context in manual-action outcomes

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #215](https://github.com/monochange/monochange/pull/215) _Introduced in:_ [`36c1d4e`](https://github.com/monochange/monochange/commit/36c1d4ec3c2daa675c233e388e161f339a77b6c2) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

## [0.1.0](https://github.com/monochange/monochange/releases/tag/v0.1.0) (2026-04-13)

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

> _Owner:_ Ifiok Jr. _Introduced in:_ [`4542b5a`](https://github.com/monochange/monochange/commit/4542b5aee8b63a86c7ffc0ea9436090162a18056)
