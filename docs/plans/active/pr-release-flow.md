# Pull request release flow

## Status: Implemented (v1) — merged or pending next steps

## Problem

Releases required creating a local `commit-release` and pushing it directly to the default branch. We wanted to support a pull-request-based flow where releases are prepared, reviewed, and merged via PRs, with the squash-merge and tagging happening automatically when an authorized user types `/release`.

## Design decisions

- **Authorization**: explicit allow-list (`allowed_users`, `allowed_teams`) + repo admins by default. Controlled via `source.pull_requests.slash_commands.authorization` in `monochange.toml`.
- **Merge mechanism**: GitHub API squash-merge with a computed release commit message.
- **Release record**: Recomputed at merge time from the PR branch working tree (the CI workflow checks out `refs/pull/{n}/head`).
- **Cross-provider abstractions**: New trait methods on `HostedSourceAdapter` in `monochange_core` so GitLab and Gitea can be wired in later.

## Files changed

| Crate               | File                                      | Change                                                                                                                                                                                                                                                   |
| ------------------- | ----------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `monochange_core`   | `src/lib.rs`                              | Added `ProviderSlashCommandSettings`, `SlashCommandAuthorizationSettings`, `MergeReleasePullRequestRequest`, `MergeReleasePullRequestOutcome`, `MergeReleasePrReport`, `SlashCommandAuthorizationResult`, and new trait methods on `HostedSourceAdapter` |
| `monochange_github` | `src/lib.rs`                              | Implemented `authorize_slash_command_release` and `merge_release_pull_request` using octocrab                                                                                                                                                            |
| `monochange`        | `src/cli.rs`                              | Added `build_merge_release_pr_subcommand()` built-in command                                                                                                                                                                                             |
| `monochange`        | `src/lib.rs`                              | Dispatched `merge-release-pr` subcommand to `release_record::render_merge_release_pr_report()`                                                                                                                                                           |
| `monochange`        | `src/release_record.rs`                   | Added `render_merge_release_pr_report()` and `text_merge_release_pr_report()`                                                                                                                                                                            |
| `monochange`        | `src/cli_help.rs`                         | Added `merge-release-pr` entry                                                                                                                                                                                                                           |
| `monochange`        | `src/workspace_ops.rs`                    | Generates `slash-command-release.yml` during `mc init --provider github`                                                                                                                                                                                 |
| `monochange`        | `src/templates/slash-command-release.yml` | New workflow template that triggers on PR comments matching `/release`                                                                                                                                                                                   |
| `monochange`        | `src/monochange.init.toml`                | Added `[source.pull_requests.slash_commands.authorization]` example config                                                                                                                                                                               |

## Usage

### 1. Generate the workflow and config

```bash
mc init --provider=github --force
```

This creates `.github/workflows/slash-command-release.yml` and adds `slash_commands.authorization` to `monochange.toml`.

### 2. Configure authorization

Edit `monochange.toml`:

```toml
[source.pull_requests.slash_commands.authorization]
allow_admins = true
allowed_users = ["ifiokjr"]
allowed_teams = ["maintainers"]
```

### 3. Trigger in CI

The GitHub Actions workflow triggers automatically on `/release` comments in PRs. It:

1. Checks out the PR branch
2. Installs `mc`
3. Runs `mc merge-release-pr --pr-number N --author @commenter`

### 4. What `mc merge-release-pr` does

1. Loads workspace configuration (requires `[source]`)
2. Calls the provider to check if the comment author is authorized (admins, allowed users, or team members)
3. Prepares the release from the PR branch working tree
4. Computes the release commit message (same logic as `commit-release`)
5. Squash-merges the PR via the provider API with the computed message as the squash commit title/body
6. Discovers the release record from the merge commit
7. Tags the merge commit with all declared release tags

## Open work / next steps

1. **Add tests**: unit tests for `authorize_slash_command_release` and `merge_release_pull_request` with `httpmock`
2. **GitLab/Gitea implementations**: wire in the new trait methods for GitLab and Gitea providers
3. **Tag push after merge**: the code creates local tags but does not push them; add `push_git_tags` call after tag creation. Also, after the API merge the merge commit SHA lives on the remote base branch, not in the local PR checkout — `discover_release_record` needs a `git fetch origin <base>` before it can see it.
   - **Fixed (v2)**: Added `git fetch origin <base>` after the API merge so the merge commit becomes available locally before tag creation.
4. **Error recovery**: improve error reporting when the PR merge fails due to branch protection or merge conflicts.

## Validation

- `cargo check --all` passes
- `cargo clippy -p monochange -p monochange_core -p monochange_github` passes
- `mc init --provider=github` generates `slash-command-release.yml`
- `mc merge-release-pr --help` renders correctly
- `mc check` on the workspace passes
