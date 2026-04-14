---
monochange: minor
monochange_core: minor
monochange_config: minor
monochange_hosting: minor
monochange_github: minor
monochange_gitlab: minor
monochange_gitea: minor
---

#### add publish configuration and publication targets

monochange now carries publish metadata through configuration loading and release preparation so package publishing can be planned alongside changelogs, tags, and release requests.

**Before (`monochange.toml`):**

```toml
[package.core]
path = "crates/core"
type = "cargo"
```

**After:**

```toml
[ecosystems.cargo.publish]
mode = "builtin"
trusted_publishing = true

[package.core]
path = "crates/core"
type = "cargo"

[package.core.publish.placeholder]
readme_file = "docs/core-placeholder.md"
```

Package-level placeholder settings now override inherited ecosystem defaults cleanly, so a package can switch from an inline placeholder README to a file-backed README without triggering a config error.

Prepared release JSON and rendered release manifests now include `packagePublications` entries that describe which packages should be published, which registry they target, and whether they use built-in or external publishing mode.

For npm-family workspaces, monochange now follows the detected workspace manager when running built-in publish commands, so pnpm workspaces publish with `pnpm publish` while still routing trusted-publishing setup through the npm CLI.

**Before (`mc release --format json`):**

```json
{
	"manifest": {
		"releaseTargets": [
			{ "id": "core", "version": "1.2.3" }
		]
	}
}
```

**After:**

```json
{
	"manifest": {
		"releaseTargets": [
			{ "id": "core", "version": "1.2.3" }
		],
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
