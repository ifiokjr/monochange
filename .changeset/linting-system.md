# feat: Add comprehensive linting system for monochange

## Summary

This PR introduces a complete linting framework for monochange that enables ecosystem-specific lint rules for monorepo package manifests. The system supports Cargo.toml, package.json, and provides a foundation for Deno and Dart ecosystems.

## Motivation

Monorepos often accumulate inconsistencies across package manifests over time. Common issues include:
- Dependencies using inconsistent versioning schemes
- Missing required package metadata
- Internal dependencies not using workspace references
- Dependencies listed multiple times across sections
- Packages not intended for publication lacking `private` markers

A configurable linting system helps maintain consistency and prevents accidental publishing of internal packages.

## Architecture Overview

### Core Types (`monochange_core/src/lint.rs`)

The linting system is built around several key types:

- **`LintSeverity`**: `Off`, `Warning`, `Error` - Controls whether rules run and their impact
- **`LintCategory`**: `Style`, `Correctness`, `Performance`, `Suspicious`, `BestPractice` - Classification for documentation
- **`LintRule`**: Rule definition with id, name, description, autofixable flag
- **`LintResult`**: Individual findings with location, message, severity, and optional fix
- **`LintFix`** / **`LintEdit`**: Structured autofix suggestions with byte spans and replacements
- **`LintRuleConfig`**: Flexible configuration supporting simple severity or detailed options
- **`LintReport`**: Aggregated results with error/warning counts and fixable items
- **`LintContext`**: Rule input containing workspace root, manifest path, and file contents
- **`LintRuleRunner`**: Trait for executable rules with `rule()`, `applies_to()`, and `run()` methods
- **`LintRuleRegistry`**: Registry pattern for managing and discovering rules

### Configuration Schema

Users configure lint rules per ecosystem in `monochange.toml`:

```toml
[ecosystem.cargo.lints]
dependency-field-order = "error"
internal-dependency-workspace = { level = "error", fix = true }
required-package-fields = { level = "warn", fields = ["description", "license", "repository"] }
sorted-dependencies = "warn"
unlisted-package-private = "error"

[ecosystem.npm.lints]
workspace-protocol = "error"
sorted-dependencies = "warn"
required-package-fields = { level = "warn", fields = ["description", "repository", "license"] }
root-no-prod-deps = "error"
no-duplicate-dependencies = "error"
unlisted-package-private = "error"
```

Configuration supports:
- **Simple severity**: `rule = "error"` or `rule = "warn"` or `rule = "off"`
- **Detailed config**: `{ level = "error", ...rule_specific_options }`

### Lint Rules Reference

#### Cargo Rules (5 rules)

| Rule ID | Description | Autofixable | Category |
|---------|-------------|-------------|----------|
| `cargo/dependency-field-order` | Enforces field ordering (workspace/version → default-features → features → others) | Yes | Style |
| `cargo/internal-dependency-workspace` | Requires `workspace = true` for internal dependencies | Yes | Correctness |
| `cargo/required-package-fields` | Enforces required `[package]` fields | Partial | Correctness |
| `cargo/sorted-dependencies` | Requires alphabetically sorted dependency tables | Yes | Style |
| `cargo/unlisted-package-private` | Packages not in monochange.toml must be private | Yes | Correctness |

#### NPM Rules (6 rules)

| Rule ID | Description | Autofixable | Category |
|---------|-------------|-------------|----------|
| `npm/workspace-protocol` | Requires `workspace:` for internal dependencies | Yes | Correctness |
| `npm/sorted-dependencies` | Requires alphabetically sorted dependencies | Yes | Style |
| `npm/required-package-fields` | Enforces required fields | Partial | Correctness |
| `npm/root-no-prod-deps` | Root should only have devDependencies | Yes | BestPractice |
| `npm/no-duplicate-dependencies` | Same dep shouldn't appear in multiple sections | Yes | Correctness |
| `npm/unlisted-package-private` | Packages not in monochange.toml must be private | Yes | Correctness |

### New `mc lint` Command

```bash
# Run all lint rules
mc lint

# Auto-fix issues where possible
mc lint --fix

# Limit to specific ecosystems
mc lint --ecosystem cargo,npm

# Output as JSON for CI/integration
mc lint --format json
```

### Lint CLI Step

The `Lint` step is now available in CLI configuration:

```toml
[[cli.check.steps]]
type = "Lint"
inputs = { format = "text", fix = false }
```

Supported inputs:
- `format`: "text" or "json" (default: text)
- `fix`: boolean to auto-fix (default: false)
- `ecosystem`: comma-separated list of ecosystems to lint

### Autofix Architecture

Fixes are designed to preserve file formatting as much as possible:

1. **TOML files**: Uses `toml_edit` for format-preserving edits that maintain:
   - Comments
   - Whitespace and indentation
   - Key ordering (outside of sorted rules)
   - Inline vs table formatting

2. **JSON files**: Uses custom span-based replacements to:
   - Preserve key ordering
   - Minimize diff footprint
   - Avoid full-file reformatting

