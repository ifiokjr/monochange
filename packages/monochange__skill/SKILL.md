---
name: monochange
description: Guides agents through monochange discovery, changesets, release planning, and provider-aware release workflows. Use when working on `monochange.toml`, `.changeset/*.md`, release automation, grouped versions, cross-ecosystem monorepo releases, CLI step composition, MCP tool interactions, or linting configuration.
---

# monochange

## Quick start

1. If `monochange.toml` does not exist yet, run `mc init`. If it exists but you want editable built-in command definitions, run `mc populate`.
2. Read `monochange.toml` first — it is the single source of truth.
3. Run `mc validate` before making release-affecting edits.
4. Use `mc discover --format json` to inspect the workspace model.
5. Use `mc change` to write explicit release intent as `.changeset/*.md` files.
6. Use `mc diagnostics --format json` to inspect changeset context and git provenance.
7. Use `mc lint` to check package manifests for consistency and best practices.
8. Use `mc release --dry-run --format json` or `mc release --dry-run --diff` before mutating release state.

## Working rules

- Treat `monochange.toml` as the source of truth for packages, groups, source providers, ecosystems, `[cli.<command>]` entries, and lint configuration.
- Prefer configured package or group ids over guessing manifest names.
- Use `.changeset/*.md` files for explicit release intent — each targets one or more package/group ids with a bump severity, optional `type`, optional explicit `version`, and a human-readable summary.
- Use `caused_by` in changeset frontmatter when a dependent package is updating because of a dependency change — this provides context and replaces the automatic "dependency changed → patch" propagation.
- When `mc affected` flags a package that has no meaningful change, create a changeset with `bump: none` and `caused_by` listing the root cause package(s).
- Review existing `.changeset/*.md` files before creating a new one so you can decide whether the right lifecycle action is create, update, replace, or remove.
- Keep changesets package-centric and granular. Distinct features should get distinct changesets even when they land in the same package; only expand an existing changeset when the new work is clearly the same feature growing in scope.
- Combine near-duplicate changesets when the outward change is the same across multiple related packages. Do not emit cloned compatibility notes that differ only by package name.
- Breaking changes must always get their own dedicated changeset with a migration path instead of being bundled into a broader feature note.
- Run dry-run flows before real release commands.
- Run `mc lint` before releases to catch manifest inconsistencies early.
- Keep docs, templates, and changelog behavior aligned with config changes.
- Use `mc diagnostics --format json` to audit changesets before release — it shows git provenance, linked PRs, related issues, and introduced/last-updated commits.

## Recommended command flow

<!-- {=recommendedCommandFlow} -->

1. **Validate** — `mc validate` checks config and changeset targets.
2. **Discover** — `mc discover --format json` inspects the workspace model.
3. **Create changesets** — `mc change --package <id> --bump <severity> --reason "..."` writes explicit release intent.
4. **Preview release** — `mc release --dry-run --format json` shows planned bumps, changelog output, and changed files.
5. **Inspect changeset context** — `mc diagnostics --format json` shows git provenance and linked review metadata for all pending changesets.
6. **Inspect cached manifest** — `mc release --dry-run --format json` refreshes the cached manifest and shows the downstream automation payload.
7. **Publish** — `mc publish-release --format json` creates provider releases after human review.

<!-- {/recommendedCommandFlow} -->

Release-oriented commands default to markdown output. Use `--format json` for automation and `--diff` when you want unified file previews without mutating the workspace.

## Deep dives

- [REFERENCE.md](REFERENCE.md) — high-context reference with more examples
- [skills/README.md](skills/README.md) — index of focused skill modules
- [skills/changesets.md](skills/changesets.md) — creating and managing changesets
- [skills/commands.md](skills/commands.md) — built-in commands and workflow selection
- [skills/configuration.md](skills/configuration.md) — creating and extending `monochange.toml`
- [skills/linting.md](skills/linting.md) — current lint rules, rationale, and examples
- [CHANGESET-GUIDE.md](CHANGESET-GUIDE.md) — full lifecycle guidance
- [ARTIFACT-TYPES.md](ARTIFACT-TYPES.md) — package-type-specific release-note guidance

## CLI commands

