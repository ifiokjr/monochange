# monochange reference

## What monochange is for

monochange manages versions and releases for monorepos that span more than one package ecosystem.

Use it when a repository needs one release-planning model across:

- Cargo
- npm / pnpm / Bun
- Deno
- Dart / Flutter

It discovers packages, normalizes dependency relationships, applies package and group rules from `monochange.toml`, reads explicit `.changeset/*.md` files, and turns those inputs into deterministic release plans.

## Installation

### CLI via npm

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

### CLI via Cargo

```bash
cargo install monochange
monochange --help
mc --help
```

### Skill package

```bash
npm install -g @monochange/skill
monochange-skill --print-install
monochange-skill --print-skill
monochange-skill --copy ~/.pi/agent/skills/monochange
```

## Skill map

Use the bundled docs like this:

- [SKILL.md](./SKILL.md) — concise entrypoint for agents
- [skills/README.md](./skills/README.md) — index of focused skill modules
- [skills/changesets.md](./skills/changesets.md) — creating and managing changesets
- [skills/commands.md](./skills/commands.md) — built-in commands and workflow selection
- [skills/configuration.md](./skills/configuration.md) — creating and evolving `monochange.toml`
- [skills/linting.md](./skills/linting.md) — `mc check`, `[lints]`, and manifest-focused rule explanations with examples
- [CHANGESET-GUIDE.md](./CHANGESET-GUIDE.md) — full lifecycle guidance
- [ARTIFACT-TYPES.md](./ARTIFACT-TYPES.md) — artifact-aware changeset framing

Keep this `REFERENCE.md` open when you want one longer document with broader examples and copy-paste snippets.

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

For release-oriented commands, markdown is the default human-readable output format. Use `--format json` for automation and `--diff` when you want file previews without mutating the workspace.

## Command catalog

### Bootstrap and validate

```bash
mc init
mc init --provider github
mc populate
mc validate
```

Use these when you are creating or refining `monochange.toml`.

- `mc init` generates a starter config from detected packages
- `mc init --provider github` also seeds source/provider automation config
- `mc populate` appends any missing built-in `[cli.<command>]` definitions to an existing config
- `mc validate` checks config shape and changeset targets before you do anything riskier

### Inspect the workspace model

```bash
mc discover --format json
mc diagnostics --format json
mc release-record --from HEAD --format json
```

Use these when you need facts before making changes.

- `mc discover --format json` gives you package ids, dependency edges, and groups
- `mc diagnostics --format json` adds changeset provenance, linked reviews, and related issues
- `mc release-record --from <ref>` inspects the durable release declaration stored in release history

### Create release intent

```bash
mc change --package monochange --bump minor --reason "add diagnostics command"
mc change --package monochange_config --bump none --caused-by monochange_core --reason "dependency-only follow-up"
mc affected --changed-paths crates/monochange/src/lib.rs --format json
```

Use these when you are deciding what should be released.

### Preview and apply releases

```bash
mc release --dry-run
mc release --dry-run --diff
mc release --dry-run --format json
mc commit-release --dry-run --diff
mc publish --dry-run --format json
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc placeholder-publish --dry-run --format json
mc repair-release --from v1.2.3 --target HEAD --dry-run
```

Use the dry-run forms first. They are the safest way to audit release behavior before mutating files, git state, registries, or provider releases.

### Assistant setup and MCP

```bash
mc assist pi
mc assist generic
mc assist claude --format json
mc mcp
```

Use `mc assist` when you need install instructions and client-specific MCP configuration. Use `mc mcp` when the client actually needs the stdio server process.

For a shorter command-only guide, see [skills/commands.md](./skills/commands.md).

## Changeset authoring

Changesets are markdown files in `.changeset/` with YAML frontmatter:

```markdown
---
core: minor
---

#### add release automation

Introduce automated release preparation with changelog rendering and version bumps.
```

Frontmatter keys are package or group ids. Values are bump severities (`none`, `patch`, `minor`, `major`) or configured change types. Object syntax supports `bump`, `version`, `type`, and `caused_by`:

```markdown
---
core:
  bump: major
  version: "2.0.0"
  type: security
---

#### breaking API change

Redesign the public API surface.
```

