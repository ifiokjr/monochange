# Adoption and migration guide

Use this when adding monochange to an existing monorepo or migrating from another release tool (knope, NOPE/changesets, semantic-release, custom).

## Discovery checklist

Before writing any config, gather:

1. **Ecosystems**: Cargo, npm/pnpm, Deno, Dart/Flutter, Python, Go — which are present?
2. **Package IDs**: Which packages should be release-managed? Use manifest names as IDs where possible.
3. **Private packages**: Which packages should be excluded from releases (`publish = false`)?
4. **Version groups**: Which packages must share a version (lockstep)? Which are independently versioned?
5. **Lockfiles**: Which lockfiles or generated schemas must refresh after version changes?
6. **Current CI**: Which release/publish CI jobs exist? Should monochange replace or integrate with them?
7. **Changelog format**: `keep_a_changelog` (default) or `monochange` (richer sections)?
8. **Binary releases**: Any CLI binaries that need cross-compilation and GitHub Release assets?

## Initial commands

```bash
mc init --provider github
mc step:validate
mc step:discover --format json
mc check
```

Edit the generated config rather than accepting it blindly.

## Migration by source tool

### From knope

See the dedicated guide: [Migrating from knope](../../docs/src/guide/10-migrating-from-knope.md).

Key translations:

| knope                              | monochange                                      |
| ---------------------------------- | ----------------------------------------------- |
| `knope.toml`                       | `monochange.toml`                               |
| `[package]` with `versioned_files` | `[package.<id>]` with `path`                    |
| `[[workflows]]` arrays             | `[cli.<command>]` map entries                   |
| `[github]`                         | `[source]` (provider-neutral)                   |
| `scopes`                           | Not needed — use package IDs in changesets      |
| `knope release`                    | `mc release-pr` (opens/updates PR)              |
| `knope forced-release`             | Automatic on PR merge                           |
| `knope document-change`            | `mc step:create-change-file` or `mc change`     |
| `default: minor` (lockstep)        | Group ID in changeset (e.g., `main: minor`)     |
| `app: patch` (scoped)              | Package ID in changeset (e.g., `my_app: patch`) |

### From NOPE / Atlassian changesets

NOPE changesets use YAML frontmatter similar to monochange, but the body format differs:

**Before** (NOPE):

```markdown
---
my_crate: minor
---

Add new LSP feature.
```

**After** (monochange):

```markdown
---
my_crate: minor
---

# Add new LSP feature

Detailed description of the feature.
```

Key difference: monochange requires a `# heading` as the first line of the body.

### From semantic-release / custom

There is no automated migration path. Follow the adoption checklist from the top.

## Ecosystem-specific notes

### Rust / cargo

- monochange auto-discovers and updates `Cargo.toml` versions — no `versioned_files` needed for the crate manifest
- Add `Cargo.lock` or workspace inheritance entries only if you need extra version tracking
- Use `[group.<id>]` for lockstep versioned workspaces
- Set `parent_bump = "patch"` (or `"none"`) for transitive dependency bumps
- For CLI binaries, see the [binary release guide](#binary-release-for-rust-clis) below

### npm / pnpm

- monochange manages `package.json` `version` fields automatically
- Use `[ecosystems.npm]` for lockfile commands (`pnpm install`, etc.)
- Set `version_format = "namespaced"` for scoped packages (tags like `@scope/pkg@1.0.0`)
- Enable npm provenance: `NPM_CONFIG_PROVENANCE = true` in CI

### Dart / Flutter

- monochange manages `pubspec.yaml` `version` fields automatically
- Use melos for workspace publishing: `melos publish --no-dry-run`
- Set `package_type = "dart"` or `"flutter"` per package
- Use `publish = false` for app packages with `publish_to: none`

### Deno

- monochange manages `deno.json` or `mod.ts` version exports
- Set `package_type = "deno"`

### Python

- monochange manages `pyproject.toml` versions
- Set `package_type = "python"`

### Go

- monochange manages Go module versions
- Set `package_type = "go"`

## CI workflow templates

Every monochange migration needs at least 3 GitHub Actions workflows. Below are complete templates.

### 1. `.github/workflows/release.yml` — Release PR + tagging

```yaml
name: release

on:
  push:
    branches: [main]

concurrency:
  group: ${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true

jobs:
  release-pr:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0
      - uses: cachix/install-nix-action@v31
        with:
          github_access_token: ${{ secrets.GITHUB_TOKEN }}
      - run: nix profile add --accept-flake-config nixpkgs#cachix && cachix use devenv && nix profile add nixpkgs#devenv
        shell: bash
      - name: create or update release PR
        run: mc release-pr
        shell: devenv shell -- bash -e {0}
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

monochange handles tag creation and GitHub release publishing automatically when the release PR merges. No separate `forced-release` step needed.

### 2. `.github/workflows/publish.yml` — Trusted publishing

Per-ecosystem variants:

#### crates.io (Rust) with OIDC

```yaml
name: publish

on:
  workflow_dispatch:
    inputs:
      tag:
        description: "Release tag to publish (e.g. v0.1.0)"
        required: true
        type: string

jobs:
  publish:
    environment: publisher
    permissions:
      contents: read
      id-token: write
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
        with:
          ref: ${{ inputs.tag }}
          fetch-depth: 0
      - uses: cachix/install-nix-action@v31
        with:
          github_access_token: ${{ secrets.GITHUB_TOKEN }}
      - run: nix profile add --accept-flake-config nixpkgs#cachix && cachix use devenv && nix profile add nixpkgs#devenv
        shell: bash
      - name: get crates.io OIDC token
        uses: rust-lang/crates-io-auth-action@v1
        id: crates-oidc
      - name: check publish readiness
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}
        run: mc step:publish-readiness --from HEAD --format json
        shell: devenv shell -- bash -e {0}
      - name: publish cargo packages
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}
        run: mc step:publish-packages --all --format json
        shell: devenv shell -- bash -e {0}
