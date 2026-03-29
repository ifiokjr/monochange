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
[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk", "packages/mobile-sdk"]
strategy = "shared"
```

<!-- {/configurationVersionGroupsSnippet} -->

<!-- {@configurationPackageOverridesSnippet} -->

```toml
[[package_overrides]]
package = "crates/sdk_core"
changelog = "crates/sdk_core/CHANGELOG.md"

[[package_overrides]]
package = "packages/web-sdk"
changelog = "packages/web-sdk/CHANGELOG.md"
```

<!-- {/configurationPackageOverridesSnippet} -->

<!-- {@configurationWorkflowsSnippet} -->

```toml
[[workflows]]
name = "release"

[[workflows.steps]]
type = "PrepareRelease"

[[workflows.steps]]
type = "Command"
command = "printf '%s\n' '$version'"
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

Package references may use package ids, package names, manifest-relative paths, or manifest-directory paths.

<!-- {/configurationPackageReferenceRules} -->

<!-- {@configurationCurrentStatus} -->

Current implementation status:

- actively used today: `defaults.parent_bump`, `defaults.warn_on_group_mismatch`, `version_groups`, `package_overrides.changelog`, and `workflows`
- parsed but not currently enforced by discovery or planning: `defaults.include_private`, `version_groups.strategy`, and `[ecosystems.*].enabled/roots/exclude`
- workflow names must be unique, must not collide with built-in commands, and must contain at least one step
- supported workflow steps today: `PrepareRelease` and `Command`

<!-- {/configurationCurrentStatus} -->

<!-- {@versionGroupsExample} -->

```toml
[[version_groups]]
name = "sdk"
members = ["crates/sdk_core", "packages/web-sdk"]
strategy = "shared"
```

<!-- {/versionGroupsExample} -->

<!-- {@versionGroupsBehavior} -->

- the highest required bump in the group wins
- every member in the group receives that bump
- one planned group version is calculated from the highest current member version
- dependents of newly synced members still receive propagated parent bumps
- unmatched members produce warnings during discovery
- mismatched current versions produce warnings when `warn_on_group_mismatch = true`

<!-- {/versionGroupsBehavior} -->

<!-- {@versionGroupsCurrentStatus} -->

`strategy` is parsed from config, but the current implementation always applies shared synchronized versioning behavior.

<!-- {/versionGroupsCurrentStatus} -->

<!-- {@releaseChangesAddCommand} -->

```bash
mc changes add --root . --package sdk_core --bump minor --reason "public API addition"
```

<!-- {/releaseChangesAddCommand} -->

<!-- {@releaseManualChangesetExample} -->

```markdown
---
sdk_core: minor
---

#### public API addition
```

<!-- {/releaseManualChangesetExample} -->

<!-- {@releaseEvidenceExample} -->

```markdown
---
sdk_core: patch
origin:
  sdk_core: direct-change
evidence:
  sdk_core:
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
- version-group synchronization runs before final output is rendered

<!-- {/releasePlanningRules} -->

<!-- {@releaseWorkflowBehavior} -->

Current `PrepareRelease` behavior:

- reads `.changeset/*.md`
- computes one synchronized release plan from discovered change files
- updates Cargo package versions and Cargo workspace dependency versions when a release is applied
- appends changelog sections only for packages configured through `[[package_overrides]]` with `changelog` paths
- deletes consumed change files only after a successful non-dry-run execution
- leaves the workspace untouched during `--dry-run`

<!-- {/releaseWorkflowBehavior} -->