### Dependency propagation with `caused_by`

When a dependency changes, monochange automatically patches all dependents with no context. The `caused_by` field provides that context and suppresses the automatic propagation:

```markdown
---
monochange_config:
  bump: patch
  caused_by: ["monochange_core"]
---

#### update dependency on monochange_core

Bumps `monochange_core` dependency to v2.1.0 after the public API change to `ChangelogFormat`.
```

For packages flagged by `mc affected` that have no meaningful change, use `bump: none` with `caused_by`:

```markdown
---
monochange_config:
  bump: none
  caused_by: ["monochange_core"]
  type: deps
---

#### update monochange_core dependency

No user-facing changes. Dependency version updated to match the group release.
```

CLI flag: `mc change --package <id> --bump patch --caused-by monochange_core --reason "update dependency"`

### Granularity and lifecycle rules

Agent-authored changesets should follow package-centric granularity:

- review existing `.changeset/*.md` files before writing a new one
- choose the right lifecycle action for each note: create, update, replace, or remove
- keep related work together, but split unrelated features apart even when they land in the same package
- combine near-duplicate notes when multiple related packages changed for the same outward reason
- use `caused_by` when a package is only changing because of dependency propagation, and prefer `bump: none` when there is no real user-facing change
- treat breaking changes as separate, dedicated changesets with their own migration guides
- avoid cloned compatibility notes; if only the package ids change and the user-facing message stays the same, write one multi-package changeset

Use these decision rules:

- **Create new** when the feature is genuinely new or distinct from existing release notes
- **Update existing** only when the same feature expanded in scope
- **Replace** when the implementation changed enough that the old note is misleading
- **Remove** when the feature was reverted or replaced before release

Examples:

```markdown
# Good: separate changesets for distinct features in the same package

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

```markdown
# Good: combine similar package notes into one related multi-package changeset

---
github: patch
gitlab: none
gitea: none
hosting: none
---

#### align provider manifests with package publication metadata

...
```

```markdown
# Good: update an existing changeset when the same feature grows

---
cli: minor
---

#### add --verbose and --debug flags

Adds two related debugging flags:

- `--verbose` for progress detail
- `--debug` for internal timing and state output
```

```markdown
# Good: dedicate a separate changeset to a breaking change

---
config: major
---

#### rename `WorkflowDefinition` to `CliCommandDefinition`

> **Breaking change** — update imports and config references from `workflows` to `cli`.
```

Changeset content still needs the full user-facing quality bar:

- a clear headline
- an impact summary explaining why the change matters
- concrete before/after examples or realistic usage snippets

Use `mc diagnostics --format json` when you need changeset provenance and review context before deciding whether a note should be created, updated, or removed.

## CLI step types and composition

`monochange.toml` defines top-level CLI commands with `[cli.<command>]` entries. Each command has `help_text`, optional `inputs`, and ordered `steps`.

### Input types

| Type          | Description                            |
| ------------- | -------------------------------------- |
| `string`      | Single string value                    |
| `string_list` | Repeatable value (`--flag a --flag b`) |
| `path`        | File path value                        |
| `choice`      | Constrained to a set of `choices`      |
| `boolean`     | Boolean flag (`true`/`false`)          |

### Step types

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

### Command step features

`Command` steps support template interpolation with built-in variables (`{{ version }}`, `{{ group_version }}`, `{{ released_packages }}`, `{{ changed_files }}`, `{{ changesets }}`), CLI input forwarding via `{{ inputs.name }}`, and step output references via `{{ steps.ID.stdout }}`.

```toml
[cli.post-release]
help_text = "Release and run post-release commands"

[[cli.post-release.steps]]
type = "PrepareRelease"

[[cli.post-release.steps]]
type = "Command"
id = "notify"
command = "echo Released {{ version }}"
shell = true
```

`shell` accepts `true` (uses `sh -c`), a shell name like `"bash"`, or `false`/omitted for direct execution.

## Configuration reference

### Defaults

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
strict_version_conflicts = false
# package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"
```

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

### Versioned files

`versioned_files` update additional managed files beyond native manifests when versions change:

