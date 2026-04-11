---
"monochange": patch
"monochange_cargo": patch
"monochange_core": patch
---

Speed up `mc release` by keeping the default release path fully in-process.

Before:

- `mc release` inferred external lockfile commands such as `cargo generate-lockfile` or `npm install --package-lock-only`
- release diff previews were prepared even when you did not ask for `--diff`

After:

- `mc release` rewrites supported lockfiles directly from the release plan (`Cargo.lock`, `package-lock.json`, `pnpm-lock.yaml`, `bun.lock`, `bun.lockb`, `deno.lock`, and `pubspec.lock`)
- external `lockfile_commands` only run when you configure them explicitly, and Cargo falls back to `cargo generate-lockfile` only when the existing lock is too incomplete for a safe in-place rewrite
- diff previews are generated only for `--diff` or when later command steps reference `release.file_diffs`

Example:

```toml
[ecosystems.cargo]
# no lockfile_commands configured
```

Before:

```sh
mc release
# shells out to cargo generate-lockfile
```

After:

```sh
mc release
# stays on the fast in-process path unless you opt into lockfile_commands
```
