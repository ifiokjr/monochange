# `RenderReleaseManifest`

## What it does

`RenderReleaseManifest` turns the prepared release into a stable JSON manifest and can write it to disk.

This is the step to use when another job, script, AI workflow, or deployment stage needs a machine-readable description of the release that monochange just prepared.

## Why use it

Use `RenderReleaseManifest` when you need an automation artifact rather than just human-readable terminal output.

It is the best fit for:

- CI workflows that upload a manifest artifact
- follow-up jobs that should consume exact release metadata
- automation that should not re-run planning heuristics itself
- external tools that want a stable release payload contract

## Inputs and fields

Built-in inputs:

- none

Step-specific config fields:

- `path` — optional output path for the manifest file

## Prerequisites

- a previous `PrepareRelease` step in the same command

## Side effects and outputs

- builds the structured release manifest
- optionally writes it to the configured `path`
- exposes `manifest.path` to later `Command` steps when a path was written

## Example

<!-- {=cliStepRenderReleaseManifestExample} -->

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

[[cli.release-manifest.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"
```

<!-- {/cliStepRenderReleaseManifestExample} -->

## Composition ideas

### Prepare, render, then upload

```toml
[cli.release-manifest-upload]
help_text = "Prepare a release, write a manifest, and upload it"

[[cli.release-manifest-upload.steps]]
type = "PrepareRelease"

[[cli.release-manifest-upload.steps]]
type = "RenderReleaseManifest"
path = ".monochange/release-manifest.json"

[[cli.release-manifest-upload.steps]]
type = "Command"
command = "echo uploading {{ manifest.path }}"
shell = true
```

### Use it as the contract between jobs

A common design is to keep one job responsible for running `PrepareRelease` and `RenderReleaseManifest`, then let downstream jobs consume the emitted JSON rather than invoking several release calculations independently.

## Why choose it over `PrepareRelease --format json` alone?

`PrepareRelease --format json` is useful for immediate command output.

`RenderReleaseManifest` is better when you want:

- a stable on-disk artifact
- a file path that later steps can reference
- a clear, typed workflow stage inside `[cli.<command>.steps]`

## Common mistake

Do not place `RenderReleaseManifest` before `PrepareRelease`. It has no independent release state to serialize.