```toml
# package-scoped shorthand infers the package ecosystem
versioned_files = ["Cargo.toml"]
versioned_files = ["**/crates/*/Cargo.toml"]

# explicit typed entries
versioned_files = [{ path = "group.toml", type = "cargo", name = "sdk-core" }]

# regex entries update plain-text files (must include (?<version>...) capture)
versioned_files = [
	{ path = "README.md", regex = 'v(?<version>\d+\.\d+\.\d+)' },
]
```

### Lockfile commands

Lockfile refresh is command-driven. monochange infers defaults when not configured:

- Cargo: `cargo generate-lockfile`
- npm-family: detects owned lockfiles and runs the matching command (`npm install --package-lock-only`, `pnpm install --lockfile-only`, `bun install --lockfile-only`)
- Dart / Flutter: `dart pub get` or `flutter pub get`
- Deno: no inferred default

Explicit configuration overrides inference:

```toml
[ecosystems.npm]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
	{ command = "npm install --package-lock-only", cwd = "packages/legacy", shell = true },
]
```

### Package publishing and trusted publishing

Package publishing is separate from provider release publishing:

- `mc placeholder-publish` bootstraps missing registry packages with placeholder `0.0.0` releases
- `mc publish` runs built-in package-registry publishing for prepared release state
- `mc publish-release` publishes hosted/provider releases such as GitHub releases

Built-in package publishing currently supports only the canonical public registries:

- Cargo → `crates.io`
- npm packages → `npm`
- Deno packages → `jsr`
- Dart / Flutter packages → `pub.dev`

If a workspace uses `pnpm`, monochange uses `pnpm publish` and `pnpm exec npm trust ...` instead of raw `npm` commands so workspace protocol and catalog dependency handling stays aligned with the workspace manager.

Publishing is configured through `publish` on packages and ecosystems:

```toml
[ecosystems.npm.publish]
mode = "builtin"
trusted_publishing = true

[package.web.publish.placeholder]
readme_file = "docs/web-placeholder.md"
```

Placeholder README content can come from:

- `publish.placeholder.readme`
- `publish.placeholder.readme_file`

`trusted_publishing = true` tells monochange to manage or verify trusted publishing when supported.

- npm trusted publishing can be configured automatically from GitHub Actions context; monochange verifies the current state first, then runs `npm trust github <package> --repo <owner/repo> --file <workflow> [--env <environment>] --yes` or the `pnpm exec npm trust ...` equivalent for pnpm workspaces
- Cargo, `jsr`, and `pub.dev` currently require manual trusted-publishing setup; monochange reports the setup URL and blocks the next built-in release publish until trust is configured
- See [TRUSTED-PUBLISHING.md](TRUSTED-PUBLISHING.md) for a GitHub-focused setup guide covering the exact registry fields and commands for `npm`, `crates.io`, `jsr`, and `pub.dev`
- See [MULTI-PACKAGE-PUBLISHING.md](MULTI-PACKAGE-PUBLISHING.md) when one repository publishes multiple public packages and you need to choose between shared built-in flows and package-specific external workflows
- Built-in publishing does not yet manage registry rate-limit retries or delayed requeues; use `mode = "external"` when your workflow needs custom scheduling

### Lint rules

Configure ecosystem-specific manifest lint rules and run them through `mc check`:

```toml
[lints.rules]
"cargo/dependency-field-order" = "error"
"cargo/internal-dependency-workspace" = "error"
"cargo/required-package-fields" = "error"
"cargo/sorted-dependencies" = "error"
"cargo/unlisted-package-private" = "warning"

# npm rules also live under [lints.rules]
"npm/workspace-protocol" = "error"
"npm/sorted-dependencies" = "error"
"npm/required-package-fields" = "error"
"npm/root-no-prod-deps" = "error"
"npm/no-duplicate-dependencies" = "error"
"npm/unlisted-package-private" = "warning"
```

Rule configuration:

- Simple severity: `"rule-id" = "error"`, `"rule-id" = "warning"`, or `"rule-id" = "off"`
- Detailed config: `{ level = "error", fix = true, ...options }`

Today the built-in rule sets focus on Cargo and npm-family manifests.

Available Cargo lint rules:

