# `monochange_gitea`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeGiteaCrateDocs} -->

`monochange_gitea` turns `monochange` release manifests into Gitea automation requests.

Reach for this crate when you want to preview or publish Gitea releases and release pull requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive Gitea release payloads and release-PR bodies from `monochange`'s structured release manifest
- keep Gitea automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building Gitea release automation on top of `mc release`
- previewing would-be Gitea releases and release PRs in CI before publishing
- self-hosted Gitea instances that need the same release workflow as GitHub or GitLab

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a pull-request payload for the release
- `validate_source_configuration(source)` validates Gitea-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeGiteaCrateDocs} -->

<!-- {=monochangeGiteaBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__gitea-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_gitea
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__gitea-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_gitea/

<!-- {/monochangeGiteaBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
