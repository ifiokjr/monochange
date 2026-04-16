# `monochange_gitlab`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeGitlabCrateDocs} -->

`monochange_gitlab` turns `monochange` release manifests into GitLab automation requests.

Reach for this crate when you want to preview or publish GitLab releases and merge requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive GitLab release payloads and merge-request bodies from `monochange`'s structured release manifest
- keep GitLab automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building GitLab release automation on top of `mc release`
- previewing would-be GitLab releases and merge requests in CI before publishing
- self-hosted GitLab instances that need the same release workflow as GitHub

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a merge-request payload for the release
- `validate_source_configuration(source)` validates GitLab-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeGitlabCrateDocs} -->

<!-- {=crateBadgeLinks:"monochange_gitlab"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__gitlab-orange?logo=rust [crate-link]: https://crates.io/crates/monochange_gitlab [docs-image]: https://img.shields.io/badge/docs.rs-monochange__gitlab-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange_gitlab/

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
