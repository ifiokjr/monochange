# Changelog

All notable changes to `monochange` will be documented in this file.

## [0.2.0](https://github.com/ifiokjr/monochange/releases/tag/v0.2.0) (2026-04-21)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #259](https://github.com/ifiokjr/monochange/pull/259) _Introduced in:_ [`c698d29`](https://github.com/ifiokjr/monochange/commit/c698d29ffabefab418a0a750a06de3b1d6074561) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #254](https://github.com/ifiokjr/monochange/pull/254) _Introduced in:_ [`1b2b412`](https://github.com/ifiokjr/monochange/commit/1b2b41237b194c608c3e521daefd3f15c2729f91) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #257](https://github.com/ifiokjr/monochange/pull/257) _Introduced in:_ [`0a6937a`](https://github.com/ifiokjr/monochange/commit/0a6937ac81dadbc6252e5eac60fd692335cee3a5) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add `caused_by` changeset context for dependency propagation

You can now annotate dependency-only follow-up changesets with `caused_by`, use `mc change --caused-by ...` to author them, inspect the linkage in diagnostics output, and suppress matching automatic dependency propagation during release planning.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #245](https://github.com/ifiokjr/monochange/pull/245) _Introduced in:_ [`8ec612b`](https://github.com/ifiokjr/monochange/commit/8ec612beb9a8b8100037435695826042bc7361c4) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#208](https://github.com/ifiokjr/monochange/issues/208)

#### Add advanced Dart workspace and Flutter lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #238](https://github.com/ifiokjr/monochange/pull/238) _Introduced in:_ [`acb4faa`](https://github.com/ifiokjr/monochange/commit/acb4faae2361ac75d01f20824ed85259ed5139a0) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#233](https://github.com/ifiokjr/monochange/issues/233)

#### add the first Dart lint suite foundation

monochange now wires Dart manifests into the ecosystem-owned lint registry and supports scaffolding Dart lint files with `mc lint new dart/<rule-name>`.

This foundation change adds:

- a new `monochange_dart::lints` module with target collection for `pubspec.yaml`
- Dart lint suite registration in the `mc lint` and `mc check` command paths
- Dart lint scaffolding support in `mc lint new`
- tests covering managed Dart lint target collection and fixture filtering

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #235](https://github.com/ifiokjr/monochange/pull/235) _Introduced in:_ [`5a7a4fe`](https://github.com/ifiokjr/monochange/commit/5a7a4fed84603f51dd5d152d11e739f30dea2b64) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#230](https://github.com/ifiokjr/monochange/issues/230)

#### Add the first Dart metadata and publishability lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #236](https://github.com/ifiokjr/monochange/pull/236) _Introduced in:_ [`f4669e3`](https://github.com/ifiokjr/monochange/commit/f4669e3110c68bb6384bc985e412822ce2a3ffe9) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#231](https://github.com/ifiokjr/monochange/issues/231)

#### Add Dart SDK constraint and dependency hygiene lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #237](https://github.com/ifiokjr/monochange/pull/237) _Introduced in:_ [`1192d15`](https://github.com/ifiokjr/monochange/commit/1192d1576156c89804373f1ea6b69f94d887f255) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#232](https://github.com/ifiokjr/monochange/issues/232)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #206](https://github.com/ifiokjr/monochange/pull/206) _Introduced in:_ [`a417022`](https://github.com/ifiokjr/monochange/commit/a417022f80f93d61add00b8087e0f80102a9fd52) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### Add a package-scoped `mc analyze` CLI command for release-aware semantic analysis.

The command defaults `--release-ref` to the most recent tag for the selected package or its owning version group, compares `main -> head` for first releases when no prior tag exists, and supports text or JSON output for package-focused review workflows.

