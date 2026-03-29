<!-- {@discoverySupportedSources} -->

- Cargo workspaces and standalone crates
- npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages
- Deno workspaces and standalone `deno.json` / `deno.jsonc` packages
- Dart and Flutter workspaces plus standalone `pubspec.yaml` packages

<!-- {/discoverySupportedSources} -->

<!-- {@discoveryKeyBehaviors} -->

- native workspace globs are expanded by each ecosystem adapter
- dependency names are normalized into one graph
- version-group assignments are attached after discovery
- unmatched group members and version mismatches produce warnings
- discovery currently scans all supported ecosystems regardless of `[ecosystems.*]` toggles in `monochange.toml`

<!-- {/discoveryKeyBehaviors} -->

<!-- {@configurationDefaultsSnippet} -->

```toml
[defaults]
parent_bump = "patch"
include_private = false
warn_on_group_mismatch = true
```

<!-- {/configurationDefaultsSnippet} -->

<!-- {@configurationVersionGroupsSnippet} -->

```toml
[package.sdk-core]
path = "crates/sdk_core"
type = "cargo"
changelog = "crates/sdk_core/CHANGELOG.md"
versioned_files = ["crates/sdk_core/extra.toml"]
tag = false
release = false
version_format = "namespaced"
```

<!-- {/configurationVersionGroupsSnippet} -->

<!-- {@configurationPackageOverridesSnippet} -->

Legacy repositories may still contain `[[package_overrides]]` entries such as:

```toml
[[package_overrides]]
package = "crates/sdk_core"
changelog = "crates/sdk_core/CHANGELOG.md"
```

Under the new model, move that changelog configuration onto the matching `[package.<id>]` declaration instead.

<!-- {/configurationPackageOverridesSnippet} -->

<!-- {@configurationWorkflowsSnippet} -->

```toml
[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Command"
command = "cargo test --workspace --all-features"
```

<!-- {/configurationWorkflowsSnippet} -->

<!-- {@configurationWorkflowVariables} -->

- `$version` — one shared release version when all released packages resolve to the same version
- `$group_version` — one shared synced version across released version groups, falling back to `$version`
- `$released_packages` — comma-separated released package names
- `$changed_files` — space-separated changed file paths
- `$changesets` — space-separated consumed `.changeset/*.md` paths

<!-- {/configurationWorkflowVariables} -->

<!-- {@configurationEcosystemSettingsSnippet} -->

```toml
[ecosystems.cargo]
enabled = true
roots = ["crates/*"]
exclude = ["crates/experimental/*"]

[ecosystems.npm]
enabled = true
roots = ["packages/*"]
exclude = ["packages/legacy/*"]

[ecosystems.deno]
enabled = true

[ecosystems.dart]
enabled = true
```

<!-- {/configurationEcosystemSettingsSnippet} -->

<!-- {@configurationPackageReferenceRules} -->

Package references in changesets and CLI commands should use configured package ids or group ids. Legacy manifest-relative paths and directory paths may still appear in older repos during migration, but `mc check` should guide you toward declared ids.

<!-- {/configurationPackageReferenceRules} -->

<!-- {@configurationCurrentStatus} -->

Current implementation notes:

- `defaults.include_private` is parsed, but discovery behavior is still centered on the supported fixture-driven workflows in this milestone
- `version_groups.strategy` belongs to the legacy model and should be migrated to `[group.<id>]`
- `[ecosystems.*].enabled/roots/exclude` are parsed and documented as the ecosystem control surface
- `package_overrides.changelog` is a legacy setting that should be migrated to package declarations
- supported workflow steps today are `PrepareRelease` and `Command`

<!-- {/configurationCurrentStatus} -->

<!-- {@versionGroupsExample} -->

```toml
[package.sdk-core]
path = "cargo/sdk-core"
type = "cargo"

[package.web-sdk]
path = "packages/web-sdk"
type = "npm"

[group.sdk]
packages = ["sdk-core", "web-sdk"]
tag = true
release = true
version_format = "primary"
```

<!-- {/versionGroupsExample} -->

<!-- {@versionGroupsBehavior} -->

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- the group owns outward release identity
- member package changelogs can still be updated individually
- group changelog and group `versioned_files` can also be updated
- dependents of newly synced members still receive propagated parent bumps
- unmatched members produce warnings during discovery
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

<!-- {@versionGroupsCurrentStatus} -->

Legacy `version_groups.strategy` is no longer the primary authoring model. The current implementation always derives synchronized release behavior from `[group.<id>]` declarations.

<!-- {/versionGroupsCurrentStatus} -->

<!-- {@releaseChangesAddCommand} -->

```bash
mc changes add --root . --package sdk-core --bump minor --reason "public API addition"
```

<!-- {/releaseChangesAddCommand} -->

<!-- {@releaseManualChangesetExample} -->

```markdown
---
sdk-core: minor
---

#### public API addition
```

<!-- {/releaseManualChangesetExample} -->

<!-- {@releaseEvidenceExample} -->

```markdown
---
sdk-core: patch
evidence:
  sdk-core:
    - rust-semver:major:public API break detected
---

#### breaking API change
```

<!-- {/releaseEvidenceExample} -->

<!-- {@releasePlanningRules} -->

- `mc changes add` defaults `--bump` to `patch`
- markdown change files require an explicit `patch`, `minor`, or `major` entry per package
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- configured groups synchronize before final output is rendered
- release targets carry effective `tag`, `release`, and `version_format` metadata

<!-- {/releasePlanningRules} -->

<!-- {@releaseWorkflowBehavior} -->

`mc release` only works when your config defines a workflow named `release`.

During migration, you may still see references to `[[package_overrides]]` in older documentation or repositories, but release preparation now expects package/group declarations and consumes `.changeset/*.md` files through that new model.

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates native manifests plus configured changelogs and versioned files
- applies group-owned release identity for outward `tag`, `release`, and `version_format`
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run`

<!-- {/releaseWorkflowBehavior} -->
