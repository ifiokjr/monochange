---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_schema: patch
---

# Add `always_run` primitive to CLI steps and group/ecosystem filters to `PublishPackages`

## `always_run` primitive

A new `always_run` boolean field is available on every CLI step definition. When `always_run: true`, the step continues to execute even when a previous step in the same command has failed.

This enables composable dry-run workflows such as:

```toml
[[cli.publish-dry-run]]
name = "publish-dry-run"
help_text = "Preview publishing without side effects"
steps = [
	{ type = "PrepareRelease", name = "prepare", inputs = { allow_empty_changesets = "true" } },
	{ type = "PublishPackages", name = "publish", always_run = true, inputs = { resume = ".monochange/local/previous-result.json" } },
]
```

Running `mc publish-dry-run --dry-run` will always execute the `PublishPackages` step regardless of whether `PrepareRelease` succeeds, because `PublishPackages` is marked `always_run = true`.

### Behavior

- When a step fails and later steps have `always_run: true`, those steps still execute.
- Non-`always_run` steps after a failure are skipped.
- The overall command still returns the first error after all `always_run` steps finish.

## `PublishPackages` filters

`PublishPackages` now accepts two new step inputs:

- `--group <group-id>` — resolves a group from the workspace configuration and publishes all packages in that group.
- `--ecosystem <ecosystem>` — filters publication targets to a specific ecosystem (`cargo`, `npm`, `deno`, `dart`, `flutter`, `python`, or `go`).

Both inputs can be repeated:

```bash
mc publish --group sdk --group apps --ecosystem npm --ecosystem cargo
```

Groups are resolved to their member packages before ecosystem filtering is applied.

## Dry-run guards

`PublishPackages` now skips the following side-effecting operations when `--dry-run` is active:

- `release_branch_policy::verify_release_ref_for_publish`
- `publish_rate_limits::enforce_publish_rate_limits`
- writing the publish report artifact to disk

## Per-command `dry_run` field

CLI command definitions now support a `dry_run` boolean field. When `dry_run = true`, the command always executes in dry-run mode regardless of whether `--dry-run` is passed on the CLI. This enables built-in preview commands such as:

```toml
[cli.publish-check]
help_text = "Validate the release and preview package publishing in dry-run mode"
dry_run = true
steps = [
	{ type = "PrepareRelease", name = "plan release" },
	{ type = "PublishPackages", name = "publish packages dry run" },
]
```

Running `mc publish-check` (without `--dry-run`) will still run in dry-run mode because the command definition sets `dry_run = true`.
