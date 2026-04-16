# Linting architecture roadmap

## Goals

- keep `monochange_lint` ecosystem-agnostic
- move ecosystem-specific lint logic into ecosystem crates
- centralize lint configuration in top-level `[lints]`
- support presets, scoped overrides, and rule discovery
- make new lint authoring and testing cheaper

## Implemented architecture

### Core contracts

Shared lint contracts live in `monochange_core::lint`:

- rule metadata and results
- target metadata
- top-level `[lints]` configuration types
- `LintRuleRunner`
- `LintSuite`

### Generic engine

`crates/monochange_lint` now owns:

- suite registration
- preset registration
- scoped config merging
- rule filtering
- autofix application

It does not parse Cargo or npm manifests itself.

### Ecosystem suites

Ecosystem crates now own their own lint suites:

- `crates/monochange_cargo/src/lints/`
- `crates/monochange_npm/src/lints/`

Each suite:

- discovers its own targets
- parses manifests once per file
- exposes presets
- provides rule implementations

### Authoring and testing helpers

New helper crates:

- `crates/monochange_linting` — rule declaration macro helpers
- `crates/monochange_lint_testing` — snapshot-friendly report and fix formatting

## Configuration shape

```toml
[lints]
use = ["cargo/recommended", "npm/recommended"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

## CLI surface

New lint-focused commands:

- `mc lint list`
- `mc lint explain <id>`
- `mc lint new <ecosystem>/<rule-name>`
- `mc check --only <rule-id>`

## Follow-up work

The refactor establishes the new architecture. Natural next steps are:

1. split suite modules into one file per rule
2. add Deno and Dart lint suites when rules exist
3. expand `mc lint new` so it also wires module registration automatically
4. migrate more rule tests to `monochange_lint_testing`
5. add richer rule documentation and examples to `mc lint explain`
