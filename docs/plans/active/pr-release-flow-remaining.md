# Pull request release flow: remaining work

## Task 1: Automated tests for GitHub slash command flow

### Status: ✅ Done

Added 9 tests to `crates/monochange_github/src/__tests.rs`:

- `authorize_slash_command_release_allows_explicit_user`
- `authorize_slash_command_release_denies_unlisted_user`
- `authorize_slash_command_release_empty_config_allows_everyone`
- `authorize_slash_command_release_allows_admin`
- `authorize_slash_command_release_denies_non_admin_when_admins_only`
- `authorize_slash_command_release_allows_team_member`
- `authorize_slash_command_release_denies_inactive_team_member`
- `merge_release_pull_request_success`
- `merge_release_pull_request_not_found`

All 52 tests in `monochange_github` pass.

## Task 2: API release publishing after merge

### Status: ✅ Done

After local git tags are created, `merge-release-pr` should also publish provider releases via the API. The `MergeReleasePrReport` may need a `release_outcomes` field.

## Task 3: GitLab adapter

### Status: ✅ Done

Implementing `authorize_slash_command_release` and `merge_release_pull_request` for GitLab using the reqwest blocking client pattern.

## Task 4: Gitea adapter

### Status: ✅ Done

Implementing `authorize_slash_command_release` and `merge_release_pull_request` for Gitea using the reqwest blocking client pattern.
