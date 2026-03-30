# AGENTS

`monochange` is a Rust-based release-planning toolkit for monorepos that span multiple package ecosystems.

## Workspace essentials

- Tooling: use `cargo` in the `devenv` shell; repository task commands are exposed as `devenv` scripts.
- Build: `build:all`
- Quality/typecheck: `lint:all`
- Validation: `mc validate`

## Task-specific guidance

- [Tooling and commands](docs/agents/tooling.md)
- [Workflow expectations](docs/agents/workflow.md)
- [Testing requirements](docs/agents/testing.md)
- [Documentation workflow](docs/agents/documentation.md)
- [Product and architecture rules](docs/agents/product-rules.md)
- [Rust quality and safety](docs/agents/rust-quality.md)
