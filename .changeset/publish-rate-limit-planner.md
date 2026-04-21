---
monochange: feat
monochange_core: feat
monochange_config: feat
monochange_npm: fix
---

#### add publish rate-limit planning and batched publish metadata

monochange can now plan package-registry publish work before mutating registries. The new default `mc publish-plan` command renders built-in rate-limit metadata, recommended publish batches, evidence links, and optional CI snippets for GitHub Actions or GitLab CI. `mc publish` and `mc placeholder-publish` now also accept repeated `--package` filters so the planned batches can be executed one window at a time.

**Before:**

```bash
# no dedicated planning command
mc publish --dry-run
mc placeholder-publish --dry-run
```

Built-in publish flows had no registry-window report and no first-class batch plan to hand off to CI.

**After:**

```bash
mc publish-plan --format json
mc publish-plan --mode placeholder --ci github-actions
mc publish --dry-run --package core --package web --format json
mc placeholder-publish --package core --format json
```

`mc publish-plan` now reports:

- per-registry publish windows
- `batches` with explicit package ids per window
- evidence URLs plus confidence levels
- optional CI snippets that expand to `mc publish --package ...` invocations

Representative JSON now includes a publish-rate-limit section alongside publish output:

```json
{
	"packagePublish": { "...": "..." },
	"publishRateLimits": {
		"windows": [
			{
				"registry": "pub_dev",
				"pending": 13,
				"batchesRequired": 2,
				"fitsSingleWindow": false
			}
		],
		"batches": [
			{
				"registry": "pub_dev",
				"batchIndex": 1,
				"totalBatches": 2,
				"packages": ["core", "web"]
			}
		]
	}
}
```

Built-in catalog coverage now includes `crates.io`, `npm`, `jsr`, and `pub.dev`, with confidence and evidence attached to each policy.

#### add `publish.rate_limits.enforce` to workspace configuration

`monochange_config` and `monochange_core` now model per-ecosystem and per-package publish rate-limit enforcement so teams can decide whether planned overages should warn or block.

**Before (`monochange.toml`):**

```toml
[ecosystems.dart.publish]
mode = "builtin"
```

**After:**

```toml
[ecosystems.dart.publish]
mode = "builtin"

[ecosystems.dart.publish.rate_limits]
enforce = true
```

When `enforce = true`, built-in publish commands stop before running a package set that requires more than one known registry window. This lets CI fail early and lets teams split the work into planned follow-up batches instead of discovering throttling halfway through a release.

#### extend public publish planning types in `monochange_core`

`monochange_core` now exposes rate-limit settings and batch metadata for callers that build their own CLI or automation around monochange release plans.

**Before (`monochange_core`):**

```rust
pub struct PublishSettings {
	pub enabled: bool,
	pub mode: PublishMode,
	pub registry: Option<PublishRegistry>,
	pub trusted_publishing: TrustedPublishingSettings,
	pub placeholder: PlaceholderSettings,
}

pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub warnings: Vec<String>,
}
```

**After:**

```rust
pub struct PublishSettings {
	pub enabled: bool,
	pub mode: PublishMode,
	pub registry: Option<PublishRegistry>,
	pub trusted_publishing: TrustedPublishingSettings,
	pub rate_limits: PublishRateLimitSettings,
	pub placeholder: PlaceholderSettings,
}

pub struct PublishRateLimitReport {
	pub dry_run: bool,
	pub windows: Vec<RegistryRateLimitWindowPlan>,
	pub batches: Vec<PublishRateLimitBatch>,
	pub warnings: Vec<String>,
}
```

This keeps config parsing, runtime enforcement, dry-run JSON, and library consumers aligned around the same publish-batch model.

#### fix standalone npm package discovery ids in `monochange_npm`

`monochange_npm` now normalizes standalone package ids relative to the discovery root so repositories that contain multiple standalone `package.json` files no longer collapse distinct packages into one record.

**Before:**

- two standalone npm packages under different directories could share the same internal `npm:package.json` id
- the adapter could keep only one package depending on filesystem traversal order

**After:**

- standalone npm packages get stable root-relative ids such as `npm:packages/docs/package.json`
- publish planning and other discovery-driven flows now keep both packages consistently across platforms
