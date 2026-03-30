# `monochange_core`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeCoreCrateDocs} -->

`monochange_core` is the shared vocabulary for the `monochange` workspace.

Reach for this crate when you are building ecosystem adapters, release planners, or custom automation and need one set of types for packages, dependency edges, version groups, change signals, and release plans.

## Why use it?

- avoid redefining package and release domain models in each crate
- share one error and result surface across discovery, planning, and workflow layers
- pass normalized workspace data between adapters and planners without extra translation

## Best for

- implementing new ecosystem adapters against the shared `EcosystemAdapter` contract
- moving normalized package or release data between crates without custom conversion code
- depending on the workspace domain model without pulling in discovery or planning behavior

## What it provides

- normalized package and dependency records
- version-group definitions and planned group outcomes
- change signals and compatibility assessments
- changelog formats, changelog targets, structured release-note types, release-manifest types, and GitHub automation config types
- shared error and result types

## Example

```rust
use monochange_core::render_release_notes;
use monochange_core::ChangelogFormat;
use monochange_core::ReleaseNotesDocument;
use monochange_core::ReleaseNotesSection;

let notes = ReleaseNotesDocument {
    title: "1.2.3".to_string(),
    summary: vec!["Grouped release for `sdk`.".to_string()],
    sections: vec![ReleaseNotesSection {
        title: "Features".to_string(),
        entries: vec!["- add keep-a-changelog output".to_string()],
    }],
};

let rendered = render_release_notes(ChangelogFormat::KeepAChangelog, &notes);

assert!(rendered.contains("## [1.2.3]"));
assert!(rendered.contains("### Features"));
assert!(rendered.contains("- add keep-a-changelog output"));
```

<!-- {/monochangeCoreCrateDocs} -->

<!-- {=monochangeCoreBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__core-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_core
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__core-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_core/

<!-- {/monochangeCoreBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
