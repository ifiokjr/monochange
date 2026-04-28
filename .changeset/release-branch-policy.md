---
"monochange": minor
"monochange_core": minor
"monochange_config": minor
"monochange_gitea": patch
"monochange_github": patch
"monochange_gitlab": patch
"monochange_hosting": patch
"@monochange/cli": patch
---

#### Add release branch policy enforcement

monochange can now enforce that release tags and registry publishing only run from commits that are reachable from configured release branches. This moves release-branch safety from ad hoc CI shell checks into reusable CLI behavior that works in detached CI checkouts.

**Before (`monochange.toml`):**

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"
```

Release workflows had to add their own branch guard scripts before running commands such as `mc tag-release` or `mc publish`.

**After (`monochange.toml`):**

```toml
[source]
provider = "github"
owner = "acme"
repo = "widgets"

[source.releases]
branches = ["main", "release/*"]
enforce_for_tags = true
enforce_for_publish = true
enforce_for_commit = false
```

`mc tag-release`, `PublishRelease`, and `PublishPackages` now reject release refs that are not reachable from one of the configured release branch patterns. `CommitRelease` remains usable on release-preparation branches by default, but projects can opt into the same guard with `enforce_for_commit = true`.

**Before (manual CI guard):**

```bash
git fetch origin main
# custom shell checks here
mc tag-release --from v1.2.0 --push
```

**After (CLI-native guard):**

```bash
mc step:verify-release-branch --from v1.2.0
mc tag-release --from v1.2.0 --push
```

The explicit `step:verify-release-branch` command is available for pipelines that want an early, named verification step, while mutation commands still enforce the policy internally when the relevant `enforce_for_*` setting is enabled.

**Before (`monochange_core::ProviderReleaseSettings`):**

```rust
ProviderReleaseSettings {
    enabled,
    draft,
    prerelease,
    generate_notes,
    source,
}
```

**After:**

```rust
ProviderReleaseSettings {
    enabled,
    draft,
    prerelease,
    generate_notes,
    source,
    branches,
    enforce_for_tags,
    enforce_for_publish,
    enforce_for_commit,
}
```

Callers constructing `ProviderReleaseSettings` directly should include the release branch policy fields or use `ProviderReleaseSettings::default()` to keep the default `main` branch policy.
