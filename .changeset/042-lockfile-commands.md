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

`versioned_files` also now support plain-text regex replacements without an explicit ecosystem `type`, as long as the regex includes a named `version` capture.
