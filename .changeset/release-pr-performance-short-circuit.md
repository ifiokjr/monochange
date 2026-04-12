---
"monochange": patch
"monochange_core": patch
"monochange_gitea": patch
"monochange_github": patch
"monochange_gitlab": patch
---

#### skip redundant hosted release request updates

Release request creation now avoids a second round of hosted work when the open pull request or merge request already matches the prepared release branch state. This makes reruns cheaper for GitHub, GitLab, and Gitea adapter callers, and it adds a real `Skipped` outcome so callers can tell the difference between "updated" and "already aligned".

Before:

```sh
mc release-pr --format json
# always prepared a branch push and provider update path
```

After:

```sh
mc release-pr --format json
# still prepares the release request, but hosted adapters now skip redundant
# push and metadata updates when the existing request already matches HEAD
```

If you call the provider crates directly, `SourceChangeRequestOperation` now exposes the no-op state explicitly:

```rust
use monochange_core::SourceChangeRequestOperation;

let operation = SourceChangeRequestOperation::Skipped;
assert_eq!(operation.to_string(), "skipped");
```

The benchmark suite now also includes a real hosted `release-pr` creation path alongside the existing dry-run preview benchmark:

```sh
cargo bench -p monochange --bench cli_commands release_pr -- --noplot
```
