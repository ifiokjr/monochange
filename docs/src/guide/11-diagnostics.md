# Diagnostics

`mc diagnostics` gives you a quick snapshot of every pending changeset together with its git and review provenance.

It is useful for human developers reviewing a PR, for AI agents auditing what has changed, and for CI steps that need to understand the full context of pending work before triggering a release.

## Basic usage

Inspect all pending changesets:

```bash
mc diagnostics
```

Inspect a specific changeset:

```bash
mc diagnostics --changeset .changeset/feature.md
```

You can pass `--changeset` multiple times. Duplicate paths are deduplicated automatically:

```bash
mc diagnostics \
  --changeset .changeset/api-change.md \
  --changeset .changeset/api-change.md \
  --changeset .changeset/bug-fix.md
```

A short name without the directory prefix also works:

```bash
mc diagnostics --changeset feature.md
```

And absolute paths resolve correctly too:

```bash
mc diagnostics --changeset /home/user/project/.changeset/feature.md
```

## JSON output

Machine-readable diagnostics for scripting, CI, or AI consumption:

```bash
mc diagnostics --format json
```

The JSON envelope includes:

- `requestedChangesets` — the resolved paths that were queried
- `changesets` — full `PreparedChangeset` records, each with:
  - `path` — workspace-relative path to the changeset file
  - `summary` — the first paragraph of the markdown body
  - `details` — optional follow-up paragraphs
  - `targets` — package/group bump entries, each with `kind`, `id`, `bump`, `origin`, and optional `evidenceRefs`
  - `context` — provenance metadata (see below)

## Provenance context

When a changeset has been committed to a git repository, each `context` record contains:

| Field | Description |
|---|---|
| `introduced` | revision where the changeset file was first committed |
| `lastUpdated` | revision where it was most recently changed (omitted when same as `introduced`) |
| `relatedIssues` | issues linked by the changeset or the PR that introduced it |

Each revision record includes:

| Field | Description |
|---|---|
| `commit.sha` | full commit SHA |
| `commit.shortSha` | short SHA for display |
| `reviewRequest` | PR/MR number and URL when the commit is associated with a pull request |

## Config entry

Add it manually or run `mc init` to get it included automatically:

```toml
[cli.diagnostics]
help_text = "Show changeset diagnostics and provenance"

[[cli.diagnostics.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.diagnostics.inputs]]
name = "changeset"
type = "string_list"
help_text = "changeset path(s), e.g. .changeset/feature.md (omit for all changesets)"

[[cli.diagnostics.steps]]
type = "DiagnoseChangesets"
```

## AI agent and MCP usage

`mc diagnostics --format json` is designed to be called by AI agents to get a structured overview of all pending changes before planning a release, reviewing a PR, or proposing follow-up work.

A typical agent workflow looks like this:

1. `mc discover --format json` — understand the workspace package graph
2. `mc diagnostics --format json` — see all pending changesets, linked PRs, and introduced commits
3. `mc release --dry-run --format json` — preview the computed release plan
4. `mc change ...` — add, update, or remove changesets as needed
5. `mc release` — execute the release when everything looks correct

Because `mc diagnostics` returns stable, workspace-relative paths and structured JSON, agents can parse the output without needing to read raw markdown files directly. Each changeset record includes enough context — who introduced it, which PR it belongs to, which issues it closes — for an agent to make targeted decisions about whether to proceed with a release or request changes.

### Example: check for undocumented packages before a release

```bash
mc diagnostics --format json | jq '[.changesets[] | select(.targets | length == 0)]'
```

### Example: list all open review requests linked to pending changesets

```bash
mc diagnostics --format json \
  | jq '[.changesets[].context?.introduced?.reviewRequest? | select(. != null) | .id] | unique'
```
