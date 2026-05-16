# `monochange_forgejo`

<br />

<!-- {=crateReadmeBadgeRow:"monochange_forgejo"} -->

[![Crates.io](https://img.shields.io/badge/crates.io-monochange**forgejo-orange?logo=rust)](https://crates.io/crates/monochange_forgejo) [![Docs.rs](https://img.shields.io/badge/docs.rs-monochange**forgejo-1f425f?logo=docs.rs)](https://docs.rs/monochange_forgejo/) [![CI](https://github.com/monochange/monochange/actions/workflows/ci.yml/badge.svg)](https://github.com/monochange/monochange/actions/workflows/ci.yml) [![Coverage](https://codecov.io/gh/monochange/monochange/branch/main/graph/badge.svg?flag=monochange_forgejo)](https://codecov.io/gh/monochange/monochange?flag=monochange_forgejo) [![License](https://img.shields.io/badge/license-Unlicense-blue.svg)](https://opensource.org/license/unlicense)

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeForgejoCrateDocs} -->

`monochange_forgejo` turns `monochange` release manifests into Forgejo automation requests.

Reach for this crate when you want to preview or publish Forgejo releases and release pull requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive Forgejo release payloads and release-PR bodies from `monochange`'s structured release manifest
- keep Forgejo automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building Forgejo release automation on top of `mc step:prepare-release` and `mc step:publish-release`
- previewing would-be Forgejo releases and release PRs in CI before publishing
- self-hosted Forgejo instances that need the same release workflow as GitHub or GitLab

## Public entry points

- `build_release_requests(manifest, source)` builds release payloads from prepared release state
- `build_change_request(manifest, source)` builds a pull-request payload for the release
- `validate_source_configuration(source)` validates Forgejo-specific source config
- `source_capabilities()` returns provider feature flags

<!-- {/monochangeForgejoCrateDocs} -->
