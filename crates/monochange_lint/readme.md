# monochange_lint

Ecosystem-specific lint rules for monochange.

## Purpose

This crate provides linting capabilities for monorepo package manifests across multiple ecosystems (Cargo, npm, Deno, Dart). It enforces best practices, consistency, and correctness through configurable rules.

## Features

- **Cross-ecosystem linting**: Unified rule framework for Cargo.toml, package.json, deno.json, and pubspec.yaml
- **Configurable severity**: Each rule can be set to error, warn, or off
- **Autofix support**: Many rules can automatically fix issues while preserving formatting
- **Extensible architecture**: Easy to add new rules for any ecosystem

## Cargo Rules

| Rule ID                         | Description                                           | Autofixable |
| ------------------------------- | ----------------------------------------------------- | ----------- |
| `dependency-field-order`        | Enforces consistent ordering of dependency fields     | Yes         |
| `internal-dependency-workspace` | Requires `workspace = true` for internal dependencies | Yes         |
| `required-package-fields`       | Enforces required `[package]` fields                  | Partial     |
| `sorted-dependencies`           | Requires alphabetically sorted dependency tables      | Yes         |

## NPM Rules

| Rule ID                   | Description                                      | Autofixable |
| ------------------------- | ------------------------------------------------ | ----------- |
| `workspace-protocol`      | Requires `workspace:` protocol for internal deps | Yes         |
| `sorted-dependencies`     | Requires alphabetically sorted dependencies      | Yes         |
| `required-package-fields` | Enforces required fields in package.json         | Partial     |

## Usage

```rust
use monochange_lint::Linter;
use monochange_core::lint::LintReport;

let linter = Linter::default();
let report: LintReport = linter.lint_workspace("/path/to/repo")?;

if report.has_errors() {
    for result in report.results {
        println!("{}", result);
    }
}
```
