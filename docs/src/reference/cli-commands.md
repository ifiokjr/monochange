# CLI command surface

monochange exposes three command classes. Keeping them separate avoids stale automation and incorrect docs.

## Built-in binary commands

These are wired directly by the `mc` / `monochange` binary and do not require `monochange.toml` workflow entries:

- `mc init` — generate a starter `monochange.toml`.
- `mc populate` — add missing configurable workflow command definitions to an existing config.
- `mc command` — interactively add or edit `[cli.*]` workflow commands.
- `mc skill` — install the monochange skill bundle through the upstream `skills add` workflow.
- `mc subagents` — generate repo-local agent guidance and optional MCP config.
- `mc analyze` — inspect semantic changes for one package.
- `mc migrate audit` and `mc migrate release-records` — inspect or migrate release metadata.
- `mc check` — validate config, changesets, and manifest lint rules.
- `mc lint list`, `mc lint explain`, and `mc lint new` — inspect or scaffold lint rules.
- `mc mcp` — run the stdio MCP server.
- `mc help` — show help for built-in, step, and configured workflow commands.

## Built-in step commands

Every built-in `CliStepDefinition` except generic `Command` is also exposed as `mc step:<kebab-name>`, for example:

```sh
mc step:config --format json
mc step:discover --format json
mc step:prepare-release --dry-run --format json
mc step:publish-readiness --from HEAD --output .monochange/local/readiness.json
```

Step commands are the portable fallback for docs and scripts because they exist even when a repository has no `[cli.*]` workflows.

## Configured workflow commands

Every `[cli.<name>]` table in `monochange.toml` becomes a top-level `mc <name>` command in that repository. Common names such as `mc change`, `mc release`, `mc release-pr`, `mc publish-plan`, or `mc publish` are configuration, not CLI guarantees.

Use this wording in generic docs:

- Prefer `mc step:<name>` when documenting guaranteed behavior.
- Say “if your repository defines `[cli.release]`, run `mc release`” when using a friendly workflow alias.
- Run `mc help` or `mc step:config --format json` before assuming a workflow command exists.

`mc init` writes a minimal starter config and does not seed default workflow aliases. Use `mc populate` or `mc command` when you want to add named workflows.

## `mc` and `monochange`

`mc` and `monochange` are aliases over the same runtime and command implementation. Prefer `mc` in examples for brevity, but use `monochange` where a full binary name is clearer.
