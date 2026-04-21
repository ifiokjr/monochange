---
monochange: feat
---

#### add built-in package publishing and placeholder bootstrap commands

monochange can now publish package artifacts directly from its own release state instead of leaving registry publication entirely to external scripts.

**Before:**

```bash
mc release --dry-run --format json
mc publish-release --dry-run --format json
```

`mc publish-release` only handled hosted/provider releases such as GitHub releases. Package registry publication still had to be wired separately.

**After:**

```bash
mc placeholder-publish --format text
mc publish --format text
mc publish-release --format json
```

- `mc placeholder-publish` checks each built-in package registry and publishes a placeholder `0.0.0` package only when the package does not exist yet
- `mc publish` reads monochange release state and runs the built-in registry publish flow for supported public registries
- npm workspaces that use `pnpm` now publish with `pnpm publish`, and trusted-publishing setup runs through `pnpm exec npm trust ...`

**Before (`mc release --dry-run --format json`):**

```json
{
	"manifest": {
		"releaseTargets": [{ "id": "core", "version": "1.2.3" }]
	}
}
```

**After:**

```json
{
	"manifest": {
		"releaseTargets": [{ "id": "core", "version": "1.2.3" }],
		"packagePublications": [
			{
				"package": "core",
				"ecosystem": "cargo",
				"registry": "crates_io",
				"mode": "builtin",
				"version": "1.2.3"
			}
		]
	}
}
```

Built-in publishing also reports trusted-publishing status in text, markdown, and JSON output, including manual setup URLs when a registry still needs trust configured.
