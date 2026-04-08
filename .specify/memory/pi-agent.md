# monochange Development Guidelines

Auto-generated from feature plans. Last updated: 2026-03-25

## Active Technologies

- Rust 2021 workspace with MSRV 1.86.0
- CLI surface in `crates/monochange`
- Shared domain logic in `crates/monochange_core`
- Ecosystem adapters for Cargo, npm-family, Deno, and Dart/Flutter
- `devenv` for reproducible development environments
- `dprint` for formatting
- `cargo nextest`, doc tests, `rstest`, and `insta` for testing
- mdBook documentation in `docs/`

## Project Structure

```text
crates/
├── monochange/
├── monochange_core/
├── monochange_config/
├── monochange_graph/
├── monochange_semver/
├── monochange_cargo/
├── monochange_npm/
├── monochange_deno/
└── monochange_dart/

docs/
fixtures/
specs/
setup/
```

## Commands

- `devenv shell`
- `install:all`
- `build:all`
- `build:book`
- `lint:all`
- `test:all`
- `snapshot:review`
- `snapshot:update`
- `monochange --help`
- `mc --help`

## Code Style

- Use test-first development for non-trivial behavior
- Prefer focused crates over monolithic implementations
- Keep adapter-specific manifest logic out of shared planning crates
- Treat docs as a product surface and update the mdBook with behavior changes
- Preserve strict lint, formatting, and safety gates

## Recent Changes

- `001-first-step-port`: planned cross-ecosystem release discovery, graph propagation, version groups, semver-aware parent bumping, and docs-book-first delivery

<!-- MANUAL ADDITIONS START -->
<!-- MANUAL ADDITIONS END -->
