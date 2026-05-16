# Diagnostics

`mc step:diagnose-changesets` gives you a quick snapshot of every pending changeset together with its git and review context.

It is useful for human developers reviewing a PR, for AI agents auditing what has changed, and for CI steps that need to understand the full context of pending work before triggering a release.

## Basic usage

Inspect all pending changesets:

```bash
mc step:diagnose-changesets
```

Inspect a specific changeset:

```bash
mc step:diagnose-changesets --changeset .changeset/feature.md
```

You can pass `--changeset` multiple times. Duplicate paths are deduplicated automatically:

```bash
mc step:diagnose-changesets \
  --changeset .changeset/api-change.md \
  --changeset .changeset/api-change.md \
  --changeset .changeset/bug-fix.md
```

A short name without the directory prefix also works:

```bash
mc step:diagnose-changesets --changeset feature.md
```

And absolute paths resolve correctly too:

```bash
mc step:diagnose-changesets --changeset /home/user/project/.changeset/feature.md
```

## JSON output

Machine-readable diagnostics for scripting, CI, or AI consumption:

```bash
mc step:diagnose-changesets --format json
```

The JSON envelope includes:

- `requestedChangesets` — the resolved paths that were queried
- `changesets` — full `PreparedChangeset` records, each with:
  - `path` — workspace-relative path to the changeset file
  - `summary` — the first paragraph of the markdown body
  - `details` — optional follow-up paragraphs
  - `targets` — package/group bump entries, each with `kind`, `id`, `bump`, `origin`, and optional `evidenceRefs`
  - `context` — git and review context (see below)

## Context fields

When a changeset has been committed to a git repository, each `context` record contains:

- `introduced` — revision where the changeset file was first committed
- `lastUpdated` — revision where it was most recently changed (omitted when same as `introduced`)
- `relatedIssues` — issues linked by the changeset or the PR that introduced it

Each revision record includes:

- `commit.sha` — full commit SHA
- `commit.shortSha` — short SHA for display
- `reviewRequest` — PR/MR number and URL when the commit is associated with a pull request

## Command

Use the generated immutable step command directly:

```bash
mc step:diagnose-changesets --format json
```

You only need a `[cli.*]` entry if you want a repository-specific alias that wraps diagnostics with additional steps or inputs.

## AI agent and MCP usage

`mc step:diagnose-changesets --format json` and the MCP tool `monochange_diagnostics` are designed to give AI agents a structured overview of all pending changes before planning a release, reviewing a PR, or proposing follow-up work.

A typical agent workflow looks like this:

1. `mc step:discover --format json` — understand the workspace package graph
2. `mc step:diagnose-changesets --format json` — see all pending changesets, linked PRs, and introduced commits
3. `mc step:prepare-release --dry-run --format json` — preview the computed release plan
4. `mc step:create-change-file ...` — add, update, or remove changesets as needed
5. `mc step:prepare-release` — execute the release when everything looks correct

Because `mc step:diagnose-changesets` and `monochange_diagnostics` return stable, workspace-relative paths and structured JSON, agents can parse the output without needing to read raw markdown files directly. Each changeset record includes enough context — who introduced it, which PR it belongs to, which issues it closes — for an agent to make targeted decisions about whether to proceed with a release or request changes.

### Example: check for undocumented packages before a release

```bash
mc step:diagnose-changesets --format json | jq '[.changesets[] | select(.targets | length == 0)]'
```

### Example: list all open review requests linked to pending changesets

```bash
mc step:diagnose-changesets --format json \
  | jq '[.changesets[].context?.introduced?.reviewRequest? | select(. != null) | .id] | unique'
```
