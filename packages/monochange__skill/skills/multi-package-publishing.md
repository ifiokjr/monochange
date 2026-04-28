# Multi-package publishing patterns

Use this guide when one repository publishes more than one public package and the publish workflow cannot be explained by a single generic `mc publish-readiness` plus `mc publish --readiness` example.

This is the practical companion to [trusted-publishing.md](./trusted-publishing.md).

## Core idea

For multi-package repositories, keep one distinction clear:

- monochange plans releases at the workspace level
- registries authorize publishing at the package level

That means one release commit can describe the whole workspace while publish automation still needs to stay package-specific.

A good default rollout is:

1. let monochange prepare one release commit for the workspace
2. decide which packages use built-in publishing and which use `mode = "external"`
3. keep each registry's trusted-publishing enrollment aligned with the exact workflow, tag pattern, and working directory that will publish that package

## Pattern 1: one post-merge `mc publish-readiness` and `mc publish --readiness` job

Use this when most packages can stay on monochange's built-in publishing path.

```yaml
name: publish-packages

on:
  push:
    branches: [main]

jobs:
  publish:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - uses: ./.github/actions/devenv
        with:
          github-token: ${{ secrets.GITHUB_TOKEN }}

      - name: detect monochange release commit
        shell: bash
        run: |
          set -euo pipefail
          if ! devenv shell -- mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "HEAD is not a monochange release commit; skipping publish"
            exit 0
          fi

      - name: publish packages
        run: |
          devenv shell -- mc publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- mc publish --readiness .monochange/readiness.json
```

This is the best fit when:

- multiple npm packages publish from the same workflow
- multiple packages share the same built-in post-merge flow
- you do not need package-specific tag triggers to satisfy the registry

## Pattern 2: package-specific external workflows

Use this when the registry expects each package to have its own tag trigger, working directory, or workflow.

This is often the clearest fit for:

- `pub.dev`
- some `crates.io` setups
- mixed workspaces where one package needs registry-native steps that do not match `mc publish`

Example tag naming scheme:

- `web-v{{version}}`
- `cli-v{{version}}`
- `dart_client-v{{version}}`

Example config:

```toml
[ecosystems.cargo.publish]
enabled = true
mode = "external"
trusted_publishing = true

[ecosystems.dart.publish]
enabled = true
mode = "external"
trusted_publishing = true
registry = "pub.dev"
```

Example workflow shape:

```yaml
name: publish-dart-client

on:
  push:
    tags:
      - "dart_client-v[0-9]+.[0-9]+.[0-9]+"

jobs:
  publish:
    permissions:
      id-token: write
    uses: dart-lang/setup-dart/.github/workflows/publish.yml@v1
    with:
      working-directory: packages/dart_client
      # environment: pub.dev
```

Choose this pattern when a tag for one package must never authorize publishing a different package.

## Pattern 3: one workflow, multiple package-specific jobs

Use this when you want one workflow file but separate jobs per package.

That gives you:

- one place to manage permissions and branch or tag triggers
- package-specific working directories
- package-specific environments
- package-specific failure visibility

Example shape:

```yaml
jobs:
  publish-crate-a:
    environment: crates-a
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
        id: auth
      - run: cargo publish --package crate_a
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}

  publish-crate-b:
    environment: crates-b
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
        id: auth
      - run: cargo publish --package crate_b
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
```

This pattern is especially useful when multiple packages live in the same ecosystem but should not share the same trusted-publishing enrollment.

## Registry-specific recommendations

| Registry  | Recommended multi-package pattern                                                                         | Why                                                                   |
| --------- | --------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------- |
| npm       | one post-merge `mc publish-readiness` + `mc publish --readiness` job when possible                        | monochange can automate npm trusted-publishing setup on GitHub        |
| crates.io | one job per crate when using external OIDC auth                                                           | trusted publishing is enrolled per crate and workflow context matters |
| jsr       | built-in `mc publish-readiness` + `mc publish --readiness` is often fine, but keep setup package-specific | registry linking is still manual today                                |
| pub.dev   | package-specific tags and often one workflow per package                                                  | automated publishing is tag-driven and package-specific               |

## Keep config, tags, and workflows aligned

For each published package, keep these values aligned:

- package id in `monochange.toml`
- registry package name
- trusted-publishing repository, workflow, and environment values
- workflow trigger
- tag pattern, when the registry uses tags
- working directory, when the registry workflow publishes from a subdirectory

If those drift apart, trusted-publishing validation will be confusing even when release planning is correct.

## When to use package-level overrides

Use package-level publishing config when one package differs from the ecosystem default.

```toml
[ecosystems.dart.publish]
enabled = true
mode = "external"
trusted_publishing = true
registry = "pub.dev"

[package.dart_client.publish.trusted_publishing]
workflow = "publish-dart-client.yml"
environment = "pub.dev"

[package.example_app.publish]
enabled = false
```

This is the right move when:

- one package publishes from a different workflow file
- one package needs a protected environment but others do not
- one package is internal and should not publish publicly
- one ecosystem default is correct for most packages, but not all of them

## Practical rollout

1. decide which packages are public and which stay unpublished
2. choose `builtin` or `external` per ecosystem or package
3. register trusted publishing for each package at the registry
4. prefer package-specific tags where a registry is tag-authorized
5. run `mc publish --dry-run` after registry enrollment changes
6. keep the workflow filename and environment stable once a registry record is enrolled

## Common mistakes

Avoid these failure modes:

- using one broad tag pattern that lets a tag for package A publish package B
- reusing one trusted-publishing record across packages that actually publish from different workflows
- changing a workflow filename after registry enrollment without updating the registry record
- keeping `mode = "builtin"` for packages that really need registry-native external publish steps
- forgetting that `pub.dev` automated publishing is tag-triggered

## Related references

- [trusted-publishing.md](./trusted-publishing.md) — registry-side trusted-publishing setup details
- [configuration.md](./configuration.md) — publishing config and per-package overrides
- [reference.md](./reference.md) — broader publishing and release workflow reference