- `cargo/dependency-field-order` — Enforces consistent field ordering in dependency specifications
- `cargo/internal-dependency-workspace` — Requires `workspace = true` for internal dependencies
- `cargo/required-package-fields` — Enforces required `[package]` fields
- `cargo/sorted-dependencies` — Requires alphabetically sorted dependency tables
- `cargo/unlisted-package-private` — Packages not in monochange.toml must be private

Available NPM lint rules:

- `npm/workspace-protocol` — Requires `workspace:` protocol for internal dependencies
- `npm/sorted-dependencies` — Requires alphabetically sorted dependencies
- `npm/required-package-fields` — Enforces required fields in package.json
- `npm/root-no-prod-deps` — Root package.json should only have devDependencies
- `npm/no-duplicate-dependencies` — Prevents duplicate dependencies across sections
- `npm/unlisted-package-private` — Packages not in monochange.toml must be private

Run `mc check` to validate and lint. Use `mc check --fix` to auto-fix where possible.

### Groups

Groups synchronize versions across packages:

```toml
[group.sdk]
packages = ["sdk-core", "web-sdk"]
tag = true
release = true
version_format = "primary"

[group.sdk.changelog]
path = "changelog.md"
include = ["sdk-cli"]
```

### Changeset verification

```toml
[changesets.verify]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
```

## Linting and validation reference

monochange's linting docs in this skill package are about **manifest lint rules configured in `monochange.toml` and run via `mc check`**.

Use this workflow when editing package manifests or lint configuration:

```bash
mc validate
mc check
mc check --fix
```

If you edited shared docs in `.templates/`, also run:

```bash
devenv shell docs:check
```

For the full rule-by-rule explanation — including the available `[lints]` rules, why you would enable them, and examples of what changes with and without them — see [skills/linting.md](./skills/linting.md).

## Important modeling rules

- `monochange.toml` is the source of truth.
- Groups own outward release identity for their member packages.
- Package changelogs and package versioned files may still apply even when a group owns versioning.
- Changesets should reference configured package ids or group ids.
- Prefer package ids over group ids when the change is package-specific — monochange propagates to dependents and groups automatically.
- Source-provider release publishing is downstream from prepared release data, not a substitute for planning.
- Built-in package publishing currently supports public registries only. Use `mode = "external"` for private or custom registries.
- Changelog version headings now include the release date by default. Set `changelog_version_title = "{{ version }}"` to restore the previous format.

## MCP server

The MCP server exposes reviewable, JSON-first tools for workspace inspection and release planning:

<!-- {=mcpToolsList} -->

- `monochange_validate` — validate `monochange.toml` and `.changeset` targets
- `monochange_discover` — discover packages, dependencies, and groups across the repository
- `monochange_change` — write a `.changeset` markdown file for one or more package or group ids
- `monochange_release_preview` — prepare a dry-run release preview from discovered `.changeset` files
- `monochange_release_manifest` — generate a dry-run release manifest JSON document for downstream automation
- `monochange_affected_packages` — evaluate changeset policy from changed paths and optional labels

<!-- {/mcpToolsList} -->

### MCP configuration

<!-- {=mcpConfigSnippet} -->

```json
{
	"mcpServers": {
		"monochange": {
			"command": "monochange",
			"args": ["mcp"]
		}
	}
}
```

<!-- {/mcpConfigSnippet} -->

Start the server manually: `mc mcp`

Print assistant-specific setup guidance: `mc assist claude`, `mc assist generic`, `mc assist pi`

## Repo-local guidance for assistants

<!-- {=assistantRepoGuidance} -->

- Read `monochange.toml` before proposing release workflow changes.
- Run `mc validate` before and after release-affecting edits.
- Use `mc discover --format json` to inspect package ids, group ownership, and dependency edges.
- Use `mc diagnostics --format json` for a structured view of all pending changesets with git and review context.
- Prefer `mc change` plus `.changeset/*.md` files over ad hoc release notes.
- Use `mc release --dry-run --format json` before mutating release state.

<!-- {/assistantRepoGuidance} -->

When you need full changeset context — introduced commit, linked PR, related issues — use `mc diagnostics --format json` directly. It returns stable workspace-relative paths and structured records that agents can parse without reading raw markdown files.
