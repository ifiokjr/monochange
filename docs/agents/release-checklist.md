# Release prerequisites checklist

A pre-release verification list for agents working on the monochange repository. Run every item before cutting a release. Fix any blockers before proceeding.

## 1. Placeholder publishing

New crates that do not yet exist on their target registry must be placeholder-published before the main release can succeed. Crates.io rejects publications when the crate name is completely unknown; a placeholder version (typically `0.0.0`) reserves the name and lets subsequent real versions through.

- [ ] Identify every package in `monochange.toml` whose registry entry does not yet exist. Run `cargo search <crate>` for each cargo package, and `npm view <pkg> version` for each npm package. Anything that returns "not found" needs a placeholder.
- [ ] For each missing crate, verify its `Cargo.toml` does **not** have `publish = false`. If it does, the crate is intentionally private and should also have `tag = false, release = false, changelog = false` in `monochange.toml`.
- [ ] Run `mc placeholder-publish` to publish placeholder versions for every missing package. This step is manual and must happen **before** the release PR is merged.
- [ ] Verify placeholders landed: re-run the registry search for each crate.

**Why it matters.** If a new crate is in the release group (`tag = true`, `release = true`) but has no registry entry, the publish job will fail partway through. crates.io will 404 on the crate name, and the remaining batches never execute.

## 2. Group and package consistency

- [ ] Every package in a release group (`tag = true` / `release = true`) must be publishable on its registry. Packages with `publish = false` in their manifest should have `tag = false`, `release = false`, and `changelog = false` in `monochange.toml`, and should ideally not be members of a release group at all.
- [ ] Run `mc validate` and confirm no warnings about group members that contradict their package manifests.
- [ ] Check that `defaults.include_private` in `monochange.toml` is set correctly (currently `false`), and that no private package accidentally appears in a release group.

## 3. Batched publisher readiness

- [ ] Run `mc publish-plan --mode publish --format json` locally and inspect the output. Confirm that:
  - Every expected package appears in the plan.
  - Crate batches respect crates.io rate limits (1 new crate / 5 min, or OIDC-provenance batch limits).
  - npm platform packages are **excluded** from the `mc publish-plan` output if they are no longer published by this script (npm publishing is now handled by `mc publish`).
- [ ] If crates.io OIDC trusted publishing is used (which it should be), confirm the publish job runs in the `publisher` environment with `id-token: write` permission.
- [ ] Confirm the publish matrix gracefully handles zero batches (no packages to publish) without failing the workflow.

## 4. CI workflow flow verification

```text
push to main → ci.yml (release-pr) → manual merge → release-pr-merge.yml
  → push to main → ci.yml (release-post-merge: tag + draft release)
  → tag push → release.yml (cross-compile, upload assets, draft release)
  → publish.yml (build npm packages, plan batches, publish cargo batches)
```

- [ ] Verify each step above is present and triggered by the correct event.
- [ ] Confirm `mc release-record --from HEAD` correctly detects a release commit after merge (the `release-post-merge` job gates on this).
- [ ] Confirm tag push triggers `release.yml` automatically (`on: push: tags: "v*"`).
- [ ] Verify concurrency groups prevent duplicate runs without silently canceling important work.

## 5. npm publishing flow

npm packages are handled differently from cargo crates. Platform-specific npm packages (`@monochange/cli-darwin-arm64`, etc.) and the main CLI wrapper (`@monochange/cli`) are built and populated by `scripts/npm/populate-packages.mjs` inside the `publish.yml` **plan** job. They are now published by `mc publish` alongside cargo crates in the **publish** job.

- [ ] Confirm `scripts/npm/build-packages.mjs` runs **before** `scripts/npm/populate-packages.mjs` so binaries are populated.
- [ ] Confirm `scripts/npm/populate-packages.mjs` validates binary presence before populate.
- [ ] Verify that `mc publish-plan` does **not** re-include npm packages in its batch output for a second publish attempt. If `mc publish-plan` cannot filter by ecosystem, pass `--package` flags to exclude npm packages from the cargo publish.

## 6. Trusted publishing and OIDC

- [ ] crates.io: the `publish.yml` workflow uses `rust-lang/crates-io-auth-action@v1` for OIDC. Confirm the `publisher` environment has `id-token: write` permission and no additional restrictions that block the OIDC flow.
- [ ] npm: trusted publishing relies on GitHub Actions OIDC. Confirm the `publish` job has `id-token: write` permission.
- [ ] Confirm no long-lived `CARGO_REGISTRY_TOKEN`, `NODE_AUTH_TOKEN`, or `NPM_TOKEN` secrets are set in the repository or environment. These would bypass OIDC and lose provenance benefits.

## 7. Environment gates

- [ ] The `publisher` GitHub environment should require approval **only** for jobs that actually publish (the `publish.yml` publish job).
- [ ] The `ci.yml` `release-pr` job should **not** use the `publisher` environment. It only creates a PR and does not publish anything.
- [ ] Verify the `release-pr-merge.yml` workflow's `RELEASE_PR_MERGE_TOKEN` secret is available outside the `publisher` environment (it uses `secrets.RELEASE_PR_MERGE_TOKEN` directly, not through an environment).

## 8. Changeset and version validation

- [ ] Run `mc validate` and fix any errors or warnings.
- [ ] Run `lint:monochange` (devenv script) and fix any lint errors.
- [ ] Check that every changeset targeting a new package uses `major` (not `patch`) for the first release of that package.
- [ ] Verify the `main` group's `version_format = "primary"` is set — only one package or group may use `"primary"` and it produces the top-level `vX.Y.Z` tag.
- [ ] Verify `versioned_files` in the group entry covers all files that need version bumps (currently `Cargo.toml` with `workspace.package.version`).

## 9. Asset build and attestation

- [ ] `release.yml` cross-compiles for all targets in the matrix. Verify the target list matches what npm platform packages expect (`darwin-arm64`, `darwin-x64`, `linux-arm64-gnu`, `linux-arm64-musl`, `linux-x64-gnu`, `linux-x64-musl`, `win32-x64-msvc`, `win32-arm64-msvc`).
- [ ] Verify `taiki-e/upload-rust-binary-action` is configured with `archive: "$bin-$target-$tag"` — the download step in `publish.yml` matches this pattern (`monochange-*-${RELEASE_TAG}.tar.gz` / `.zip`).
- [ ] Build attestations (`actions/attest-build-provenance@v3`) and verification steps exist and pass.

## 10. Post-release verification

- [ ] Confirm the GitHub Release is non-draft and marked as latest.
- [ ] Confirm all cross-compiled binaries appear as release assets.
- [ ] Confirm crate versions are live on crates.io (`cargo search monochange`).
- [ ] Confirm npm packages are live on npmjs.com (`npm view @monochange/cli version`).
- [ ] Confirm tags exist in the repo for each release target.
- [ ] Confirm `mc release-record --from HEAD` shows the release record as resolved.
- [ ] Confirm issues referenced in changesets received close/comment notifications.
- [ ] Confirm docs deployed to GitHub Pages (if applicable).
