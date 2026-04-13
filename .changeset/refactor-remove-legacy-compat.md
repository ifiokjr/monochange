---
"monochange": patch
"monochange_config": patch
"monochange_core": patch
---

monochange now accepts only the current pre-1.0 configuration and CLI schema.

- `monochange.toml` source automation must use `[source]`, `[source.releases]`, `[source.pull_requests]`, and `[source.bot.changesets]`; legacy `[github]` tables are no longer accepted.
- CLI step definitions now accept only the current step names such as `PublishRelease`, `OpenReleaseRequest`, and `AffectedPackages`; legacy aliases like `PublishGitHubRelease`, `OpenReleasePullRequest`, `EnforceChangesetPolicy`, and `VerifyChangesets` were removed.
- `Command` steps now accept only `dry_run_command`; the legacy `dry_run` field alias was removed.
- Command templates now expose CLI inputs only through `{{ inputs.name }}`.
