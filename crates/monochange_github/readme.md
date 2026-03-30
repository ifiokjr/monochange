# `monochange_github`

<br />

<!-- {=crateReadmeBadgeRow} -->

[![Crates.io][crate-image]][crate-link] [![Docs.rs][docs-image]][docs-link] [![CI][ci-status-image]][ci-status-link] [![Coverage][coverage-image]][coverage-link] [![License][license-image]][license-link]

<!-- {/crateReadmeBadgeRow} -->

<br />

<!-- {=monochangeGithubCrateDocs} -->

`monochange_github` turns `MonoChange` release manifests into GitHub automation requests.

Reach for this crate when you want to preview or publish GitHub releases and release pull requests using the same structured release data that powers changelog files and release manifests.

## Why use it?

- derive GitHub release payloads and release-PR bodies from `MonoChange`'s structured release manifest
- keep GitHub automation aligned with changelog rendering and release targets
- reuse one publishing path for dry-run previews and real repository updates

## Best for

- building GitHub release automation on top of `mc release`
- previewing would-be GitHub releases and release PRs in CI before publishing
- converting grouped or package release targets into repository automation payloads

## Public entry points

- `build_release_requests(config, manifest)` converts a release manifest into GitHub release requests
- `publish_release_requests(requests)` publishes requests through the GitHub API via `octocrab`
- `build_release_pull_request_request(config, manifest)` converts a release manifest into a GitHub release-PR request
- `publish_release_pull_request(root, request, tracked_paths)` creates or updates a release PR through `git` and the GitHub API

## Example

```rust
use monochange_core::GitHubBotSettings;
use monochange_core::GitHubConfiguration;
use monochange_core::GitHubPullRequestSettings;
use monochange_core::GitHubReleaseSettings;
use monochange_core::ReleaseManifest;
use monochange_core::ReleaseManifestPlan;
use monochange_core::ReleaseManifestTarget;
use monochange_core::ReleaseOwnerKind;
use monochange_core::VersionFormat;
use monochange_github::build_release_requests;

let manifest = ReleaseManifest {
    command: "release".to_string(),
    dry_run: true,
    version: Some("1.2.0".to_string()),
    group_version: Some("1.2.0".to_string()),
    release_targets: vec![ReleaseManifestTarget {
        id: "sdk".to_string(),
        kind: ReleaseOwnerKind::Group,
        version: "1.2.0".to_string(),
        tag: true,
        release: true,
        version_format: VersionFormat::Primary,
        tag_name: "v1.2.0".to_string(),
        members: vec!["core".to_string(), "app".to_string()],
    }],
    released_packages: vec!["workflow-core".to_string(), "workflow-app".to_string()],
    changed_files: Vec::new(),
    changelogs: Vec::new(),
    deleted_changesets: Vec::new(),
    deployments: Vec::new(),
    plan: ReleaseManifestPlan {
        workspace_root: std::path::PathBuf::from("."),
        decisions: Vec::new(),
        groups: Vec::new(),
        warnings: Vec::new(),
        unresolved_items: Vec::new(),
        compatibility_evidence: Vec::new(),
    },
};
let github = GitHubConfiguration {
    owner: "ifiokjr".to_string(),
    repo: "monochange".to_string(),
    releases: GitHubReleaseSettings::default(),
    pull_requests: GitHubPullRequestSettings::default(),
    bot: GitHubBotSettings::default(),
};

let requests = build_release_requests(&github, &manifest);

assert_eq!(requests.len(), 1);
assert_eq!(requests[0].tag_name, "v1.2.0");
assert_eq!(requests[0].repository, "ifiokjr/monochange");
```

<!-- {/monochangeGithubCrateDocs} -->

<!-- {=monochangeGithubBadgeLinks} -->

[crate-image]: https://img.shields.io/badge/crates.io-monochange__github-orange?logo=rust
[crate-link]: https://crates.io/crates/monochange_github
[docs-image]: https://img.shields.io/badge/docs.rs-monochange__github-1f425f?logo=docs.rs
[docs-link]: https://docs.rs/monochange_github/

<!-- {/monochangeGithubBadgeLinks} -->

<!-- {=repoStatusBadgeLinks} -->

[ci-status-image]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml/badge.svg
[ci-status-link]: https://github.com/ifiokjr/monochange/actions/workflows/ci.yml
[coverage-image]: https://codecov.io/gh/ifiokjr/monochange/branch/main/graph/badge.svg
[coverage-link]: https://codecov.io/gh/ifiokjr/monochange
[license-image]: https://img.shields.io/badge/license-Unlicense-blue.svg
[license-link]: https://opensource.org/license/unlicense

<!-- {/repoStatusBadgeLinks} -->
