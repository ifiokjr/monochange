# Publishing workflow

Built-in readiness and bootstrap commands:

```bash
mc step:publish-readiness --from HEAD --output readiness.json
mc step:placeholder-publish --from HEAD --output bootstrap.json
```

Repository-specific workflow commands can wrap publish planning and publishing:

```toml
[cli.publish-plan]
help_text = "Plan package publishing"
inputs = [
	{ name = "format", type = "choice", choices = ["markdown", "json"], default = "markdown" },
	{ name = "readiness", type = "path" },
]
steps = [
	{ name = "plan publish rate limits", type = "PlanPublishRateLimits", inputs = ["format", "mode", "package", "readiness", "ci"] },
]

[cli.publish]
help_text = "Publish package artifacts"
inputs = [
	{ name = "output", type = "path" },
	{ name = "resume", type = "path" },
]
steps = [
	{ name = "publish packages", type = "PublishPackages", inputs = ["format", "package", "group", "ecosystem", "resume", "output"] },
]
```

Use dry-run checks and keep JSON artifacts for resume/retry.
