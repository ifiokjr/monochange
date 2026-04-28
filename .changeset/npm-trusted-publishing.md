---
monochange: patch
---

#### publish CLI npm packages with trusted publishing

monochange's own CLI npm package workflow now publishes without `NODE_AUTH_TOKEN` or `NPM_TOKEN`. The publish job keeps the protected `publisher` environment and `id-token: write` permission so npm can use GitHub OIDC trusted publishing and produce provenance for the CLI packages.

**Before:**

```yaml
- name: publish cli npm packages
  env:
    NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
  run: node scripts/npm/publish-packages.mjs --packages-dir packages
```

**After:**

```yaml
- name: publish cli npm packages
  run: node scripts/npm/publish-packages.mjs --packages-dir packages
```

The publish script rejects long-lived npm token environment variables and verifies it is running from `monochange/monochange`'s `publish.yml` workflow with GitHub Actions OIDC context before invoking `npm publish --provenance`.
