# Trusted publishing

monochange publishing settings can opt packages into trusted/OIDC publishing where supported.

## Configuration

```toml
[ecosystems.npm.publish]
trusted_publishing = true

[package."@acme/api".publish]
enabled = true
mode = "builtin"
registry = "npm"
trusted_publishing = true
```

Use a table when the trust context needs explicit repository/workflow/environment metadata:

```toml
[package."@acme/api".publish.trusted_publishing]
enabled = true
repository = "acme/widgets"
workflow = "publish.yml"
environment = "npm"
```

Registry support and setup requirements vary. Treat trusted-publishing setup as a registry-side operation that may require a human maintainer. Agents should generate configuration and workflow code, but should not use local credentials or perform registry-side changes unless explicitly authorized and allowed by project policy.

## Per-registry setup

### crates.io

Use `rust-lang/crates-io-auth-action@v1` in your publish workflow. It auto-generates a short-lived OIDC token from GitHub Actions — no `CARGO_REGISTRY_TOKEN` secret needed.

```yaml
- name: get crates.io OIDC token
  uses: rust-lang/crates-io-auth-action@v1
  id: crates-oidc

- name: publish cargo packages
  env:
  CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}
  run: mc step:publish-packages --all --format json
```

Required workflow permissions: `id-token: write`.

### npm

Enable provenance attestations with `NPM_CONFIG_PROVENANCE = true`:

```yaml
- uses: actions/setup-node@v6
  with:
    node-version: 24
    registry-url: https://registry.npmjs.org
- run: pnpm -r publish --access public --no-git-checks
  env:
    NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
    NPM_CONFIG_PROVENANCE: true
```

For full OIDC (no `NPM_TOKEN` secret), set up npm trusted publishing at [npm docs](https://docs.npmjs.com/generating-provenance-statements#publishing-packages-with-provenance-via-github-actions).

### pub.dev (Dart)

1. Go to your package on pub.dev → Admin → Automated Publishing
2. Add GitHub Actions as publisher with repository, workflow, environment
3. In your workflow, set `environment: publisher` and `permissions: id-token: write`
4. Use `dart pub publish --force` or `melos publish --no-dry-run`

```yaml
- uses: dart-lang/setup-dart@v1
  with:
    sdk: stable
- run: dart pub get
- run: melos publish --no-dry-run
```

Setup guide: [dart.dev/tools/pub/automated-publishing](https://dart.dev/tools/pub/automated-publishing)

## Security model

- **No long-lived tokens**: OIDC tokens are short-lived and scoped to the CI run
- **No local credentials**: Agents should never use `CARGO_REGISTRY_TOKEN`, `NPM_TOKEN`, or `PUB_DEV_TOKEN` secrets — all publishing should go through CI/OIDC
- **Verified builds**: Tags are created only from release PRs that merge into main
- **Registry provenance**: crates.io and npm both support signed provenance attestations when publishing from CI
