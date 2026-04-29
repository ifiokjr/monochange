---
monochange: minor
monochange_core: minor
monochange_github: minor
---

# Add post-merge release automation

- Add `release-pr-manual-merge-blocker` job to CI that fails on PRs from `monochange/release/*` branches, forcing the `/merge` slash-command workflow
- Protect the `release-pr` job with `environment: publisher` so branch-protection rules apply
- Introduce a `release-post-merge` job that runs `PublishRelease` and `CommentReleasedIssues` steps after a release PR merges
- Add `from-ref` support to `PublishRelease` and `CommentReleasedIssues` for discovering the release record from the merge commit when `prepared_release` context is unavailable
- Add `auto-close-issues` flag to `CommentReleasedIssues` that closes released issues not already closed by a PR reference
- Store `changesets` in `ReleaseRecord` so post-merge steps can resolve related issues without access to the deleted changeset files
- Update `plan_released_issue_comments` to include all issue relationships and set `close` state appropriately
- Update `comment_released_issues_with_client` to PATCH issue state to `"closed"` when `plan.close` is `true`
- Add dedicated composite actions: `publish-release` and `comment-released-issues`
- Add `publish-release` and `comment-released-issues` CLI step definitions to `monochange.init.toml`
