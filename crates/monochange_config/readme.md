# `monochange_config`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeConfigCrateDocs} -->

`monochange_config` parses and validates the inputs that drive planning and release commands.

Reach for this crate when you need to load `monochange.toml`, resolve package references, or turn `.changeset/*.md` files into validated change signals for the planner.

## Why use it?

- centralize config parsing and validation rules in one place
- resolve package references against discovered workspace packages
- keep CLI command definitions, version groups, and change files aligned with the planner's expectations

## Best for

- validating configuration before handing it to planning code
- parsing and resolving change files in custom automation
- keeping package-reference rules consistent across tools

## Public entry points

- `load_workspace_configuration(root)` loads and validates `monochange.toml`
- `load_change_signals(root, changes_dir, packages)` parses markdown change files into change signals
- `resolve_package_reference(reference, workspace_root, packages)` maps package names, ids, and paths to discovered packages
- `apply_version_groups(packages, configuration)` attaches configured version groups to discovered packages

## Responsibilities

- load `monochange.toml`
- validate version groups and CLI commands
- resolve package references against discovered packages
- parse change-input files, evidence, release-note `type` / `details` fields, changelog paths, changelog format overrides, source-provider config, changeset-bot policy config, and command release/manifest/policy steps

## Example

```rust
use monochange_config::load_workspace_configuration;
use monochange_core::ChangelogFormat;

let root = std::env::temp_dir().join("monochange-config-changelog-format-docs");
let _ = std::fs::remove_dir_all(&root);
std::fs::create_dir_all(root.join("crates/core")).unwrap();
std::fs::write(
    root.join("crates/core/Cargo.toml"),
    "[package]\nname = \"core\"\nversion = \"1.0.0\"\n",
)
.unwrap();
std::fs::write(
    root.join("monochange.toml"),
    r#"
[defaults]
package_type = "cargo"

[defaults.changelog]
path = "{{ path }}/CHANGELOG.md"
format = "keep_a_changelog"

[package.core]
path = "crates/core"
"#,
)
.unwrap();

let configuration = load_workspace_configuration(&root).unwrap();
let package = configuration.package_by_id("core").unwrap();

assert_eq!(configuration.defaults.changelog_format, ChangelogFormat::KeepAChangelog);
assert_eq!(package.changelog.as_ref().unwrap().format, ChangelogFormat::KeepAChangelog);
assert_eq!(package.changelog.as_ref().unwrap().path, std::path::PathBuf::from("crates/core/CHANGELOG.md"));

let _ = std::fs::remove_dir_all(&root);
```

<!-- {/monochangeConfigCrateDocs} -->

<!-- {=monochangeConfigBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__config-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_config
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__config-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_config/

<!-- {/monochangeConfigBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
