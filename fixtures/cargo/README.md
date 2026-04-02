Cargo fixtures for workspace discovery, standalone discovery, release planning, and workspace-version validation.

## Fixture index

- `standalone/` — single Cargo package discovery
- `workspace/` — baseline Cargo workspace discovery and release-planning behavior
- `workspace-versioned/` — cargo adapter fixture where one package uses `version = { workspace = true }` and another uses an explicit version
- `workspace-versioned-same-group/` — validation passes when all configured workspace-versioned packages share one version group
- `workspace-versioned-different-groups/` — validation fails when configured workspace-versioned packages are split across groups
- `workspace-versioned-ungrouped/` — validation fails when multiple configured workspace-versioned packages are left ungrouped
- `workspace-versioned-single/` — validation passes when only one configured workspace-versioned package exists without a group
- `workspace-versioned-ignores-unconfigured/` — validation ignores unconfigured/private workspace-versioned crates that are not declared in `monochange.toml`
- `workspace-versioned-multi-workspace/` — validation scopes checks per Cargo workspace root and allows multiple independent workspaces in one repository
- `workspace-versioned-grouped-release/` — grouped release fixture used by CLI validate snapshot coverage