```

#### npm with provenance

```yaml
- uses: actions/setup-node@v6
  with:
    node-version: 24
    registry-url: https://registry.npmjs.org
- uses: pnpm/action-setup@v6
  with:
    version: 10
- run: pnpm install --frozen-lockfile
- run: pnpm -r publish --access public --no-git-checks
  env:
    NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
    NPM_CONFIG_PROVENANCE: true
```

#### pub.dev with OIDC

```yaml
- uses: dart-lang/setup-dart@v1
  with:
    sdk: stable
- run: dart pub get
- run: melos publish --no-dry-run
  # Requires OIDC publisher setup at https://dart.dev/tools/pub/automated-publishing
```

### 3. `.github/workflows/changeset-policy.yml` — PR coverage

```yaml
name: changeset-policy

on:
  pull_request:
    types: [opened, synchronize, reopened, labeled, unlabeled]

jobs:
  check:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      issues: write
      pull-requests: read
    steps:
      - uses: actions/checkout@v6
      - uses: cachix/install-nix-action@v31
        with:
          github_access_token: ${{ secrets.GITHUB_TOKEN }}
      - run: nix profile add --accept-flake-config nixpkgs#cachix && cachix use devenv && nix profile add nixpkgs#devenv
        shell: bash
      - uses: tj-actions/changed-files@v46
        id: changed
      - name: run changeset policy
        env:
          CHANGED_FILES: ${{ steps.changed.outputs.all_changed_files }}
          PR_LABELS_JSON: ${{ toJson(github.event.pull_request.labels.*.name) }}
        shell: devenv shell -- bash -e {0}
        run: |
          set -euo pipefail
          mapfile -t labels < <(jq -r '.[]' <<<"$PR_LABELS_JSON")
          args=(step:affected-packages --format json --verify)
          for path in $CHANGED_FILES; do args+=(--changed-paths "$path"); done
          for label in "${labels[@]}"; do args+=(--label "$label"); done
          mc "${args[@]}"
```

### Add monochange to devenv

Add monochange to the project's `devenv.yaml` and `devenv.nix` so it's available in both local development and CI:

#### `devenv.yaml`

```yaml
inputs:
  ifiokjr-nixpkgs:
    url: github:ifiokjr/nixpkgs
