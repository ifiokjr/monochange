---
"monochange": minor
"monochange_lint": minor
---

#### add mc check command and lint rule integration

Add `mc check` CLI command that combines workspace validation with lint rule enforcement. Supports `--fix` for autofix, `--ecosystem` for filtering, and `--format` for output selection. The Validate step also runs lint rules and accepts a `fix` input. Includes the full `monochange_lint` crate with 5 Cargo and 6 NPM lint rules.