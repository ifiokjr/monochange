# `monochange_hosting`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeHostingCrateDocs} -->

`monochange_hosting` packages the shared git and HTTP plumbing used by hosted source providers.

Reach for this crate when you are implementing GitHub, Gitea, or GitLab release adapters and want one place for release-body rendering, change-request branch naming, JSON requests, and git branch orchestration.

## Why use it?

- keep provider adapters focused on provider-specific payloads instead of repeated plumbing
- share one markdown rendering path for release bodies and release pull requests
- reuse one set of blocking HTTP helpers with consistent error messages

## Best for

- implementing or testing hosted source adapters
- generating release pull request bodies from prepared manifests
- staging, committing, and pushing release branches through shared wrappers

## Public entry points

- `release_body(source, manifest, target)` resolves the outward release body for a target
- `release_pull_request_body(manifest)` renders the provider change-request body
- `release_pull_request_branch(prefix, command)` normalizes the change-request branch name
- `get_json`, `post_json`, `patch_json`, and `put_json` wrap provider API requests
- `git_checkout_branch`, `git_stage_paths`, `git_commit_paths`, and `git_push_branch` wrap shared git operations

<!-- {/monochangeHostingCrateDocs} -->

<!-- {=crateBadgeLinks:"monochange_hosting"} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__hosting-orange?logo=rust [crate-link]: https://crates.io/crates/monochange_hosting [docs-image]: https://img.shields.io/badge/docs.rs-monochange__hosting-1f425f?logo=docs.rs [docs-link]: https://docs.rs/monochange_hosting/

<!-- {/crateBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
