# Migrating from legacy config

The package/group configuration API replaces the earlier `version_groups` and `package_overrides` model.

## Old style

```toml
[[version_groups]]
name = "workspace"
members = ["crates/monochange", "crates/monochange_core"]

[[package_overrides]]
package = "crates/monochange"
changelog = "crates/monochange/changelog.md"
```

## New style

```toml
[defaults]
package_type = "cargo"

[package.monochange]
path = "crates/monochange"
changelog = "crates/monochange/changelog.md"

[package.monochange_core]
path = "crates/monochange_core"
changelog = "crates/monochange_core/changelog.md"

[group.main]
packages = ["monochange", "monochange_core"]
tag = true
release = true
version_format = "primary"
```

## Migration rules

- move each package override into a `[package.<id>]` table
- set `[defaults].package_type` when the repository uses a single ecosystem so package entries can omit `type`
- use package ids in changesets instead of raw manifest paths
- move each legacy version group into `[group.<id>]`
- keep package changelog configuration on the package declaration
- use group-level `tag`, `release`, `version_format`, `changelog`, and `versioned_files` for shared outward release behavior

## Recommended migration flow

1. Add `[package.<id>]` entries for every release-managed package.
2. Replace each legacy version group with `[group.<id>]`.
3. Update existing `.changeset/*.md` files to use package ids or group ids.
4. Run `mc validate` and fix any reported issues.
5. Run `mc release --dry-run` to verify the resulting release targets.
