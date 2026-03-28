# monochange

The `monochange` crate provides the end-user CLI.

## Commands

```bash
mc workspace discover --root . --format json
mc changes add --root . --package crates/monochange --bump patch --reason "describe the change"
mc plan release --root . --changes .changeset/1234567890-crates-monochange.toml --format json
```

## Responsibilities

- aggregate all supported ecosystem adapters
- load `monochange.toml`
- resolve change input files
- render discovery and release-plan output in text or JSON
