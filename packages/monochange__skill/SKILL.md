---
name: monochange
description: Guides agents through monochange discovery, changesets, release planning, and provider-aware release workflows. Use when working on `monochange.toml`, `.changeset/*.md`, release automation, grouped versions, cross-ecosystem monorepo releases, CLI step composition, or MCP tool interactions.
---

# monochange

## Quick start

1. Read `monochange.toml` first — it is the single source of truth.
2. Run `mc validate` before making release-affecting edits.
3. Use `mc discover --format json` to inspect the workspace model.
4. Use `mc change` to write explicit release intent as `.changeset/*.md` files.
5. Use `mc release --dry-run --format json` before mutating release state.
6. Use `mc diagnostics --format json` to inspect changeset context and git provenance.

## Working rules

- Treat `monochange.toml` as the source of truth for packages, groups, source providers, ecosystems, and `[cli.<command>]` entries.
- Prefer configured package or group ids over guessing manifest names.
- Use `.changeset/*.md` files for explicit release intent — each targets one or more package/group ids with a bump severity, optional `type`, optional explicit `version`, and a human-readable summary.
- Use `caused_by` in changeset frontmatter when a dependent package is updating because of a dependency change — this provides context and replaces the automatic "dependency changed → patch" propagation.
- When `mc affected` flags a package that has no meaningful change, create a changeset with `bump: none` and `caused_by` listing the root cause package(s).
- Run dry-run flows before real release commands.
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

## CLI commands

| Command                  | Purpose                                                                |
| ------------------------ | ---------------------------------------------------------------------- |
| `mc init`                | Generate a starter `monochange.toml` from detected packages            |
| `mc populate`            | Append missing built-in CLI command definitions to config              |
| `mc validate`            | Validate config and changeset targets                                  |
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

## Guidance

See [REFERENCE.md](REFERENCE.md) for install steps, changeset authoring, grouped release rules, input types, step composition, and assistant setup guidance.
