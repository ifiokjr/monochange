# `RenderReleaseManifest` (legacy)

`RenderReleaseManifest` is now a legacy compatibility step.

## What changed

`PrepareRelease` now refreshes the cached release manifest automatically at:

- `.monochange/release-manifest.json`

That file is intended to be local monochange metadata, and `.monochange/` should be gitignored.

The written path is exposed to later `Command` steps as:

- `manifest.path`

## Recommended migration

Prefer this:

```toml
[cli.release-manifest]
help_text = "Prepare a release and write a stable JSON manifest"

[[cli.release-manifest.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "json"

[[cli.release-manifest.steps]]
type = "PrepareRelease"
```

Instead of this older pattern:

```toml
[[cli.release-manifest.steps]]
type = "PrepareRelease"

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"
```

## When to keep using it

Only keep `RenderReleaseManifest` when you are migrating an existing workflow that still wants to rewrite the manifest to a non-default path.

For new workflows, treat the manifest as a cached `PrepareRelease` artifact rather than a separate stage.
