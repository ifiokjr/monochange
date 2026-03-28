# Discovery

`monochange` discovers packages from native manifests and workspace definitions.

Supported sources in this milestone:

- Cargo workspaces and standalone crates
- npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages
- Deno workspaces and standalone `deno.json` / `deno.jsonc` packages
- Dart and Flutter workspaces plus standalone `pubspec.yaml` packages

Run discovery:

```bash
mc workspace discover --root . --format json
```

Key behaviors:

- workspace globs are expanded by each ecosystem adapter
- dependency names are normalized into one graph
- version-group assignments are attached after discovery
- unmatched group members and version mismatches produce warnings
