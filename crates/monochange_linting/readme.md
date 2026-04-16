# monochange_linting

Authoring helpers and macros for monochange lint suites.

This crate keeps lint declaration boilerplate small so ecosystem crates can focus on rule logic instead of repeatedly spelling out metadata constructors.

## Guidance

- Use `declare_lint_rule!` for straightforward rules whose custom behavior mostly lives in `run(...)`.
- The Cargo suite uses the macro for real rules, so the helper now reflects actual ecosystem code instead of scaffolding-only examples.
- If a rule eventually needs extra construction state or a custom constructor, an explicit `struct` plus `LintRule::new(...)` is still fine.
