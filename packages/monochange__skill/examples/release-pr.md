# Release pull request workflow

A release PR command is user-defined. Example:

```toml
[cli.release-pr]
help_text = "Prepare a release pull request"
inputs = [
	{ name = "format", type = "choice", choices = ["markdown", "json"], default = "markdown" },
	{ name = "no_verify", type = "boolean", default = false },
]
steps = [
	{ name = "plan release", type = "PrepareRelease", allow_empty_changesets = true, inputs = ["format"] },
	{ name = "refresh lockfile", type = "Command", command = "pnpm install --lockfile-only", when = "{{ number_of_changesets > 0 }}" },
	{ name = "create release commit", type = "CommitRelease", when = "{{ number_of_changesets > 0 }}" },
	{ name = "open release request", type = "OpenReleaseRequest", when = "{{ number_of_changesets > 0 }}", inputs = ["format"] },
]
```

Preview with `--dry-run` when supported before allowing the workflow to mutate branches.
