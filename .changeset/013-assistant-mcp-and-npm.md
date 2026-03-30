---
main: minor
---

#### add assistant setup, MCP server, and npm distribution

Introduce built-in `assist` and `mcp` commands so assistants can install MonoChange, add it as an MCP server, and follow a consistent repo-local workflow for validation, discovery, changesets, and dry-run release planning.

Add an installable `@monochange/skill` package with bundled guidance plus a `monochange-skill` helper, and add release automation for publishing the CLI through `@monochange/cli` using GitHub release assets and npm platform packages.
