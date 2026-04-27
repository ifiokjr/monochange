# Configuration skill

Use this guide when the task is to create, review, or extend `monochange.toml`.

## First choice: generate, then edit

Start with generated config whenever possible:

```bash
mc init
```

`mc init` seeds editable `[cli.*]` workflow commands. If you already have `monochange.toml`, edit existing workflow tables directly or add new `[cli.<name>]` tables; use immutable `mc step:*` commands when you need direct built-in step execution without a wrapper.

Validate after every meaningful change:

```bash
mc validate
```

## Minimum viable config

A small repo can start with defaults plus one package:

```toml
[defaults]
package_type = "cargo"

[package.monochange]
path = "crates/monochange"
```

Use this when the repo is simple and one ecosystem dominates.

## Add changelog defaults early

A useful next step is defining how changelogs should render:

```toml
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/changelog.md"
format = "keep_a_changelog"
```

Why:

- packages inherit the same changelog shape by default
- you avoid repeating the same table everywhere
- you can still override per package or group later

## Declare packages explicitly

monochange works best when every managed package has a stable id.

```toml
[package.monochange_core]
path = "crates/monochange_core"

[package.monochange]
path = "crates/monochange"
versioned_files = ["Cargo.toml"]
```

Prefer ids you want to see in changesets and release output.

## Use groups for shared release identity

When several packages version together, create a group:

```toml
[group.sdk]
packages = ["monochange_core", "monochange"]
tag = true
release = true
version_format = "primary"
```

Use a group when the outward release boundary is shared.

Do not use a group just because packages depend on each other. Simple dependency propagation often does the right thing without grouping.

## Add extra changelog sections intentionally

Use `extra_changelog_sections` when one change type deserves its own heading:

```toml
[package.web-app]
path = "apps/web"
type = "cargo"
extra_changelog_sections = [
	{ name = "User Experience", types = ["ux"], default_bump = "minor" },
]
```

Then create a changeset like:

```bash
mc change --package web-app --bump minor --type ux --reason "redesign settings navigation"
```

## Version more than manifests

Use `versioned_files` when a release also needs to update README badges, install scripts, or extra manifests.

### Ecosystem-aware entries

```toml
[package.monochange]
path = "crates/monochange"
versioned_files = [
	"Cargo.toml",
	{ path = "crates/monochange/extra.toml", type = "cargo" },
]
```

### Regex entries for plain text

```toml
[package.monochange]
path = "crates/monochange"
versioned_files = [
	{ path = "README.md", regex = 'v(?<version>\d+\.\d+\.\d+)' },
]
```

Use regex entries when no ecosystem parser applies.

## Lockfile refresh strategy

If the built-in lockfile rewriting is enough, keep config minimal.

Add `lockfile_commands` only when the package manager must do the refresh:

```toml
[ecosystems.npm]
lockfile_commands = [
	{ command = "pnpm install --lockfile-only", cwd = "packages/web" },
]
```

Use this when workspace-specific package-manager behavior matters more than raw speed.

## Package publishing and trust

Publishing is separate from release planning.

```toml
[ecosystems.npm.publish]
mode = "builtin"
trusted_publishing = true

[package.web.publish.placeholder]
readme_file = "docs/web-placeholder.md"
```

Use:

- `mc placeholder-publish` to bootstrap missing public packages
- `mc publish` for package-registry publishing
- `mc publish-release` for hosted/provider releases

Preference rules for trusted publishing:

- for npm on GitHub, `mode = "builtin"` is the preferred path because monochange can verify and configure trust itself
- for `crates.io`, prefer `rust-lang/crates-io-auth-action@v1` when you want a registry-native GitHub Actions publish workflow
- for `pub.dev`, prefer `dart-lang/setup-dart/.github/workflows/publish.yml@v1` when you want the workflow shape recommended by the Dart team
- for `crates.io` and `pub.dev`, `mode = "external"` is often the clearest fit when the registry-maintained workflow should own the publish command directly
- if one repository publishes multiple public packages, use [multi-package-publishing.md](./multi-package-publishing.md) to decide between one shared `mc publish` job, package-specific jobs, or fully external workflows

## Release titles and changelog headings

Customize outward release text with these fields:

```toml
[defaults]
release_title = "{{ version }} ({{ date }})"
changelog_version_title = "[{{ version }}]({{ tag_url }}) ({{ date }})"
```

Use them when:

- provider release titles need a consistent format
- changelog headings should include links or dates
- group releases should read differently from package releases

## Add or override top-level commands

`monochange.toml` can define or override `[cli.<command>]` entries.

```toml
[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["markdown", "text", "json"]
default = "markdown"

[[cli.release.steps]]
type = "PrepareRelease"
```

Use the `[cli.*]` workflow tables generated by `mc init` as the editable starting point, or add a new `[cli.<name>]` table yourself.

## A safe config-edit workflow

```bash
mc init
mc validate
mc discover --format json
mc validate
mc release --dry-run --format json
```

If you touched grouped packages, changelog settings, or versioned files, add `--diff` to inspect the planned file changes:

```bash
mc release --dry-run --diff
```

## Common mistakes

### Hand-writing ids before discovery

**Avoid:** inventing ids from directory names.

**Prefer:**

```bash
mc discover --format json
```

### Over-grouping packages

**Avoid:** putting every related package into one group.

**Prefer:** only group packages that should share one outward version and release identity.

### Forgetting validation after config edits

**Avoid:** editing `monochange.toml` and going straight to publishing.

**Prefer:**

```bash
mc validate
mc release --dry-run --format json
```

## Related references

- [reference.md](./reference.md)
- [commands.md](./commands.md)
- [linting.md](./linting.md)