3. **Fix application**: 
   - Fixes are collected and sorted by span (largest offsets first)
   - Applied in a single pass to avoid invalidating spans
   - Only fixes for enabled rules are applied

### Implementation Highlights

#### Rule Registration Pattern

Rules are registered in `Linter::new()`:

```rust
let mut registry = LintRuleRegistry::new();
for rule in CargoLintRules::default_rules() {
    registry.register(rule);
}
for rule in NpmLintRules::default_rules() {
    registry.register(rule);
}
```

Rules implement `LintRuleRunner`:

```rust
pub trait LintRuleRunner: Send + Sync {
    fn rule(&self) -> &LintRule;
    fn applies_to(&self, path: &Path) -> bool;
    fn run(&self, ctx: &LintContext<'_>, config: &LintRuleConfig) -> Vec<LintResult>;
}
```

#### Workspace Integration

The `unlisted-package-private` rule exemplifies cross-crate integration:

1. Reads `monochange.toml` to find configured packages
2. For each manifest not in config, checks `private`/`publish` state
3. Reports error if package is implicitly public
4. Provides autofix to add appropriate privacy marker

This prevents accidental publishing of packages discovered but not explicitly managed.

## Files Changed

### Core Types
- `crates/monochange_core/src/lib.rs`: Added `Lint` variant to `CliStepDefinition`, added `lints` field to `EcosystemSettings`
- `crates/monochange_core/src/lint.rs`: **NEW** - Core linting types and traits

### Configuration
- `crates/monochange_config/src/lib.rs`: Added `lints` field to `RawEcosystemSettings`, updated parsing

### Lint Rules Implementation
- `crates/monochange_lint/Cargo.toml`: **NEW** - Crate configuration
- `crates/monochange_lint/readme.md`: **NEW** - Documentation
- `crates/monochange_lint/src/lib.rs`: **NEW** - Main `Linter` struct and orchestration
- `crates/monochange_lint/src/cargo.rs`: **NEW** - Cargo-specific rules (5 rules)
- `crates/monochange_lint/src/npm.rs`: **NEW** - NPM-specific rules (6 rules)

### CLI Integration
- `crates/monochange/src/cli.rs`: Added `build_lint_subcommand()` with `--fix`, `--ecosystem`, `--format` flags
- `crates/monochange/src/lib.rs`: Added `lint` module, match arm for `"lint"` command
- `crates/monochange/src/lint.rs`: **NEW** - `run_lint_command()` implementation

### Workspace
- `Cargo.toml`: Added `monochange_lint` to workspace dependencies

## Design Decisions

### Why Trait Objects for Rules?

`LintRuleRunner` uses trait objects (`Box<dyn LintRuleRunner>`) because:
- Rules are heterogeneous types with different internal state
- Registry needs to store mixed rule types
- Enables dynamic dispatch for rule execution
- Allows third-party rules to be registered in future

### Why Per-Ecosystem Configuration?

Lint rules are configured per ecosystem because:
- Rules are inherently ecosystem-specific (Cargo rules don't apply to NPM)
- Keeps configuration localized and understandable
- Allows different severity levels per ecosystem
- Mirrors how other ecosystem settings work (versioned_files, lockfile_commands)

### Why Span-Based Fixes?

Fixes use byte spans rather than AST manipulation because:
- Preserves user formatting (comments, whitespace)
- Minimizes diff noise in version control
- Aligns with `toml_edit` philosophy for TOML files
- Allows precise, surgical edits

### Why `unlisted-package-private` Rule?

This rule was specifically requested and addresses a real need:
- In large monorepos, packages are often added without updating monochange.toml
- These packages may get published accidentally if not marked private
- Rule provides clear remediation path (add to config OR mark private)
- Autofix makes remediation easy

## Testing

The implementation includes:

- Unit tests in `lint.rs` for core types (`LintReport`, `LintRuleConfig`, etc.)
- Unit tests in `cargo.rs` and `npm.rs` for rule applicability
- Integration tests in `lint.rs` for fix application

## Future Work

Potential extensions (not in this PR):

- **Deno rules**: `import-version-required`, `prefer-import-map`
- **Dart rules**: `version-constraint-required`, `pubspec-valid`
- **External version consistency**: Ensure external deps use same version across packages
- **Peer/dev dependency sync**: NPM peerDependencies must be in devDependencies
- **Rule documentation**: Auto-generated rule documentation from source
- **Custom rules**: User-defined rules via WASM or scripting

## Breaking Changes

None. This is a purely additive feature:
- New optional `[ecosystem.*.lints]` configuration sections
- New optional `mc lint` command
- New optional `Lint` CLI step

## Checklist

- [x] Core linting types defined
- [x] Configuration parsing implemented
- [x] Cargo rules implemented (5 rules)
- [x] NPM rules implemented (6 rules)
- [x] Autofix framework implemented
- [x] `mc lint` command added
- [x] Lint CLI step added
- [x] Tests included
- [x] Documentation added
- [x] No breaking changes
- [x] Follows project naming conventions (`monochange` lowercase)

## Related

- Design document: `docs/design/linting-system.md`
- Inspired by: [Manypkg](https://github.com/Thinkmill/manypkg) for NPM ecosystem checks
