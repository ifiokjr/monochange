---
"monochange": minor
"monochange_config": minor
"monochange_core": minor
"monochange_lint": minor
---

#### add comprehensive linting system for monorepo package manifests

This PR introduces a complete linting framework for monochange that enables ecosystem-specific lint rules for monorepo package manifests. The system supports Cargo.toml, package.json, and provides a foundation for Deno and Dart ecosystems.

**Key Features:**

- **Core Types** (`monochange_core::lint`): `LintSeverity`, `LintCategory`, `LintRule`, `LintResult`, `LintFix`, `LintRuleConfig`, `LintReport`, `LintContext`, `LintRuleRunner` trait, and `LintRuleRegistry`

- **Cargo Lint Rules** (5 rules):
  - `dependency-field-order`: Enforces field ordering in dependencies
  - `internal-dependency-workspace`: Requires `workspace = true` for internal deps
  - `required-package-fields`: Enforces required `[package]` fields
  - `sorted-dependencies`: Requires sorted dependency tables
  - `unlisted-package-private`: Packages not in monochange.toml must be private

- **NPM Lint Rules** (6 rules):
  - `workspace-protocol`: Requires `workspace:` for internal deps
  - `sorted-dependencies`: Requires sorted deps
  - `required-package-fields`: Enforces required fields
  - `root-no-prod-deps`: Root should only have devDependencies
  - `no-duplicate-dependencies`: No deps in multiple sections
  - `unlisted-package-private`: Packages not in monochange.toml must be private

- **Configuration**: Configure per ecosystem in `monochange.toml`:
  ```toml
  [ecosystem.cargo.lints]
  dependency-field-order = "error"
  internal-dependency-workspace = { level = "error", fix = true }
  ```

- **CLI**: New `mc lint` command with `--fix`, `--ecosystem`, `--format` flags

- **Lint Step**: Available in CLI workflow definitions

**The `unlisted-package-private` rule** ensures packages not defined in monochange.toml are marked as private to prevent accidental publishing. All rules support autofix where possible while preserving file formatting.
