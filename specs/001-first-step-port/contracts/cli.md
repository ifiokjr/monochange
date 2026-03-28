# CLI Contract: Release Planning Foundation

## Purpose

Define the user-facing command contract for workspace discovery and release planning in the first monochange milestone.

## Command 1: Workspace Discovery

```bash
mc workspace discover --root <path> --format <text|json>
```

### Behavior

- Discovers supported packages from native workspaces and standalone manifests.
- Resolves supported glob-based workspace entries.
- Produces a unified view of packages, dependency edges, version groups, and warnings.
- Does not modify repository files.

### Text Output Requirements

- Summarize discovered packages by ecosystem.
- Show grouped packages and warnings.
- Identify standalone packages separately from workspace-managed packages when relevant.

### JSON Output Contract

```json
{
	"workspaceRoot": ".",
	"packages": [
		{
			"id": "cargo:crates/sdk_core",
			"name": "sdk_core",
			"ecosystem": "cargo",
			"manifestPath": "crates/sdk_core/Cargo.toml",
			"version": "1.2.0",
			"versionGroup": "sdk",
			"publishState": "public"
		}
	],
	"dependencies": [
		{
			"from": "npm:packages/web-sdk",
			"to": "cargo:crates/sdk_core",
			"kind": "runtime",
			"direct": true
		}
	],
	"versionGroups": [
		{
			"id": "sdk",
			"members": [
				"cargo:crates/sdk_core",
				"npm:packages/web-sdk"
			]
		}
	],
	"warnings": []
}
```

## Command 2: Release Plan Generation

```bash
mc plan release --root <path> --changes <path> --format <text|json>
```

### Behavior

- Reads explicit change input.
- Calculates release impact through direct and transitive dependency edges.
- Applies default patch propagation to parents when no stronger compatibility signal exists.
- Applies version-group synchronization before finalizing output.
- Includes compatibility evidence when a provider escalates severity.

### Change Input Contract

- The changes file must support multiple changed packages.
- Each entry must identify a package and an optional explicit bump severity.
- Each entry may carry an optional human-readable reason.

### JSON Output Contract

```json
{
	"workspaceRoot": ".",
	"decisions": [
		{
			"package": "cargo:crates/sdk_core",
			"bump": "minor",
			"trigger": "direct-change",
			"reasons": ["public API addition"]
		},
		{
			"package": "npm:packages/web-sdk",
			"bump": "patch",
			"trigger": "transitive-dependency",
			"reasons": ["depends on cargo:crates/sdk_core"]
		}
	],
	"groups": [
		{
			"id": "sdk",
			"plannedVersion": "1.3.0",
			"members": [
				"cargo:crates/sdk_core",
				"npm:packages/web-sdk"
			]
		}
	],
	"warnings": [],
	"compatibilityEvidence": [
		{
			"package": "cargo:crates/sdk_core",
			"provider": "rust-semver",
			"severity": "major",
			"summary": "public API break detected"
		}
	]
}
```

## Non-Goals for this Contract

- No GitHub bot workflow triggers.
- No publishing side effects.
- No remote API interactions.
