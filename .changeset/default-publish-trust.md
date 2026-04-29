---
"monochange_config": patch
"monochange": patch
---

#### Document ecosystem-level trusted publishing inheritance

Trusted publishing stays an ecosystem-level publish setting that packages inherit by default:

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[package.legacy.publish]
trusted_publishing = false
```

Use `[ecosystems.<name>.publish.trusted_publishing]` for shared repository, workflow, and environment metadata. Package-level publish settings override the ecosystem defaults for package-specific workflows or opt-outs.
