# Template Variables

monochange uses template variables throughout its configuration system for dynamic values that are resolved at runtime.

## Overview

Templates use the Mustache/`{{ }}` syntax. Variables are available in:

- Changelog paths and formats
- Release titles and changelog version titles
- Command step shell commands
- Changeset templates

## Release Title Templates

Configure in `[defaults]`, `[package.<id>]`, or `[group.<id>]`:

```toml
[defaults]
release_title = "{{ version }} ({{ date }})"
changelog_version_title = "[{{ version }}]({{ tag_url }}) ({{ date }})"
```

### Available Variables

| Variable              | Description                                   | Example                                                 |
| --------------------- | --------------------------------------------- | ------------------------------------------------------- |
| `{{ version }}`       | The new version being released                | `1.2.3`                                                 |
| `{{ id }}`            | Package or group ID                           | `my-package`                                            |
| `{{ date }}`          | Current date (YYYY-MM-DD)                     | `2026-04-13`                                            |
| `{{ time }}`          | Current time (HH:MM:SS)                       | `14:30:00`                                              |
| `{{ datetime }}`      | Full datetime (ISO 8601)                      | `2026-04-13T14:30:00Z`                                  |
| `{{ changes_count }}` | Number of changes in release                  | `5`                                                     |
| `{{ tag_url }}`       | URL to git tag (if source configured)         | `https://github.com/owner/repo/releases/tag/v1.2.3`     |
| `{{ compare_url }}`   | URL comparing versions (if source configured) | `https://github.com/owner/repo/compare/v1.2.2...v1.2.3` |

## Changeset Templates

Configure in `[release_notes]`:

```toml
[release_notes]
change_templates = [
	"#### {{ summary }}\n\n{{ details }}\n\n{{ context }}",
	"- {{ summary }}",
]
```

### Available Variables

| Variable               | Description                                    |
| ---------------------- | ---------------------------------------------- |
| `{{ summary }}`        | The changeset summary/reason                   |
| `{{ details }}`        | Detailed change description (from `--details`) |
| `{{ context }}`        | Full changeset context with metadata           |
| `{{ type }}`           | Change type (if specified)                     |
| `{{ package }}`        | Target package ID                              |
| `{{ bump }}`           | Bump severity (patch/minor/major)              |
| `{{ changeset_path }}` | Path to the changeset file                     |

### Context Metadata (with source provider)

When a source provider (GitHub/GitLab/Gitea) is configured:

| Variable                       | Description                           |
| ------------------------------ | ------------------------------------- |
| `{{ change_owner_link }}`      | Markdown link to the changeset author |
| `{{ review_request_link }}`    | Markdown link to the PR/MR            |
| `{{ introduced_commit_link }}` | Markdown link to the first commit     |
| `{{ closed_issue_links }}`     | Markdown links to closed issues       |
| `{{ related_issue_links }}`    | Markdown links to related issues      |

## Command Step Templates

In `[cli.<command>.steps]` of type `Command`:

```toml
[[cli.release.steps]]
type = "Command"
name = "notify slack"
command = 'curl -X POST -d "{""text"":""Released {{ version }}""}" {{ slack_webhook }}'
```

### Available Variables

Command steps have access to:

**Release context:**

- `{{ version }}` — New version
- `{{ id }}` — Package or group ID
- `{{ manifest.path }}` — Path to manifest file
- `{{ manifest.version }}` — Previous version

**Affected packages (for affected command):**

- `{{ affected.ids }}` — List of affected package IDs
- `{{ affected.count }}` — Number of affected packages

**Step outputs:**

- `{{ steps.<step_name>.output }}` — Output from a previous step
- `{{ inputs.<input_name> }}` — CLI input values

**Release commit context:**

- `{{ release_commit.sha }}` — Commit SHA
- `{{ release_commit.message }}` — Commit message
- `{{ release_commit.author }}` — Commit author

## Changelog Path Templates

For changelog paths, you can use:

```toml
[defaults.changelog]
path = "{{ path }}/changelog.md"
```

| Variable     | Description                                  |
| ------------ | -------------------------------------------- |
| `{{ path }}` | Package path (directory containing manifest) |
| `{{ id }}`   | Package ID                                   |

## Versioned File Templates

For custom versioned file definitions:

```toml
versioned_files = [
	{ path = "README.md", regex = 'v(?<version>\d+\.\d+\.\d+)' },
]
```

The regex must include a named capture group `version` that will be replaced with the new version.

## Best Practices

1. **Use `{{ context }}` for complete metadata** — It includes all linked information without exposing transient file paths
2. **Prefer markdown links** — Use `{{ tag_url }}` and `{{ compare_url }}` for clickable references
3. **Keep templates simple** — Complex templates are harder to maintain
4. **Test with `--dry-run`** — Preview rendered output before committing
5. **Escape special characters** — In TOML, quote templates containing newlines or special characters

## Examples

### Simple release title

```toml
[defaults]
release_title = "v{{ version }}"
```

### Detailed changelog entry

```toml
[release_notes]
change_templates = [
	"### {{ summary }}\n\n{{ details }}\n\n_Released in {{ version }} on {{ date }}_",
]
```

### GitHub-linked title

```toml
[defaults]
changelog_version_title = "## [{{ version }}]({{ compare_url }}) ({{ date }})"
```
