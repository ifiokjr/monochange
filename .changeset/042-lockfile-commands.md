---
monochange: patch
monochange_core: patch
monochange_config: patch
monochange_cargo: patch
monochange_npm: patch
monochange_dart: patch
monochange_deno: patch
---

Move release-time lockfile refresh to command-driven execution.

Before:

```toml
[package.app]
path = "packages/app"
versioned_files = ["package-lock.json"]
```

After:

```toml
[ecosystems.npm]
lockfile_commands = [
	{ command = "npm install --package-lock-only", cwd = "packages/app" },
]
```

MonoChange now infers default lockfile commands for Cargo, npm-family, and Dart/Flutter workspaces when packages in that ecosystem are released, and stops inferring defaults when explicit `lockfile_commands` are configured for that ecosystem.

`versioned_files` also now support plain-text regex replacements without an explicit ecosystem `type`, as long as the regex includes a named `version` capture group.

#### Regex versioned files

Regex entries let you version-stamp any plain-text file — README badges, download links, install scripts — without needing an ecosystem-specific parser. The regex must contain a named `version` capture group; MonoChange replaces the captured substring with the new version while preserving the surrounding text.

```toml
[package.core]
path = "crates/core"
versioned_files = [
	# update a download link in the README
	{ path = "README.md", regex = 'https://example\.com/download/v(?<version>\d+\.\d+\.\d+)\.tgz' },
	# update a version badge
	{ path = "README.md", regex = 'img\.shields\.io/badge/version-(?<version>\d+\.\d+\.\d+)-blue' },
]

[group.sdk]
packages = ["core", "cli"]
versioned_files = [
	# update the install script across all packages (glob pattern)
	{ path = "**/install.sh", regex = 'SDK_VERSION="(?<version>\d+\.\d+\.\d+)"' },
]

[ecosystems.cargo]
versioned_files = [
	# update a workspace-wide version constant
	{ path = "crates/constants/src/lib.rs", regex = 'pub const VERSION: &str = "(?<version>\d+\.\d+\.\d+)"' },
]
```

Key rules:

- `regex` entries cannot set `type`, `prefix`, `fields`, or `name` — they operate on raw text
- the regex must include a `(?<version>...)` named capture group
- the `path` field supports glob patterns (e.g. `**/README.md`)
- regex entries work on packages, groups, and ecosystem-level `versioned_files`
