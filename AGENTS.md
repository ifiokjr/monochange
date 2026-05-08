# AGENTS

`monochange` is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems.

## Workspace essentials

- Tooling: use `cargo` in the `devenv` shell; repository task commands are exposed as `devenv` scripts. Use `direnv allow` to auto-populate `PATH` with devenv scripts, or prefix commands with `devenv shell` (e.g. `devenv shell fix:all`).
- Fix all warnings: `fix:all`
- Build: `build:all`
- Quality/typecheck: `lint:all`
- Validation: `mc validate`
- Patch coverage: every pull request must keep patch coverage at 100% for executable changed lines. Add or adjust tests until `coverage:patch` is green.
- New integration tests must live in `crates/monochange_integration_tests`, use file fixtures instead of dynamically generated fixtures, and use Insta snapshots for integration output assertions.
- Snapshot readability: JSON snapshots must not embed multiline strings with escaped `\n` sequences. Redact multiline JSON fields as `"[multiline text]"` and add separate string snapshots for the multiline contents.
- Snapshot relevance: `test:cargo` and CI reject unreferenced `.snap` files. Use `snapshot:update` to regenerate snapshots and delete unreferenced snapshot files.

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
- [GitHub templates](docs/agents/github-templates.md)
- [Testing requirements](docs/agents/testing.md)
- [Documentation workflow](docs/agents/documentation.md)
- [Architecture boundaries](docs/agents/architecture.md)
- [Product and architecture rules](docs/agents/product-rules.md)
- [Rust quality and safety](docs/agents/rust-quality.md)
- [Coding style](docs/agents/coding-style.md)
- [Changeset quality](docs/agents/changeset-quality.md)
- [Plans and execution notes](docs/plans/README.md)
