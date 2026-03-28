# Contributing to monochange

Thank you for contributing.

## Development environment

This repository uses `devenv` for a reproducible shell.

```sh
devenv shell
install:all
```

## Expected workflow

1. Create a feature branch from `main`.
2. Write failing tests first for non-trivial behavior.
3. Implement the smallest change that makes the tests pass.
4. Update docs, READMEs, and fixtures when behavior changes.
5. Run the full local validation suite before opening a PR.

## Core commands

```sh
monochange --help
mc --help
mc changes add --root . --package crates/monochange --bump patch --reason "describe the change"
lint:all
test:all
build:all
build:book
```

## Product rules

- Keep `crates/monochange` as the CLI package.
- Keep `crates/monochange_core` focused on shared domain types.
- Put adapter-specific manifest behavior in ecosystem crates.
- Preserve fixture-first validation for discovery and planning behavior.
- Treat `docs/` as a product surface, not an afterthought.

## Testing requirements

- Every non-trivial behavior change starts with a failing test.
- Release-planning logic needs realistic fixture coverage.
- Cross-ecosystem behavior should remain consistent across Cargo, npm-family, Deno, Dart, and Flutter.

## Safety and linting constraints

- `unsafe_code` is denied.
- `unstable_features` is denied.
- strict clippy and formatting checks stay enabled.
- explicit panic context is preferred over bare `.expect(...)`.
