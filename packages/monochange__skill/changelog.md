## @monochange/skill [0.1.0](https://github.com/monochange/monochange/releases/tag/@monochange/skill/v0.1.0) (2026-04-13)

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

## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-26)

### Changed

#### Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### Added

#### add lint include/exclude path filters with optional gitignore override

`[lints]` now supports workspace-level path filters for manifest linting.

**New options:**

- `include` — optional glob patterns that opt manifest paths into linting
- `exclude` — glob patterns that remove matching manifest paths from linting
- `disable_gitignore` — opt back into linting gitignored manifests when needed

By default, monochange now skips gitignored manifests during linting while still allowing repositories to explicitly opt them back in.

The repository config and bundled linting docs now show how to exclude `examples/**` when example manifests would otherwise trigger false positives.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #248](https://github.com/monochange/monochange/pull/248) _Introduced in:_ [`b45cb78`](https://github.com/monochange/monochange/commit/b45cb787a75746a95799f957f01d92020d27f72f) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

#### static npm packages in packages/ directory

All npm packages now live as static directories under `packages/` instead of being dynamically generated during the release workflow.

**Before:**

The `@monochange/cli` and platform packages were generated on-the-fly by `build-packages.mjs` into a temporary directory, then published from there. `@monochange/skill` lived in `npm/skill`.

**After:**

Package directories are permanently present under `packages/` using the `@scope__name` convention:

```
packages/monochange__cli/              # @monochange/cli
packages/monochange__cli-darwin-arm64/  # @monochange/cli-darwin-arm64
packages/monochange__skill/            # @monochange/skill
...
```

`build-packages.mjs` still runs during release to populate platform binaries into `packages/*/bin/`, but it no longer generates the package structure from scratch. `publish-packages.mjs` now validates that each package has the expected binaries before publishing, preventing accidental empty publishes.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #204](https://github.com/monochange/monochange/pull/204) _Introduced in:_ [`a90638b`](https://github.com/monochange/monochange/commit/a90638b911d0aca00afcda8c5686da46ead14831) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Documentation

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

#### add changeset cleanup job guide with mc diagnostics workflow

Adds a comprehensive "Changeset cleanup job" section to `skills/changesets.md` that teaches agents how to audit, deduplicate, and clean up changesets before release using `mc diagnostics --format json`.

**New workflow includes:**

- Step-by-step guide using `mc diagnostics --format json` to export changeset data
- jq filter examples for finding duplicates, short summaries, missing git context
- Decision matrix for when to merge, remove, or update changesets
- Concrete bash examples for merging duplicate changesets
- Validation checklist for pre-release changeset hygiene

Updates the root `SKILL.md` reference to highlight "auditing, cleaning up" alongside creation.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #242](https://github.com/monochange/monochange/pull/242) _Introduced in:_ [`7342723`](https://github.com/monochange/monochange/commit/7342723dc924b9fd4dd0cdf9ca34da9812e83b70) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add artifact-type-aware changeset guides and skill package expansion

Introduces three new documents to the skill package and shared mdt template blocks for changeset generation rules:

- **CHANGESET-GUIDE.md** — lifecycle management guide covering create, update, replace, and remove workflows with decision matrix
- **ARTIFACT-TYPES.md** — per-type rules for libraries, applications, CLI tools, and LSP/MCP servers, including UX changelog section configuration and screenshot support
- **`.templates/changeset.t.md`** — shared mdt template blocks for changeset philosophy, artifact tables, lifecycle rules, granularity rules, templates, and MCP tool integration

Key additions:

- **UX changelog section** (`type: ux`) for applications and websites, with S3-compatible screenshot upload configuration
- **LSP/MCP artifact type** added to the artifact type table with protocol-focused changeset guidance
- **`caused_by` frontmatter field** documented for dependency propagation context (replaces automatic "dependency changed → patch" with human-readable explanation)
- **`bump: none` with `caused_by`** workflow for `mc affected` packages with no meaningful changes
- Shared blocks propagate to `SKILL.md`, `REFERENCE.md`, and `docs/agents/changeset-generation.md` via `mdt`

**Before:** Skill package had only `SKILL.md` and `REFERENCE.md` with no artifact-type differentiation or lifecycle management guidance.

**After:** Agents can follow per-type rules, manage changeset lifecycles, configure UX sections with screenshots, and provide dependency propagation context.

