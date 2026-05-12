# monochange.toml configuration

`monochange.toml` is the source of truth for package ids, group release identities, changelog rendering, versioned files, source providers, lint rules, and custom CLI workflows.

Read it before editing changesets or suggesting commands. The same repository can mix package ecosystems, use group-level versions, hide private packages from release planning, and expose custom CLI workflows that do not exist anywhere else.

## Minimal package configuration

```toml
[defaults]
parent_bump = "patch"
include_private = false
package_type = "npm"

[package."@acme/api"]
path = "packages/api"

[package."@acme/ui"]
path = "packages/ui"

[ecosystems.npm]
enabled = true
```

The minimal shape usually has defaults, package tables, and enabled ecosystems. `parent_bump` controls how dependency changes propagate, `include_private` decides whether private packages are included by default, and `package_type` supplies a default ecosystem for package tables that do not declare one.

Use explicit `type` on packages when the repo mixes ecosystems:

```toml
[package."@acme/ui"]
path = "packages/ui"
type = "npm"

[package.acme_core]
path = "crates/acme_core"
type = "cargo"

[package.acme_cli]
path = "crates/acme_cli"
type = "cargo"
```

Supported ecosystem/package types in current code are `cargo`, `npm`, `deno`, `dart`, `flutter`, `python`, and `go`.

## Grouped versions

Groups make multiple packages share one outward version and release identity.

```toml
[group.sdk]
packages = ["@acme/api", "@acme/ui"]
tag = true
release = true
version_format = "primary"
changelog = { path = "CHANGELOG.md", format = "keep_a_changelog", include = "all" }
```

Use package ids in changesets when a specific package changed. Use the group id only when the change is intentionally group-owned.

Groups are best for products released as a unit: SDKs made of several packages, plugins that must stay version-aligned, or cross-language distributions that share one public changelog. Keep unrelated packages out of a group even if they live in the same workspace, because a group turns multiple package releases into one outward release identity.

## Versioned files

`PrepareRelease` updates native manifests and configured `versioned_files`.

```toml
[package.acme_core]
path = "crates/acme_core"
type = "cargo"
versioned_files = ["Cargo.toml"]

[group.sdk]
packages = ["@acme/api", "@acme/ui"]
versioned_files = [
	{ path = "package.json", type = "npm" },
	{ path = "README.md", regex = 'acme-sdk@(?<version>\\d+\\.\\d+\\.\\d+)' },
]
```

String entries infer the package ecosystem when they appear under `[package.*]`. Group entries should be explicit because a group can span ecosystems.

Use regex entries for docs, install snippets, generated metadata, or examples that are not native package manifests. The regex must include a named `version` capture so monochange knows exactly which portion to replace.

## Ecosystem settings

```toml
[ecosystems.cargo]
enabled = true
versioned_files = ["Cargo.toml"]
lockfile_commands = ["cargo generate-lockfile"]

[ecosystems.npm]
enabled = true
lockfile_commands = ["pnpm install --lockfile-only"]
```

Use ecosystem `publish` defaults when most packages share the same publishing behavior, and override at `[package.*].publish` when needed.

Lockfile commands are command-driven. Configure them when the repository has a preferred package manager or when inferred defaults would update the wrong files. They normally run as part of a release workflow after versions are prepared and before the release commit is created.

## Publishing settings

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true

[package."@acme/private-tool"]
path = "tools/private-tool"
type = "npm"
publish = { enabled = false }

[package."@acme/custom-registry-package"]
path = "packages/custom"
type = "npm"
publish = { enabled = true, mode = "external" }
```

Built-in publishing is for canonical public registries. Use `mode = "external"` for private registries or custom release jobs.

Use `publish = { enabled = false }` for packages that should be versioned but never published. Use external mode when monochange should still plan versions and release records but another CI job owns registry credentials, custom rate limits, private feeds, or manual approval gates.

## Changelog configuration

```toml
[defaults.changelog]
path = "{{ path }}/CHANGELOG.md"
format = "keep_a_changelog"
initial_header = """
# Changelog

All notable changes to this project will be documented in this file.
"""

[changelog.types]
feat = { bump = "minor", section = "feat", description = "New user-facing functionality" }
fix = { bump = "patch", section = "fix", description = "Bug fixes" }
docs = { bump = "none", section = "docs", description = "Documentation only" }

[changelog.sections]
feat = { heading = "Added", priority = 20 }
fix = { heading = "Fixed", priority = 30 }
docs = { heading = "Documentation", priority = 40 }
```

## Custom CLI workflows

`[cli.<name>]` creates `mc <name>` in that repository. These workflows are the maintainable place to compose built-in steps with local shell commands, input defaults, dry-run behavior, and CI-specific guards.

Name workflow commands after user intent (`change`, `release`, `publish-check`) rather than implementation detail. Keep destructive workflows explicit, and expose safe dry-run or JSON-producing workflows for agents and automation.

```toml
[cli.change]
help_text = "Create a changeset"
inputs = [
	{ name = "package", type = "string_list", required = true },
	{ name = "bump", type = "choice", choices = ["none", "patch", "minor", "major"], default = "patch" },
	{ name = "reason", type = "string", required = true },
	{ name = "type", type = "string" },
	{ name = "caused_by", type = "string_list" },
]
steps = [
	{ name = "create change file", type = "CreateChangeFile", inputs = ["interactive", "package", "bump", "version", "type", "caused_by", "reason", "details", "output"] },
]

[cli.release]
help_text = "Prepare versioned package files"
inputs = [
	{ name = "format", type = "choice", choices = ["text", "markdown", "json"], default = "markdown" },
]
steps = [
	{ name = "plan release", type = "PrepareRelease", inputs = ["format"] },
	{ name = "refresh lockfiles", type = "Command", command = "pnpm install --lockfile-only" },
]
```

Validate custom workflows with `mc step:validate` and inspect them with `mc help`.
