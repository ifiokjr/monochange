# Migration workflow templates

Use this when creating migration PRs for repositories moving from knope, NOPE/changesets, or other release tools to monochange. Contains ecosystem-specific notes, CI workflow templates, and binary release guides.

## Ecosystem-specific checklists

### Rust / cargo monorepo

- [ ] Set `package_type = "cargo"` in `[defaults]`
- [ ] Create `[package.<id>]` entries with `path` only — Cargo.toml discovery is automatic
- [ ] Add `[group.<id>]` if packages share a version
- [ ] Set `parent_bump = "patch"` (or `"none"`) for transitive dependency bumps
- [ ] Create `release.yml`, `publish.yml` (with crates.io OIDC), `changeset-policy.yml`
- [ ] Convert changeset frontmatter: `default:` → group ID, `crate_scope:` → package ID
- [ ] Add `# heading` to changeset body if missing
- [ ] Remove knope from devenv.nix and Cargo.toml workspace metadata

### npm / pnpm monorepo

- [ ] Set `package_type = "npm"` in `[defaults]`
- [ ] Create `[package.<id>]` entries with `path` only — package.json discovery is automatic
- [ ] Set `version_format = "namespaced"` on the group for scoped packages
- [ ] Create `publish.yml` with `NPM_CONFIG_PROVENANCE: true`
- [ ] Convert `default: minor` → group ID in changesets
- [ ] Remove knope from package.json devDependencies

### Dart / Flutter monorepo

- [ ] Set `package_type = "dart"` or `"flutter"` per package
- [ ] Mark app packages with `publish = { enabled = false }`
- [ ] Create `publish.yml` with pub.dev OIDC (`environment: publisher`)
- [ ] Convert short scopes (`app:`, `ui:`) to full package IDs (`my_app:`, `my_ui:`)
- [ ] Add `# heading` to changeset body if missing
- [ ] Set up `melos publish --no-dry-run` or `dart pub publish --force`

### Mixed ecosystem (Rust + npm + CLI binary)

- [ ] Set `[defaults] package_type for the dominant ecosystem
- [ ] Override `type` per package for minority ecosystem packages
- [ ] Create `publish.yml` with separate jobs per registry
- [ ] Create `release.yml` with cross-compilation steps for CLI binary
- [ ] Add monochange to devenv packages (extra.monochange)

## CI workflow templates

### release.yml — basic (no binary)

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
      - run: mc release-pr
        shell: devenv shell -- bash -e {0}
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
```

### release.yml — with CLI binary (Rust)

Add after `release-pr` job:

```yaml
  upload-assets:
    needs: release-pr
    if: startsWith(github.ref_name, 'v') || startsWith(inputs.tag, 'v')
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
          gh release download "${{ github.ref_name }}" \
            --pattern 'YOUR_BINARY-*' \
            --dir "$asset_dir"
      - uses: actions/attest-build-provenance@v3
        with:
          subject-path: ${{ runner.temp }}/release-assets/*
```

### publish.yml — crates.io with OIDC

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
      - uses: rust-lang/crates-io-auth-action@v1
        id: crates-oidc
      - run: mc step:publish-readiness --from HEAD --format json
        shell: devenv shell -- bash -e {0}
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}
      - run: mc step:publish-packages --all --format json
        shell: devenv shell -- bash -e {0}
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.crates-oidc.outputs.token }}
```

### publish.yml — npm with provenance

```yaml
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
      - uses: cachix/install-nix-action@v31
        with:
          github_access_token: ${{ secrets.GITHUB_TOKEN }}
      - run: nix profile add --accept-flake-config nixpkgs#cachix && cachix use devenv && nix profile add nixpkgs#devenv
        shell: bash
      - uses: pnpm/action-setup@v6
        with:
          version: 10
      - uses: actions/setup-node@v6
        with:
          node-version: 24
          registry-url: https://registry.npmjs.org
      - run: pnpm install --frozen-lockfile
      - run: pnpm -r publish --access public --no-git-checks
        env:
          NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
          NPM_CONFIG_PROVENANCE: true
```

### publish.yml — pub.dev with OIDC

```yaml
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
      - uses: cachix/install-nix-action@v31
        with:
          github_access_token: ${{ secrets.GITHUB_TOKEN }}
      - run: nix profile add --accept-flake-config nixpkgs#cachix && cachix use devenv && nix profile add nixpkgs#devenv
        shell: bash
      - uses: dart-lang/setup-dart@v1
        with:
          sdk: stable
      - run: dart pub get
      - run: melos publish --no-dry-run
```

### changeset-policy.yml

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