```

#### `devenv.nix`

```nix
let extra = inputs.ifiokjr-nixpkgs.packages.${pkgs.stdenv.system};
in {
  packages = [ extra.monochange ];
}
```

This makes `mc` available in `devenv shell`. All CI workflows run `mc` commands with `shell: devenv shell -- bash -e {0}` so the binary is on PATH.

## Binary release for Rust CLIs

Rust CLI binaries need cross-compilation + GitHub Release upload + npm platform packages. Add this to `release.yml`:

```yaml
upload-assets:
  if: startsWith(inputs.tag || github.ref_name, 'v')
  permissions:
    attestations: write
    contents: write
    id-token: write
  strategy:
    fail-fast: false
    matrix:
      include:
        - target: aarch64-apple-darwin
          os: macos-14
        - target: x86_64-apple-darwin
          os: macos-latest
        - target: x86_64-unknown-linux-gnu
          os: ubuntu-latest
        - target: aarch64-unknown-linux-gnu
          os: ubuntu-latest
        - target: x86_64-unknown-linux-musl
          os: ubuntu-latest
        - target: aarch64-unknown-linux-musl
          os: ubuntu-latest
        - target: x86_64-pc-windows-msvc
          os: windows-latest
        - target: aarch64-pc-windows-msvc
          os: windows-latest
  runs-on: ${{ matrix.os }}
  steps:
    - uses: actions/checkout@v6
      with:
        fetch-depth: 0
    - uses: dtolnay/rust-toolchain@stable
    - uses: taiki-e/upload-rust-binary-action@v1
      with:
        bin: YOUR_BINARY_NAME
        manifest-path: crates/your_cli/Cargo.toml
        ref: refs/tags/${{ inputs.tag || github.ref_name }}
        archive: $bin-$target-$tag
        target: ${{ matrix.target }}
        tar: all
        zip: windows
        token: ${{ secrets.GITHUB_TOKEN }}
        checksum: sha256,sha512

attest-assets:
  needs: upload-assets
  runs-on: ubuntu-latest
  permissions:
    attestations: write
    contents: write
    id-token: write
  steps:
    - uses: actions/checkout@v6
    - name: download release assets
      env:
        GH_TOKEN: ${{ github.token }}
      run: |
        asset_dir="$RUNNER_TEMP/release-assets"
        mkdir -p "$asset_dir"
        gh release download "${{ inputs.tag || github.ref_name }}" \
          --pattern 'YOUR_BINARY_PREFIX-*' \
          --dir "$asset_dir"
    - uses: actions/attest-build-provenance@v3
      with:
        subject-path: ${{ runner.temp }}/release-assets/*
```

## Trusted publishing setup per-registry

### crates.io

1. Add `rust-lang/crates-io-auth-action@v1` to your publish workflow
2. It auto-generates a short-lived OIDC token from GitHub Actions
3. No `CARGO_REGISTRY_TOKEN` secret needed
4. Ensure `permissions: id-token: write` in your workflow

### npm

1. Set `NPM_CONFIG_PROVENANCE: true` in publish step env
2. This attaches provenance attestations to published packages
3. For full OIDC (no `NPM_TOKEN` secret), set up npm trusted publishing at https://docs.npmjs.com/generating-provenance-statements
4. Until then, keep `NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}`

### pub.dev (Dart)

1. Go to your package page on pub.dev → Admin → Automated Publishing
2. Add GitHub Actions as a publisher with:
   - Repository: `owner/repo`
   - Workflow: `publish.yml`
   - Environment: `publisher`
3. In your workflow, set `environment: publisher` and `permissions: id-token: write`
4. Use `dart pub publish --force` or `melos publish --no-dry-run`

## Agent workflow for migration PRs

When an AI agent creates migration PRs, follow this workflow:

1. **Clone** the repository and create a `feat/migrate-to-monochange` branch
2. **Run** `mc init --provider github` to scaffold the config
3. **Translate** the old config (knope.toml / NOPE config) to monochange.toml:
   - Map packages with `[package.<id>]` entries and `path` fields
   - Create `[group.<id>]` for lockstep versioned packages
   - Set `[source]` with provider, owner, repo
   - Configure `[defaults]` and `[defaults.changelog]`
4. **Convert** changeset files to monochange format (add `# heading` to body)
5. **Create** GitHub Actions workflows (release.yml, publish.yml, changeset-policy.yml, monochange in devenv packages)
6. **Remove** old tooling (knope.toml, knope from devenv/nixpkgs overlay, old CI references)
7. **Validate** with `mc step:validate` and `mc check`
8. **Create** PR with migration checklist in the description

## Minimal outcome

A good initial adoption has:

- Explicit `[package.*]` entries for managed packages
- Optional `[group.*]` entries for synchronized versions
- `[ecosystems.*]` settings for enabled ecosystems
- `[changesets.affected]` if CI checks changeset coverage
- `[lints]` if `mc check` should enforce manifest rules
- `[cli.*]` workflows for common team commands
- Complete CI workflows (release, publish, changeset-policy)
