---
"monochange_core": minor
---

#### add core linting types

Add `monochange_core::lint` module with the foundational types for the linting system:

- `LintSeverity` (Off, Warning, Error) — rule severity levels
- `LintCategory` (Style, Correctness, Performance, Suspicious, BestPractice) — rule classification
- `LintRule` — rule definition with id, name, description, and autofixable flag
- `LintResult`, `LintLocation` — individual findings with file location and byte spans
- `LintFix`, `LintEdit` — autofix suggestions with span-based replacements
- `LintRuleConfig` — flexible configuration supporting simple severity or detailed options
- `LintReport` — aggregated results with error/warning counts
- `LintContext` — rule input with workspace root, manifest path, and file contents
- `LintRuleRunner` trait — executable rule interface with `rule()`, `applies_to()`, and `run()`
- `LintRuleRegistry` — rule registration and discovery

Also adds `lints` field to `EcosystemSettings` for per-ecosystem lint configuration and `Lint` variant to `CliStepDefinition` with `format`, `fix`, and `ecosystem` inputs.
