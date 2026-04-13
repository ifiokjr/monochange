# Lockfile Automation

monochange can automatically refresh ecosystem lockfiles after version updates, ensuring your dependencies stay synchronized.

## Overview

When versions change during a release, dependent packages may need updated lockfiles. monochange can run lockfile refresh commands automatically as part of the release process.

## Configuration

Lockfile commands are configured per ecosystem in `[ecosystems.<name>]`:

```toml
[ecosystems.cargo]
lockfile_commands = ["cargo update --workspace"]

[ecosystems.npm]
lockfile_commands = ["npm install"]

[ecosystems.pnpm]
lockfile_commands = ["pnpm install"]

[ecosystems.bun]
lockfile_commands = ["bun install"]
```

## Default Commands

If not specified, monochange infers sensible defaults:

| Ecosystem    | Default Lockfile Command                 |
| ------------ | ---------------------------------------- |
| Cargo        | `cargo update --workspace`               |
| npm          | `npm install`                            |
| pnpm         | `pnpm install`                           |
| Bun          | `bun install`                            |
| Deno         | (none — Deno uses lockfiles differently) |
| Dart/Flutter | (none — uses pubspec.lock automatically) |

## When Commands Run

Lockfile commands execute:

1. **After version updates** — When `PrepareRelease` updates versions in manifests
2. **Before changelog rendering** — So lockfile changes are included in the release
3. **Per ecosystem** — Only for ecosystems that have changes

## Command Execution

Commands run with:

- **Working directory**: Package root directory
- **Environment**: Same as monochange process
- **Output**: Captured and shown in progress
- **Failure**: Fails the release step

## Custom Commands

You can define multiple commands or custom logic:

```toml
[ecosystems.cargo]
lockfile_commands = [
	"cargo update --workspace",
	"cargo update -p some-crate",
]
```

Or use a script:

```toml
[ecosystems.cargo]
lockfile_commands = ["./scripts/update-cargo-locks.sh"]
```

## Disabling Lockfile Updates

To disable automatic lockfile updates for an ecosystem:

```toml
[ecosystems.cargo]
lockfile_commands = []
```

## Conditional Execution

Commands only run when:

- The ecosystem has packages with version changes
- The package has a lockfile (e.g., `Cargo.lock`, `package-lock.json`)
- Lockfile commands are configured (or have defaults)

## Integration with Release Flow

Lockfile updates happen automatically during `PrepareRelease`:

```
1. Read changesets
2. Compute release plan
3. Update versions in manifests
4. ↓ Run lockfile commands ← You are here
5. Render changelogs
6. Update versioned files
7. Write release manifest
```

## Best Practices

### Cargo Workspaces

For Cargo workspaces, prefer `--workspace` to update all workspace members:

```toml
[ecosystems.cargo]
lockfile_commands = ["cargo update --workspace"]
```

### npm/pnpm/Bun

These ecosystems update lockfiles automatically on install:

```toml
[ecosystems.npm]
lockfile_commands = ["npm install --package-lock-only"]
```

Use `--package-lock-only` to avoid side effects.

### Monorepo Considerations

In monorepos with multiple ecosystems:

```toml
[ecosystems.cargo]
roots = ["crates"]
lockfile_commands = ["cargo update --workspace"]

[ecosystems.npm]
roots = ["packages"]
lockfile_commands = ["npm install"]
```

Each ecosystem's commands run in their respective roots.

## Troubleshooting

### Lockfile not updating

1. Check that `lockfile_commands` is configured or has defaults
2. Verify the package has a lockfile in the expected location
3. Run with `--log-level debug` to see command execution

### Command fails

1. Test the command manually in the package directory
2. Check that required tools are in PATH
3. Ensure the command doesn't require interactive input

### Unwanted lockfile changes

1. Use `--package-lock-only` for npm/pnpm to avoid installs
2. Use `--workspace` for Cargo to limit scope
3. Set `lockfile_commands = []` to disable

## Example: Complete Configuration

```toml
[ecosystems.cargo]
enabled = true
roots = ["."]
lockfile_commands = ["cargo update --workspace"]

[ecosystems.npm]
enabled = true
roots = ["packages"]
lockfile_commands = ["npm install --package-lock-only"]

[ecosystems.deno]
enabled = true
roots = ["deno-packages"]
# Deno doesn't use lockfile_commands — it manages its own lockfile
```