| Command                  | Purpose                                                                |
| ------------------------ | ---------------------------------------------------------------------- |
| `mc init`                | Generate a starter `monochange.toml` from detected packages            |
| `mc populate`            | Append missing built-in CLI command definitions to config              |
| `mc validate`            | Validate config and changeset targets                                  |
| `mc check`               | Validate config and run lint rules against package manifests           |
| `mc discover`            | Discover packages across ecosystems                                    |
| `mc change`              | Create a `.changeset/*.md` file                                        |
| `mc release`             | Prepare a release plan from changesets and refresh the cached manifest |
| `mc placeholder-publish` | Publish placeholder versions for packages missing from registries      |
| `mc publish`             | Publish package artifacts using built-in registry workflows            |
| `mc commit-release`      | Prepare a release and create a local commit                            |
| `mc publish-release`     | Create provider releases                                               |
| `mc release-pr`          | Open or update a release pull request                                  |
| `mc affected`            | Evaluate changeset policy from changed paths                           |
| `mc diagnostics`         | Show changeset context with git and review metadata                    |
| `mc release-record`      | Inspect a durable release declaration from git history                 |
| `mc repair-release`      | Repair a recent release by retargeting tags                            |
| `mc assist`              | Print assistant install and MCP setup guidance                         |
| `mc mcp`                 | Start the stdio MCP server                                             |

## CLI step types

<!-- {=cliStepTypes} -->

**Standalone steps** (no prerequisites):

- `Validate` — validate config and changeset targets
- `Discover` — discover packages across ecosystems
- `CreateChangeFile` — write a `.changeset/*.md` file
- `AffectedPackages` — evaluate changeset policy from CI-supplied paths and labels
- `DiagnoseChangesets` — show changeset context and review metadata
- `RetargetRelease` — repair a recent release by moving its tags

**Release-state consumer steps** (require `PrepareRelease`):

- `PrepareRelease` — compute release plan, update versions, changelogs, and versioned files
- `CommitRelease` — create a local release commit
- `PublishRelease` — create provider releases
- `OpenReleaseRequest` — open or update a release pull request
- `CommentReleasedIssues` — comment on issues referenced in changesets

**Generic step:**

- `Command` — run an arbitrary shell command with template interpolation

<!-- {/cliStepTypes} -->

## MCP tools

<!-- {=mcpToolsList} -->

- `monochange_validate` — validate `monochange.toml` and `.changeset` targets
- `monochange_discover` — discover packages, dependencies, and groups across the repository
- `monochange_change` — write a `.changeset` markdown file for one or more package or group ids
- `monochange_release_preview` — prepare a dry-run release preview from discovered `.changeset` files
- `monochange_release_manifest` — generate a dry-run release manifest JSON document for downstream automation
- `monochange_affected_packages` — evaluate changeset policy from changed paths and optional labels

<!-- {/mcpToolsList} -->

## Key configuration concepts

### Changeset generation

- Start by checking existing `.changeset/*.md` files and the current diff before you write anything new.
- Prefer one package per changeset unless a configured group is the real outward release boundary.
- Keep related changes together, but split unrelated features apart even if they touch the same package.
- If a package is only changing because of dependency propagation, prefer `caused_by` frontmatter and use `bump: none` when there is no real user-facing change.
- If multiple packages changed for the same reason and the release note would otherwise be nearly identical, combine them into one multi-package changeset instead of cloning the same body four times.
- Update an existing changeset only when the new work is clearly the same feature expanding in scope.
- Remove stale changesets when the feature was reverted or replaced before release.
- Breaking changes always get their own dedicated changeset with a migration guide.
- Changeset bodies still need the user-facing quality bar: headline, impact summary, and concrete before/after or usage examples.

### Versioned files

`versioned_files` update additional managed files beyond native manifests when versions change. Three forms:

- **Package-scoped shorthand**: `versioned_files = ["Cargo.toml"]` — infers the package ecosystem
- **Explicit typed entries**: `versioned_files = [{ path = "group.toml", type = "cargo" }]`
- **Regex entries**: `versioned_files = [{ path = "README.md", regex = 'v(?<version>\d+\.\d+\.\d+)' }]` — for plain-text files; must include a named `version` capture

### Lockfile commands

Lockfile refresh is command-driven via `[ecosystems.<name>].lockfile_commands`. monochange infers sensible defaults for Cargo, npm-family, and Dart/Flutter. Explicit configuration overrides inference.

### Publishing and trust

