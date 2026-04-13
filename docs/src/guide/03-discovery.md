# Discovery

`monochange` discovers packages from native manifests and workspace definitions.

Supported sources today:

<!-- {=discoverySupportedSources} -->

- Cargo workspaces and standalone crates
- npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages
- Deno workspaces and standalone `deno.json` / `deno.jsonc` packages
- Dart and Flutter workspaces plus standalone `pubspec.yaml` packages

<!-- {/discoverySupportedSources} -->

Run discovery:

<!-- {=projectDiscoverCommand} -->

```bash
mc validate
mc discover --format json
```

<!-- {/projectDiscoverCommand} -->

Key behaviors:

<!-- {=discoveryKeyBehaviors} -->

- native workspace globs are expanded by each ecosystem adapter
- dependency names are normalized into one graph
- package ids and manifest paths in CLI output are rendered relative to the repository root for deterministic automation
- gitignored paths and nested git worktrees are skipped during discovery
- version-group assignments are attached after discovery
- unmatched group members (declared in config but not found during discovery) produce warnings
- unresolvable group members (invalid package IDs in `group.packages`) produce errors during configuration loading
- discovery currently scans all supported ecosystems regardless of `[ecosystems.*]` toggles in `monochange.toml`

<!-- {/discoveryKeyBehaviors} -->
