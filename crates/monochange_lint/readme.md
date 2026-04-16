# monochange_lint

Ecosystem-agnostic manifest lint engine for monochange.

## Purpose

This crate runs lint suites contributed by ecosystem crates. It owns:

- lint suite registration
- preset and rule discovery
- workspace-wide lint execution
- scoped `[lints]` configuration merging
- autofix application

It deliberately does **not** know how to parse Cargo, npm, Deno, or Dart manifests. That behavior now lives with the ecosystem crates themselves.

## Related crates

- `monochange_core::lint` — shared lint contracts and config types
- `monochange_linting` — authoring macros and helpers
- `monochange_lint_testing` — snapshot-friendly testing helpers
- `monochange_cargo::lints` — Cargo manifest lint suite
- `monochange_npm::lints` — npm-family manifest lint suite
