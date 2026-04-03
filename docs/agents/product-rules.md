# Product and architecture rules

- Keep `crates/monochange` as the CLI package.
- Keep `crates/monochange_core` focused on shared domain types.
- Put adapter-specific manifest behavior in ecosystem crates.
- Preserve fixture-first validation for discovery and planning behavior.
- Prefer configured package ids and group ids over raw manifest paths in changesets and docs.
- **Keep init template in sync**: whenever configuration options are added, removed, or renamed in `monochange_core` (structs like `WorkspaceDefaults`, `PackageDefinition`, `GroupDefinition`, `SourceConfiguration`, `DeploymentDefinition`, `ChangesetVerificationSettings`, `ReleaseNotesSettings`, `EcosystemSettings`, `CliStepDefinition`, `CliInputDefinition`) or in `monochange_config` parsing, update `crates/monochange/src/monochange.init.toml` so that `mc init` always generates a fully annotated reference file covering every available option.
