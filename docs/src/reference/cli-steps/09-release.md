# Release

The `Release` step prepares and executes a release from accumulated changesets.

## Purpose

`Release` combines `PrepareRelease` and optionally `CommitRelease` into a single user-facing command. It is the primary command for preparing release files locally when you are ready to apply version bumps, changelog updates, and versioned file changes.

## When to use

Use `Release` when:

- You want to prepare release files locally after reviewing `--dry-run` output
- You are ready to apply version and changelog changes to the workspace
- You want to optionally create a release commit in one step

## Steps it combines

By default, `Release` performs:

1. **PrepareRelease** — Compute the release plan, update versions, changelogs, and versioned files
2. **CommitRelease** (optional) — Create a git commit with the release changes

## CLI usage

```bash
# Preview the release without applying changes
mc release --dry-run

# Preview with unified diffs
mc release --dry-run --diff

# Apply the release
mc release

# Apply and create a commit
mc release --commit
```

## Configuration

The `[cli.release]` command is pre-configured with sensible defaults:

```toml
[cli.release]
help_text = "Prepare a release from discovered change files"

[[cli.release.inputs]]
name = "format"
type = "choice"
choices = ["text", "json", "markdown"]
default = "markdown"

[[cli.release.inputs]]
name = "dry-run"
type = "boolean"

[[cli.release.inputs]]
name = "diff"
type = "boolean"

[[cli.release.steps]]
name = "prepare release"
type = "PrepareRelease"
```

## Output formats

- **text** — Plain text output with sections for changes, groups, and warnings
- **json** — Machine-readable JSON with complete release manifest
- **markdown** — Terminal-friendly markdown (default for human consumption)

## Progress output

Release shows progress through these phases:

1. Reading changesets
2. Computing dependency graph
3. Determining bump severities
4. Applying version updates
5. Rendering changelogs
6. Updating versioned files
7. Writing release manifest
8. Creating commit (if enabled)

Use `--progress-format json` for machine-readable progress.

## Safety

Always run with `--dry-run` first to preview changes before applying them. The `--diff` flag shows unified diffs for all file changes without mutating the workspace.

## Related commands

- [`PrepareRelease`](07-prepare-release.md) — Just the planning phase
- [`CommitRelease`](08-commit-release.md) — Just the commit creation
- [`release-pr`](11-open-release-request.md) — Open a release PR instead of committing locally
- [`publish-release`](10-publish-release.md) — Publish provider releases after local commit
