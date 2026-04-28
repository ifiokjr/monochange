---
monochange: patch
monochange_config: test
monochange_core: major
monochange_gitea: minor
monochange_github: minor
monochange_gitlab: minor
monochange_hosting: minor
---

#### Add no-verify support to release automation

> **Breaking change** — library consumers that construct `monochange_core::CliStepDefinition::CommitRelease` or `OpenReleaseRequest`, or that call the exported git/provider release helpers directly, must now handle the new `no_verify` field/argument.

Release automation can now bypass local git hooks when creating the generated release commit and when pushing the release request branch. This is useful for CI-driven `mc release-pr` flows where repository hooks depend on tools that are not available in the runner environment.

**Before (`monochange.toml`):**

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "markdown"], default = "markdown" },
]
steps = [
	{ type = "CommitRelease", name = "create release commit" },
	{ type = "OpenReleaseRequest", name = "create the pr" },
]
```

**After:**

```toml
[cli.release-pr]
inputs = [
	{ name = "format", type = "choice", choices = ["text", "json", "markdown"], default = "markdown" },
	{ name = "no_verify", type = "boolean", default = true },
]
steps = [
	{ type = "CommitRelease", name = "create release commit", inputs = { no_verify = "{{ inputs.no_verify }}" } },
	{ type = "OpenReleaseRequest", name = "create the pr", inputs = { no_verify = "{{ inputs.no_verify }}" } },
]
```

That keeps the `mc release-pr` invocation the same while making hook bypass explicit in config:

```bash
mc release-pr
```

For crate consumers, the step and git helper APIs now carry the same flag through the full release-request pipeline.

**Before (`monochange_core` / hosting adapters):**

```rust
// before
CliStepDefinition::CommitRelease { name, when, inputs }
CliStepDefinition::OpenReleaseRequest { name, when, inputs }

git_commit_paths_command(root, &message)
git_push_branch_command(root, branch)
```

**After:**

```rust
// after
CliStepDefinition::CommitRelease { name, when, no_verify, inputs }
CliStepDefinition::OpenReleaseRequest { name, when, no_verify, inputs }

git_commit_paths_command(root, &message, no_verify)
git_push_branch_command(root, branch, no_verify)
```

Provider-facing release helpers in `monochange_hosting`, `monochange_github`, `monochange_gitlab`, and `monochange_gitea` now forward that flag so a single `no_verify` choice applies consistently to commit creation and branch push operations.
