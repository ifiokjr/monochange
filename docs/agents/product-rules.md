# Product and architecture rules

- Keep `crates/monochange` as the CLI package.
- Keep `crates/monochange_core` focused on shared domain types and capability contracts, not adapter implementation details.
- Put adapter-specific manifest behavior in ecosystem crates.
- Put provider-specific source automation behavior in source crates (`monochange_github`, `monochange_gitlab`, `monochange_gitea`).
- `crates/monochange` may orchestrate adapters, but it must not implement ecosystem/provider-specific file parsing, mutation rules, API payload shaping, or capability matrices.
- `crates/monochange_config` should keep parsing focused on the supported configuration surface and delegate ecosystem/provider validation to adapter crates whenever behavior depends on a specific implementation.
- Preserve fixture-first validation for discovery and planning behavior.
- Prefer configured package ids and group ids over raw manifest paths in changesets and docs.
- When adding a new feature, decide first whether it is a shared capability (belongs in core) or an implementation of an existing capability (belongs in an adapter crate).
- New `match` branches on `EcosystemType` or `SourceProvider` outside adapter dispatch should be treated as architecture smells and justified explicitly in the PR.
- **Keep init template in sync**: whenever configuration options are added, removed, or renamed in `monochange_core` (structs like `WorkspaceDefaults`, `PackageDefinition`, `GroupDefinition`, `SourceConfiguration`, `DeploymentDefinition`, `ChangesetAffectedSettings`, `ReleaseNotesSettings`, `EcosystemSettings`, `CliStepDefinition`, `CliInputDefinition`) or in `monochange_config` parsing, update `crates/monochange/src/monochange.init.toml` so that `mc init` always generates a fully annotated reference file covering every available option.