- Package publishing is configured through `publish` on packages and ecosystems.
- Built-in publishing currently supports only the canonical public registries: `crates.io`, `npm`, `jsr`, and `pub.dev`.
- `mc placeholder-publish` exists for first-release bootstrap. It checks whether each managed package already exists in its registry and publishes a placeholder `0.0.0` version only for the missing ones.
- Placeholder README content can come from `publish.placeholder.readme` or `publish.placeholder.readme_file`.
- `publish.trusted_publishing = true` tells monochange to manage or verify trusted publishing for that package when supported.
- npm trusted publishing can be configured automatically from GitHub Actions context. pnpm workspaces use `pnpm exec npm trust ...` and `pnpm publish`.
- Cargo, `jsr`, and `pub.dev` currently require manual trusted-publishing setup. monochange reports the setup URL and blocks the next built-in release publish until trust is configured.
- Built-in publishing does not yet manage registry rate-limit retries or delayed requeues. Use `mode = "external"` if your workflow needs custom scheduling.

### Release titles

<!-- {=releaseTitleConfig} -->

Two template fields control how release names and changelog version headings render:

- **`release_title`** — plain text title for provider releases (GitHub, GitLab, Gitea)
- **`changelog_version_title`** — markdown-capable title for changelog version headings

Both are configurable at `[defaults]`, `[package.*]`, and `[group.*]` levels.

Available template variables: `{{ version }}`, `{{ id }}`, `{{ date }}`, `{{ time }}`, `{{ datetime }}`, `{{ changes_count }}`, `{{ tag_url }}`, `{{ compare_url }}`.

```toml
[defaults]
release_title = "{{ version }} ({{ date }})"
changelog_version_title = "[{{ version }}]({{ tag_url }}) ({{ date }})"

[group.main]
release_title = "v{{ version }} — released {{ date }}"
```

<!-- {/releaseTitleConfig} -->

### Groups and changelog filtering

Groups synchronize versions across packages. Group changelogs can filter included entries:

- `include = "all"` — all member changesets (default)
- `include = "group-only"` — only direct group-targeted changesets
- `include = ["package-id"]` — specific member changesets plus group-targeted ones

## Changeset lifecycle

**Changesets must be actively managed, not just created.** Before writing a new changeset:

1. Read all existing `.changeset/*.md` files to understand current coverage
2. Determine the right action: **create new**, **update existing**, or **remove stale**
3. Choose bump level and section type based on artifact type (library, application, CLI, LSP/MCP)
4. Validate with `mc validate` or `mc diagnostics --format json`

See [CHANGESET-GUIDE.md](CHANGESET-GUIDE.md) for the full lifecycle management guide.

## Artifact types

Different package types have different user-facing boundaries. Libraries expose APIs, applications expose UI, CLI tools expose commands, and LSP/MCP servers expose protocols. Changeset content, bump levels, and section types should adapt accordingly.

Applications and websites should use the `ux` changelog section type for visual and interaction changes, with screenshots when configured.

See [ARTIFACT-TYPES.md](ARTIFACT-TYPES.md) for per-type rules, templates, examples, and configuration.

### Lint configuration

Configure ecosystem-specific lint rules in `monochange.toml`:

```toml
[ecosystems.cargo.lints]
"cargo/dependency-field-order" = "error"
"cargo/internal-dependency-workspace" = "error"
"cargo/required-package-fields" = "error"
"cargo/sorted-dependencies" = "error"
"cargo/unlisted-package-private" = "warning"

[ecosystems.npm.lints]
"npm/workspace-protocol" = "error"
"npm/sorted-dependencies" = "error"
"npm/required-package-fields" = "error"
"npm/root-no-prod-deps" = "error"
"npm/no-duplicate-dependencies" = "error"
"npm/unlisted-package-private" = "warning"
```

Each rule can be configured as:

- Simple severity: `"rule-id" = "error"`, `"rule-id" = "warning"`, or `"rule-id" = "off"`
- Detailed config: `{ level = "error", ...rule_specific_options }`

Use `mc check --fix` to auto-fix issues where possible.

## Guidance

Start with [REFERENCE.md](REFERENCE.md) for the broad reference. Then open the focused deep dives in [skills/README.md](skills/README.md) when you need dedicated guidance for changesets, commands, configuration, or linting.