> _Owner:_ Ifiok Jr. _Introduced in:_ [`36bb233`](https://github.com/monochange/monochange/commit/36bb2338f182c271679bca1ad14bd3a48bbf5f71) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

#### add lint rule documentation

Document the mc check command and all available lint rules for Cargo and NPM ecosystems in SKILL.md and REFERENCE.md.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #207](https://github.com/monochange/monochange/pull/207) _Introduced in:_ [`a650862`](https://github.com/monochange/monochange/commit/a650862f2dc69b6538f6403bab5b66079f9c1304) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### reorganize bundled skill docs into lowercased skill paths

Moves the bundled deep-dive markdown files under `skills/` and lowercases the published markdown filenames so the installable skill has a more consistent layout.

**Updated structure includes:**

- top-level `SKILL.md` remains the entrypoint
- deep-dive guides such as `reference.md`, `changeset-guide.md`, `artifact-types.md`, `trusted-publishing.md`, and `multi-package-publishing.md` now live in `skills/`
- package and example readme files now use lowercase `readme.md`
- internal links and published package file paths were updated to match the new structure

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #248](https://github.com/monochange/monochange/pull/248) _Introduced in:_ [`b45cb78`](https://github.com/monochange/monochange/commit/b45cb787a75746a95799f957f01d92020d27f72f) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### document future automation boundaries for manual registries

Adds a short roadmap-style section to the trusted-publishing docs describing where monochange may add stronger automation or validation later for `crates.io`, `jsr`, and `pub.dev`.

It also makes the current boundary explicit:

- npm is still the only registry with built-in trusted-publishing enrollment
- manual registries remain guidance- and diagnostics-first today
- registry-side admin or browser-confirmed steps are still treated as manual unless the registry exposes a safer automation path later

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #223](https://github.com/monochange/monochange/pull/223) _Introduced in:_ [`ed2ae40`](https://github.com/monochange/monochange/commit/ed2ae4009e05a761d7abf24b22b65af7415912bc) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

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

#### prefer official trusted-publishing workflows in the packaged skill

The packaged skill now explicitly recommends the registry-maintained GitHub publishing workflows for manual trusted-publishing registries.

**Updated guidance:**

- prefers `rust-lang/crates-io-auth-action@v1` for `crates.io`
- prefers `dart-lang/setup-dart/.github/workflows/publish.yml@v1` for `pub.dev`
- clarifies that `mode = "external"` is often the clearest fit when those workflows should own the publish command directly

These recommendations were added to the main skill entrypoint, the configuration deep dive, and the packaged trusted-publishing guide.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #214](https://github.com/monochange/monochange/pull/214) _Introduced in:_ [`8a8a13a`](https://github.com/monochange/monochange/commit/8a8a13a09520f7549ae15204f69bf1e9357d1662) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add an adoption playbook and example indexes to the packaged skill

The packaged `@monochange/skill` now includes an interactive adoption guide plus bundled example indexes for choosing how deeply to set up monochange.

**Before:** The skill explained commands, configuration, linting, and publishing, but it did not give agents a clear question tree for quickstart vs standard vs full vs migration adoption. It also lacked a dedicated examples surface for pointing users at setup patterns.

**After:** The package adds `skills/adoption.md`, a bundled `examples/` folder with condensed scenario summaries, and references to a top-level repository `examples/` index for fuller repo-shaped setups.

This makes the skill better at plan-mode interrogation, migration guidance, and recommendation-driven setup conversations.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #241](https://github.com/monochange/monochange/pull/241) _Introduced in:_ [`05c393c`](https://github.com/monochange/monochange/commit/05c393c72ce88b4c4e1eee99858e33ae72554559) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add granular changeset generation guidance to the skill package

The packaged `@monochange/skill` guidance now teaches agents how to manage changesets as a lifecycle instead of only telling them to create one.

**Before:**

The skill told agents to use `mc change` and `mc diagnostics`, but it did not explain when to create a new changeset versus updating, replacing, or removing an existing one. It also did not make granular package-centric changesets a first-class rule.

**After:**

Agents are now instructed to:

- review existing `.changeset/*.md` files before writing a new one
- keep changesets package-centric and split unrelated features apart
- combine near-duplicate changesets when the outward change is the same across multiple related packages
- update an existing changeset only when the same feature expands in scope
- remove stale changesets when a feature is reverted before release
- dedicate separate changesets to breaking changes with migration guidance

**Skill guidance example:**

```markdown
# Separate unrelated features

---
core: minor
---

#### add file diff preview

...

---
core: minor
---

#### add changelog format detection

...
```

This makes the packaged skill better aligned with monochange's current agent rules for granular, user-facing release notes.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #205](https://github.com/monochange/monochange/pull/205) _Introduced in:_ [`467fdff`](https://github.com/monochange/monochange/commit/467fdff63ea036ffc0f38f18a62d23723f007740) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add modular skill docs and a full linting guide

The packaged `@monochange/skill` docs now split the agent guidance into focused deep dives while keeping `REFERENCE.md` at the top level as the high-context reference document.

**Before:**

The package centered on `SKILL.md` plus a few top-level docs, but it did not have a dedicated `skills/` folder for focused topics and it did not explain the current workspace lint policy rule by rule.

**After:**

The package now includes:

- `skills/changesets.md` for creating and managing `.changeset/*.md` files
- `skills/commands.md` for choosing the right `mc` command and command flow
- `skills/configuration.md` for creating and extending `monochange.toml`
- `skills/linting.md` for the current rust/clippy rules, why they exist, and what changes with and without them
- updated `SKILL.md` and `REFERENCE.md` links so agents can jump between the concise entrypoint and the deeper reference material

**Skill bundle example:**

```text
SKILL.md
REFERENCE.md
skills/
  README.md
  changesets.md
  commands.md
  configuration.md
  linting.md
```

This makes the published skill package easier to load incrementally while giving agents a much denser reference surface for current monochange features.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #210](https://github.com/monochange/monochange/pull/210) _Introduced in:_ [`63de80c`](https://github.com/monochange/monochange/commit/63de80c29e46e88271b5dfe91bbf074a6e4c6135) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#209](https://github.com/monochange/monochange/issues/209)

#### add a multi-package publishing guide to the packaged skill

The packaged skill now includes a dedicated `MULTI-PACKAGE-PUBLISHING.md` guide for repositories that publish multiple public packages from one workspace.

It explains:

- when one shared post-merge `mc publish` job is a good fit
- when package-specific jobs or fully external workflows are clearer
- how to keep tags, workflows, environments, and working directories aligned per package
- when to use package-level publishing overrides in `monochange.toml`

The skill `README.md`, `SKILL.md`, `REFERENCE.md`, and `skills/configuration.md` now point agents to the new guide when publishing strategy depends on monorepo shape rather than only on per-registry trusted-publishing setup.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #219](https://github.com/monochange/monochange/pull/219) _Introduced in:_ [`df07da7`](https://github.com/monochange/monochange/commit/df07da727967a2ef83f1995197c2159024fb46ad) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add trusted publishing setup guidance for supported registries

The packaged skill now ships a dedicated `TRUSTED-PUBLISHING.md` guide for setting up GitHub-based trusted publishing / OIDC flows across the registries that monochange supports.

**Before:** The skill explained that `publish.trusted_publishing = true` existed, but it did not show the exact registry fields or commands needed to finish setup.

**After:** The package now includes step-by-step guidance for:

- `npm` trusted publishing, including the exact `npm trust github ...` and `pnpm exec npm trust ...` commands that monochange models
- `crates.io` trusted publishing fields and the `rust-lang/crates-io-auth-action@v1` workflow pattern
- `jsr` repository linking and GitHub Actions publishing
- `pub.dev` automated publishing with repository and tag-pattern requirements

The skill README, `SKILL.md`, and `REFERENCE.md` also point agents to the new guide when they need secure package-publishing setup details.

The mdBook user guide now mirrors that content in a dedicated trusted-publishing chapter so the same setup guidance is available in both the packaged skill and the docs site.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #211](https://github.com/monochange/monochange/pull/211) _Introduced in:_ [`38fe09f`](https://github.com/monochange/monochange/commit/38fe09f69f31ab268d7adc37889dacb80bfba2b7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### refresh crates.io and pub.dev trusted publishing guidance

The packaged trusted-publishing guide now includes more complete GitHub/OIDC setup details for `crates.io` and `pub.dev`.

**Updated guidance includes:**

- crates.io prerequisites, workflow filename handling, environment matching, and the `rust-lang/crates-io-auth-action@v1` release-job pattern
- crates.io notes about the short-lived publish token flow and first-publish bootstrap requirements
- pub.dev prerequisites, tag-push-only requirements, recommended reusable `dart-lang/setup-dart` workflow usage, optional GitHub environment hardening, and multi-package repository guidance

The mdBook trusted-publishing chapter was updated to mirror the same information.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #212](https://github.com/monochange/monochange/pull/212) _Introduced in:_ [`c371be8`](https://github.com/monochange/monochange/commit/c371be8864bc87c7454da4471b2a31d53089bb3d) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Changed

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

#### Add Dart SDK constraint and dependency hygiene lint rules.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #237](https://github.com/monochange/monochange/pull/237) _Introduced in:_ [`1192d15`](https://github.com/monochange/monochange/commit/1192d1576156c89804373f1ea6b69f94d887f255) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#232](https://github.com/monochange/monochange/issues/232)

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
