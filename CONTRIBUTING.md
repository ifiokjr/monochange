# Contributing to monochange

Thank you for your interest in contributing to monochange. This guide covers the expected development workflow.

## Getting Started

This project uses [devenv](https://devenv.sh/) for a reproducible development environment.

```sh
# Enter the dev shell
devenv shell

# Install cargo-managed tooling used by the repository
install:all
```

## Building and Testing

```sh
# Build workspace crates and docs
build:all
build:book

# Preferred test runner
test:all

# Individual test commands
test:cargo
test:docs
```

## Formatting

Formatting is handled by **dprint**.

```sh
# Check formatting
lint:format

# Apply formatting
fix:format
```

## Linting

```sh
# Clippy + formatting + cargo-deny
lint:all

# Individual checks
lint:clippy
deny:check
```

## CLI Shortcuts

```sh
monochange --help
mc --help
```

## Pull Request Workflow

Every change must be submitted via a pull request. Do not commit directly to `main`.

1. Create a feature branch from `main`.
2. Write or update tests before implementing non-trivial behavior changes.
3. Keep documentation in sync with the code.
4. Run `lint:all`, `test:all`, `build:all`, and `build:book` locally.
5. Open a pull request and wait for CI to pass before merging.

## Test Requirements

- Every non-trivial behavior change must begin with a failing test.
- Bug fixes must include a regression test that fails before the fix.
- Tests should cover edge cases, error paths, and realistic monorepo workflows.

## Safety and Linting Constraints

The repository enforces strict workspace rules:

- `unsafe_code` is denied
- `unstable_features` is denied
- `clippy::correctness` is denied
- `clippy::wildcard_dependencies` is denied
- `Result::expect` is disallowed in favor of explicit error handling or explicit panic context
