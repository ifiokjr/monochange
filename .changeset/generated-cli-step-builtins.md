---
"monochange": major
"monochange_core": major
"monochange_schema": major
"@monochange/cli": major
"@monochange/skill": major
---

# generate built-in release and validation step commands

> **Breaking change** — several hardcoded top-level commands now live under generated immutable `mc step:*` command names.

The release-record, publish-readiness, tag-release, placeholder-publish, and validation operations now share the generated step-command path used by the rest of the CLI step catalog. This keeps their help, schema metadata, docs, and automation examples consistent with configured workflow steps while preserving the distinction between binary commands, generated step commands, and optional user-defined `[cli.*]` workflow aliases.

**Before:** scripts could call these hardcoded top-level commands directly:

```bash
mc validate
mc release-record --from HEAD --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc tag-release --from HEAD
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
```

**After:** call the generated step command names instead:

```bash
mc step:validate
mc step:release-record --from HEAD --format json
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:tag-release --from HEAD
mc step:placeholder-publish --from HEAD --output .monochange/bootstrap-result.json
```

`mc init` also writes a smaller starter configuration. It no longer seeds redundant generated `[cli.*]` aliases for commands that already exist as immutable step commands.

**Before:** starter configs included workflow aliases for generated behavior:

```toml
[cli.validate]
steps = [{ type = "Validate" }]
```

**After:** starter configs rely on the generated command directly and reserve `[cli.*]` for repository-specific chains, custom inputs, or shell `Command` steps:

```bash
mc step:validate
```
