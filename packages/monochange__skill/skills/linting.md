# Linting

`mc check` validates monochange configuration, changesets, and package manifests using configured lint rules.

Use `mc validate` when you only need to know whether monochange can load the workspace and changesets. Use `mc check` when package metadata consistency matters: publishability fields, workspace dependency protocols, duplicate changesets, package ownership, and other ecosystem-specific policy.

## Commands

```bash
mc check
mc check --fix
mc lint list
mc lint explain <rule-or-preset-id>
```

MCP equivalents:

- `monochange_lint_catalog` — list rules and presets for an agent UI or planning step.
- `monochange_lint_explain` — explain why a rule exists, which manifests it applies to, and what remediation usually looks like.

## Configuration

```toml
[lints]
use = ["cargo/recommended", "npm/recommended"]
exclude = ["examples/**", "fixtures/**"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"
"changesets/duplicate" = "error"

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

Rules accept either a simple severity (`"error"`, `"warning"`, `"off"`) or a table with `level` and rule-specific options.

Use presets for the baseline policy and then layer explicit rules or scopes for exceptions. Scopes are useful when published packages need stricter metadata than fixtures, examples, private tools, or generated manifests.

Run `mc check` before release previews and before merging configuration changes.
