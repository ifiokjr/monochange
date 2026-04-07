# `AffectedPackages`

## What it does

`AffectedPackages` evaluates changed files into affected package coverage and changeset policy results.

It can answer questions such as:

- which packages are affected by this change set?
- are those changes covered by changesets?
- should verification be skipped because of labels?

## Why use it

Use `AffectedPackages` when you want a CI-oriented policy step instead of a release step.

It is the best fit for:

- pull request checks
- pre-merge policy enforcement
- reusable GitHub Actions or other CI jobs
- custom failure messaging based on affected-package status

## Inputs

- `format` — `text` or `json`
- `changed_paths` — explicit changed paths
- `since` — revision to diff against; takes priority over `changed_paths`
- `verify` — whether to enforce non-zero failure on uncovered packages
- `label` — skip labels supplied from CI

## Prerequisites

None. `AffectedPackages` is standalone.

## Side effects and outputs

- computes the changeset policy evaluation
- exposes `affected.status` and `affected.summary` to later `Command` steps
- can be used as a pure reporting step or an enforcing gate depending on `verify`

## Example

<!-- {=cliStepAffectedPackagesExample} -->

```toml
[cli.affected]
help_text = "Evaluate pull-request changeset policy"

[[cli.affected.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]
default = "text"

[[cli.affected.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected.inputs]]
name = "label"
type = "string_list"

[[cli.affected.inputs]]
name = "verify"
type = "boolean"

[[cli.affected.steps]]
type = "AffectedPackages"
```

<!-- {/cliStepAffectedPackagesExample} -->

## Composition ideas

### Evaluate and then print a custom summary

```toml
[cli.affected-report]
help_text = "Evaluate affected packages and print a custom summary"

[[cli.affected-report.inputs]]
name = "changed_paths"
type = "string_list"
required = true

[[cli.affected-report.steps]]
type = "AffectedPackages"

[[cli.affected-report.steps]]
type = "Command"
command = "echo affected status {{ affected.status }}: {{ affected.summary }}"
shell = true
```

### Use it as a PR-only command

This step is often best kept in a dedicated CI command rather than bundled into normal release preparation. It answers a different question: "is the pull request policy-complete?" not "what should be released?"

## Why choose it over a plain `git diff` script?

Because it reuses MonoChange's own understanding of package paths, groups, ignored paths, additional paths, skip labels, and changeset coverage.

## Common mistakes

- providing both `since` and `changed_paths` and forgetting `since` wins
- assuming this step prepares release state
- treating verification results as equivalent to a release plan
