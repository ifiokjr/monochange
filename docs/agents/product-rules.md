# Product and architecture rules

- Keep `crates/monochange` as the CLI package.
- Keep `crates/monochange_core` focused on shared domain types.
- Put adapter-specific manifest behavior in ecosystem crates.
- Preserve fixture-first validation for discovery and planning behavior.
- Prefer configured package ids and group ids over raw manifest paths in changesets and docs.
