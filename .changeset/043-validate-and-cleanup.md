---
monochange: patch
monochange_config: patch
monochange_core: patch
---

Remove `mc commit-release` from default CLI commands. Users who need this workflow can define it in their `monochange.toml` using `PrepareRelease` and `CommitRelease` steps.

Add content-level validation to `mc validate` for versioned files: checks that referenced files exist on disk, ecosystem-typed files contain a readable version field, regex patterns match actual file content, and warns when glob patterns match no files.
