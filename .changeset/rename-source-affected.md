---
monochange: major
monochange_config: major
monochange_core: major
monochange_github: major
monochange_hosting: major
"@monochange/skill": major
---

# Consolidate affected-package configuration

> **Breaking change** — affected-package policy now lives in `[changesets.affected]`. Configurations using the previous `[changesets.verify]` section must rename it, and configurations using the hosted-source affected-package policy section (`[source.bot.changesets]` in older configs, or `[source.affected]` in prerelease configs) must move `enabled`, `required`, `skip_labels`, `comment_on_failure`, `changed_paths`, and `ignored_paths` into `[changesets.affected]`.

Move affected-package policy settings into the changesets configuration:

```toml
[changesets.affected]
enabled = true
required = true
skip_labels = ["no-changeset-required"]
comment_on_failure = true
changed_paths = ["Cargo.toml", "Cargo.lock"]
ignored_paths = ["**/tests/**"]
```

The Rust configuration model now exposes `ChangesetSettings::affected` with `ChangesetAffectedSettings`; the previous `ChangesetSettings::verify`, `SourceConfiguration::bot`, `SourceConfiguration::affected`, `ProviderChangesetBotSettings`, `ProviderAffectedSettings`, and `ProviderBotSettings` types or fields have been removed.

The `mc affected` policy command now reports `skipped` when it runs on a generated release pull request branch whose name starts with `source.pull_requests.branch_prefix`, allowing CI to ignore those branches.
