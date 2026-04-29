---
"monochange": patch
---

# enforce trusted publishing before registry publish commands

Packages with effective `publish.trusted_publishing = true` now fail before monochange invokes a built-in registry publish command unless the current environment exposes a verifiable CI/OIDC identity.

For GitHub Actions trusted publishing, monochange verifies the configured repository, workflow, optional environment, and `id-token: write` OIDC request variables. npm packages also reject long-lived token variables such as `NPM_TOKEN` and `NODE_AUTH_TOKEN` so trusted publishing cannot silently fall back to token-based publishing.

Use the same package configuration as before:

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.npm.publish.trusted_publishing]
workflow = "publish.yml"
environment = "publisher"
```

Run release publishing from the configured CI workflow, or set `publish.trusted_publishing = false` on an individual package when that package intentionally uses a manual publishing path.