`monochange_config` now reserves the built-in `analyze` command name so workspace CLI definitions cannot collide with the new built-in subcommand.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #256](https://github.com/ifiokjr/monochange/pull/256) _Introduced in:_ [`75e3329`](https://github.com/ifiokjr/monochange/commit/75e33295d17248f4daca6e7bc83988855a033a08) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #255](https://github.com/ifiokjr/monochange/pull/255) _Introduced in:_ [`26e13ff`](https://github.com/ifiokjr/monochange/commit/26e13fff071e93dc32fe071a5771232c980ebd46) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #247](https://github.com/ifiokjr/monochange/pull/247) _Introduced in:_ [`8c96c8f`](https://github.com/ifiokjr/monochange/commit/8c96c8f0a3b9d44bf30148b5a83067d7ce3ab62b) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#243](https://github.com/ifiokjr/monochange/issues/243) _Related issues:_ [#244](https://github.com/ifiokjr/monochange/issues/244)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #251](https://github.com/ifiokjr/monochange/pull/251) _Introduced in:_ [`47847db`](https://github.com/ifiokjr/monochange/commit/47847db5d8e98e9b8284e72e5f94c184473b4ffd) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add mc check command and lint rule integration

Add `mc check` CLI command that combines workspace validation with lint rule enforcement. Supports `--fix` for autofix, `--ecosystem` for filtering, and `--format` for output selection. The Validate step also runs lint rules and accepts a `fix` input. Includes the full `monochange_lint` crate with 5 Cargo and 6 NPM lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #207](https://github.com/ifiokjr/monochange/pull/207) _Introduced in:_ [`a650862`](https://github.com/ifiokjr/monochange/commit/a650862f2dc69b6538f6403bab5b66079f9c1304) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add lint include/exclude path filters with optional gitignore override

`[lints]` now supports workspace-level path filters for manifest linting.

**New options:**

- `include` — optional glob patterns that opt manifest paths into linting
- `exclude` — glob patterns that remove matching manifest paths from linting
- `disable_gitignore` — opt back into linting gitignored manifests when needed

By default, monochange now skips gitignored manifests during linting while still allowing repositories to explicitly opt them back in.

The repository config and bundled linting docs now show how to exclude `examples/**` when example manifests would otherwise trigger false positives.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #248](https://github.com/ifiokjr/monochange/pull/248) _Introduced in:_ [`b45cb78`](https://github.com/ifiokjr/monochange/commit/b45cb787a75746a95799f957f01d92020d27f72f) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #250](https://github.com/ifiokjr/monochange/pull/250) _Introduced in:_ [`0dd8460`](https://github.com/ifiokjr/monochange/commit/0dd846060614b2de9d3b2dfb5c1337075774b167) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#247](https://github.com/ifiokjr/monochange/issues/247), [#249](https://github.com/ifiokjr/monochange/issues/249)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #229](https://github.com/ifiokjr/monochange/pull/229) _Introduced in:_ [`f148184`](https://github.com/ifiokjr/monochange/commit/f148184d69fc4dc8720cde8db22768a8c1def8f7) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #225](https://github.com/ifiokjr/monochange/pull/225) _Introduced in:_ [`98013cb`](https://github.com/ifiokjr/monochange/commit/98013cb86d5644a7327dc2ee5803d747d4a0372c) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #220](https://github.com/ifiokjr/monochange/pull/220) _Introduced in:_ [`cf5e581`](https://github.com/ifiokjr/monochange/commit/cf5e58113adcda077dfff7c3dd8f5e7598e411d8) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#209](https://github.com/ifiokjr/monochange/issues/209)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #228](https://github.com/ifiokjr/monochange/pull/228) _Introduced in:_ [`94f06a0`](https://github.com/ifiokjr/monochange/commit/94f06a057150d26e5f330e2e49a08f71eb12fc92) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #240](https://github.com/ifiokjr/monochange/pull/240) _Introduced in:_ [`63fbe0d`](https://github.com/ifiokjr/monochange/commit/63fbe0de9825f3139386b7a25cf4821156813301) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### make manual trusted-publishing guidance more actionable

