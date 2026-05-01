# Commands skill

Use this guide when the task is to choose, explain, or sequence monochange CLI commands.

## Command selection by goal

### Create or repair config

| Goal                        | Command       | When to use it                                                  |
| --------------------------- | ------------- | --------------------------------------------------------------- |
| Bootstrap a repo            | `mc init`     | You need a starter `monochange.toml` based on detected packages |
| Check config and changesets | `mc validate` | Before and after release-affecting edits                        |

Examples:

```bash
mc init
mc init --provider github
mc validate
```

### Inspect the workspace

| Goal                                      | Command                          | When to use it                                                     |
| ----------------------------------------- | -------------------------------- | ------------------------------------------------------------------ |
| See normalized packages and groups        | `mc discover --format json`      | You need package ids, dependency edges, or group ownership         |
| Audit pending changesets with git context | `mc diagnostics --format json`   | You need introduced commits, linked reviews, or related issues     |
| Inspect a durable release declaration     | `mc release-record --from <ref>` | You need to inspect a past release after the commit already exists |

Examples:

```bash
mc discover --format json
mc diagnostics --format json
mc diagnostics --changeset .changeset/feature.md
mc release-record --from v1.2.3
mc release-record --from HEAD --format json
```

### Inspect lint rules and presets

| Goal                            | Command                | When to use it                                                          |
| ------------------------------- | ---------------------- | ----------------------------------------------------------------------- |
| List registered rules/presets   | `mc lint list`         | You want to see which lint ids and presets monochange currently exposes |
| Explain one rule or preset      | `mc lint explain <id>` | You want the details before configuring a rule in `[lints]`             |
| Run configured lint enforcement | `mc check`             | You want validation plus lint execution against manifests               |

Examples:

```bash
mc lint list
mc lint list --format json
mc lint explain cargo/recommended
mc lint explain npm/workspace-protocol
mc check --fix
```

### Create release intent

| Goal                                     | Command                     | When to use it                                                                               |
| ---------------------------------------- | --------------------------- | -------------------------------------------------------------------------------------------- |
| Create a changeset                       | `mc change`                 | You know the target package or group id                                                      |
| Check policy coverage from changed files | `mc step:affected-packages` | CI or review needs to confirm that changed packages have changesets without a config wrapper |

Examples:

```bash
mc change --package monochange --bump minor --reason "add diagnostics command"
mc change --package monochange_config --bump none --caused-by monochange_core --reason "dependency-only follow-up"
mc step:affected-packages --verify --changed-paths crates/monochange/src/lib.rs --format json
```

### Plan or execute a release

| Goal                            | Command                            | When to use it                                                                   |
| ------------------------------- | ---------------------------------- | -------------------------------------------------------------------------------- |
| Preview a release safely        | `mc release --dry-run`             | You want the computed plan without mutating files                                |
| Preview file diffs too          | `mc release --dry-run --diff`      | You want to see version/changelog patches before applying them                   |
| Apply the release locally       | `mc release`                       | You are ready to update files on disk                                            |
| Create a release commit locally | `mc commit-release`                | You want the prepared commit before provider publishing                          |
| Publish package artifacts       | `mc publish --output <path>`       | Built-in package publishing is configured for the release state                  |
| Create provider releases        | `mc publish-release`               | Source/provider publishing is configured                                         |
| Open or update a release PR     | `mc release-pr`                    | You want provider-hosted release-request automation                              |
| Bootstrap release packages      | `mc publish-bootstrap --from HEAD` | A release package must exist in the public registry before automation can finish |
| Retarget a recent release       | `mc repair-release --from <ref>`   | A just-created release needs to move forward to a later commit                   |

Examples:

```bash
mc release --dry-run
mc release --dry-run --diff
mc release --dry-run --format json
mc commit-release --dry-run --diff
mc publish --dry-run --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish-plan --readiness .monochange/readiness.json --format json
mc publish --output .monochange/publish-result.json
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc placeholder-publish --dry-run --format json
mc repair-release --from v1.2.3 --target HEAD --dry-run
```

### Assistant workflows

| Goal                                  | Command                       | When to use it                                               |
| ------------------------------------- | ----------------------------- | ------------------------------------------------------------ |
| Install the monochange skill locally  | `mc skill [skills-add flags]` | You want the project-local skill bundle through `skills add` |
| Generate repo-local agent setup files | `mc subagents <target>`       | You want monochange-aware agents, subagents, or rules        |
| Start the MCP server                  | `mc mcp`                      | The client launches monochange over stdio                    |

Examples:

```bash
mc help skill
mc skill
mc skill -a pi -y
mc help subagents
mc subagents claude
mc subagents pi codex
mc mcp
```

## Output formats

Many commands support `text`, `markdown`, and `json`.

Use:

- `--format json` for automation and agent parsing
- `--format markdown` for human-readable terminal output with richer structure
- `--format text` when you explicitly want the older plain-text rendering

For release-oriented commands, markdown is the default output format.

## A practical command flow

For most work, use this order:

```bash
mc validate
mc discover --format json
mc change --package <id> --bump patch --reason "describe the change"
mc diagnostics --format json
mc release --dry-run --diff
```

Then choose the next step:

- `mc release` to apply
- `mc commit-release` to produce the local release commit
- `mc publish-readiness --from HEAD --output <path>`, optional `mc publish-bootstrap --from HEAD --output <path>`, `mc publish-plan --readiness <path>`, and `mc publish --output <path>` to preflight/plan/bootstrap/publish package artifacts; rerun with `--resume <path>` after a partial publish failure
- `mc publish-release` to create provider releases
- `mc release-pr` to update a release request instead

## Common mistakes

### Guessing package ids

**Avoid:**

```bash
mc change --package crates/monochange --bump patch --reason "..."
```

**Prefer:**

```bash
mc discover --format json
mc change --package monochange --bump patch --reason "..."
```

### Releasing before a dry run

**Avoid:**

```bash
mc release
```

**Prefer:**

```bash
mc release --dry-run --diff
mc release
```

### Reading raw changesets when diagnostics would be clearer

**Avoid:** manually scraping `.changeset/*.md` files to discover provenance.

**Prefer:**

```bash
mc diagnostics --format json
```

## Related references

- [reference.md](./reference.md)
- [configuration.md](./configuration.md)
- [changesets.md](./changesets.md)
