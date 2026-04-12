# AGENTS

`monochange` is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems.

## Workspace essentials

- Tooling: use `cargo` in the `devenv` shell; repository task commands are exposed as `devenv` scripts. Use `direnv allow` to auto-populate `PATH` with devenv scripts, or prefix commands with `devenv shell` (e.g. `devenv shell fix:all`).
- Fix all warnings: `fix:all`
- Build: `build:all`
- Quality/typecheck: `lint:all`
- Validation: `mc validate`

## Naming convention

- The project name is always written in **all lowercase**: `monochange`.
- Never use `MonoChange`, `Monochange`, or `MONOCHANGE` in prose, docs, comments, or string literals.
- Issue titles must use sentence case, must not end with a full stop, and must not use conventional-commit prefixes such as `feat:` or `fix:`.
- Pull request titles must use conventional-commit style prefixes.
- Commit titles must use conventional-commit style prefixes.
- **Rust code exception**: standard Rust naming conventions apply.
  - Structs and enums may use PascalCase (e.g. `MonochangeError`, `MonochangeResult`).
  - Constants use UPPER_SNAKE_CASE (e.g. `MONOCHANGE_VERSION`).
  - Variables and functions use snake_case (e.g. `monochange_config`).

## Git rules

- Never use `--no-verify` with `git commit` or `git push`.
- Never change `commit.gpgsign` in local, global, or workspace git config.
- The only allowed exception is during `git rebase` workflows when a rebase continuation or amend step would otherwise block on hooks/editor behavior.

## Task-specific guidance

- [Tooling and commands](docs/agents/tooling.md)
- [Workflow expectations](docs/agents/workflow.md)
- [Testing requirements](docs/agents/testing.md)
- [Documentation workflow](docs/agents/documentation.md)
- [Product and architecture rules](docs/agents/product-rules.md)
- [Rust quality and safety](docs/agents/rust-quality.md)
- [Changeset quality](docs/agents/changeset-quality.md)