Improves CLI guidance for registries that still require manual trusted-publishing setup.

**Updated behavior:**

- manual trusted-publishing messages now point users to open the registry setup URL and match repository, workflow, and environment to the current GitHub context
- package-publish text and markdown output now include a concrete next step telling users to finish registry setup and rerun `mc publish`
- built-in publish prerequisite failures now tell users to complete registry setup and rerun the publish command

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #216](https://github.com/ifiokjr/monochange/pull/216) _Introduced in:_ [`3ffb516`](https://github.com/ifiokjr/monochange/commit/3ffb5165d643371be3315edf715a80b04f277144) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### improve trusted-publishing preflight diagnostics for manual registries

Improves trusted-publishing diagnostics for registries that still require manual setup.

**Updated behavior:**

- built-in publish preflight now validates the GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- manual-registry guidance now surfaces the resolved repository, workflow, and environment when monochange can infer them
- manual-registry errors now explain when the GitHub context is incomplete and point to the exact `publish.trusted_publishing.*` field that still needs configuration

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #218](https://github.com/ifiokjr/monochange/pull/218) _Introduced in:_ [`85bc41f`](https://github.com/ifiokjr/monochange/commit/85bc41f72766a34981e25cf1ad73442e9e80c267) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Refactor

#### Adopt `declare_lint_rule` for Cargo lint metadata and clarify when lint authors should use it.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #239](https://github.com/ifiokjr/monochange/pull/239) _Introduced in:_ [`f30f62f`](https://github.com/ifiokjr/monochange/commit/f30f62f26c5e846448a94b2aa08b5b84d147a27a) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#234](https://github.com/ifiokjr/monochange/issues/234)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/ifiokjr/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/ifiokjr/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Testing

#### Fix CI race condition where tests that spawn `git` could fail under parallel `cargo llvm-cov` execution because skill command tests temporarily replace `PATH`. Capture the original `PATH` at process start and pass it explicitly to every git subprocess spawned by test helpers. Also reorder coverage job so Codecov uploads always complete before the patch threshold gate fails.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #262](https://github.com/ifiokjr/monochange/pull/262) _Introduced in:_ [`184ab4f`](https://github.com/ifiokjr/monochange/commit/184ab4fab3cf96f58b14f905a66511c6d0a469aa) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add fixture-first integration coverage for manual trust diagnostics

Adds fixture-based CLI coverage for manual-registry trusted-publishing diagnostics.

The new integration tests cover:

- resolved GitHub trusted-publishing context for `crates.io`, `jsr`, and `pub.dev`
- missing workflow configuration guidance when monochange cannot resolve the GitHub workflow yet
- placeholder-publish dry-run output in both text and JSON formats

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #221](https://github.com/ifiokjr/monochange/pull/221) _Introduced in:_ [`c7a0209`](https://github.com/ifiokjr/monochange/commit/c7a0209392b81f70b5d51b0b777db40487b8ac29) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add trusted-publishing messaging test coverage

Adds regression coverage for trusted-publishing messaging in the `monochange` CLI and package-publish reporting.

The new tests cover:

- manual registry setup guidance rendering in text and markdown output
- preservation of explicit trusted-publishing context in manual-action outcomes

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #215](https://github.com/ifiokjr/monochange/pull/215) _Introduced in:_ [`36c1d4e`](https://github.com/ifiokjr/monochange/commit/36c1d4ec3c2daa675c233e388e161f339a77b6c2) _Last updated in:_ [`2bd10ab`](https://github.com/ifiokjr/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)
## [0.1.0](https://github.com/ifiokjr/monochange/releases/tag/v0.1.0) (2026-04-13)

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

> _Owner:_ Ifiok Jr. _Introduced in:_ [`4542b5a`](https://github.com/ifiokjr/monochange/commit/4542b5aee8b63a86c7ffc0ea9436090162a18056)

