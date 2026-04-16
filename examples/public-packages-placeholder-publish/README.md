# Public packages with placeholder publishing

This is a repo-shaped example for reserving public package names before the real publish flow is ready.

## What this example includes

- `monochange.toml` with npm and Cargo publishing defaults plus placeholder README overrides
- a small mixed workspace with one Cargo crate and one npm package
- placeholder README files under `docs/`
- a sample `.changeset/*.md` file for release planning

## Recommended validation flow

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Recommended publish preview

When you want to inspect the placeholder plan without publishing anything:

```bash
mc placeholder-publish --dry-run --format json
```

## Why this example is opinionated

- it models the bootstrap case where names matter before the first real release
- it keeps placeholder copy explicit so the public registry entry is readable during the reservation phase
- it shows one shared repository can prepare placeholder strategy across more than one ecosystem
