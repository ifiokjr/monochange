# Config

`Config` exposes the resolved monochange configuration and workspace root without doing package discovery or release planning.

Use it when you need a cheap, structured view of the active configuration before choosing a workflow command.

```sh
mc step:config --format json
mc step:config --jq '.workspaceRoot'
```

## Inputs

This step accepts the common output flags such as `--format` and `--jq`.

## Output

The JSON output includes the workspace root and resolved configuration after defaults have been applied. It is useful for agents and CI scripts that need to inspect package ids, groups, source-provider settings, or configured `[cli.*]` workflow commands before running a mutating step.

## Composition notes

`Config` is standalone. It does not create release state for later steps, but it is a good first step in diagnostic workflows that then branch into [`Validate`](01-validate.md), [`Discover`](02-discover.md), or custom `Command` steps.
