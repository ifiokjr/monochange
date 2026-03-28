# monochange_npm

npm-family ecosystem support for `monochange`.

## Public entry points

- `discover_npm_packages(root)` discovers npm, pnpm, and Bun workspaces plus standalone packages
- `NpmAdapter` exposes the shared adapter interface

## Scope

- `package.json` workspaces
- `pnpm-workspace.yaml`
- Bun lockfile detection
- normalized dependency extraction
