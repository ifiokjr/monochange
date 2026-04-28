# Advanced: CI, package publishing, and release PR flows

This guide brings together the practical CI patterns around `mc publish`, `mc placeholder-publish`, `mc release-pr`, `mc commit-release`, and provider release automation.

It also documents the recommended workflow for long-running release PR branches.

## Start with the command surface

These commands solve different automation problems:

<!-- {=projectCommandAutomationMatrix} -->

These are the commands most repositories use after running `mc init`. With the new CLI model, workflow names such as `discover`, `change`, `release`, `publish`, and `affected` come from `[cli.*]` tables in `monochange.toml`; hardcoded binary commands such as `validate`, `check`, `init`, and `mcp` stay built in. The underlying built-in steps are always available directly as immutable `mc step:*` commands.

| Goal                             | Command                                                     | Use it when                                                                                              |
| -------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Validate config and changesets   | `mc validate`                                               | You changed `monochange.toml` or `.changeset/*.md` files                                                 |
| Inspect package ids and groups   | `mc discover --format json`                                 | You need the normalized workspace model                                                                  |
| Create release intent            | `mc change --package <id> --bump <severity> --reason "..."` | You need a new `.changeset/*.md` file                                                                    |
| Audit pending release context    | `mc diagnostics --format json`                              | You need git provenance, PR/MR links, or related issues                                                  |
| Preview the release plan         | `mc release --dry-run --diff`                               | You want changelog/version patches without mutating the repo                                             |
| Create a durable release commit  | `mc commit-release`                                         | You want a monochange-managed release commit with an embedded `ReleaseRecord`                            |
| Open or update a release request | `mc release-pr`                                             | You want a long-lived release PR/MR branch updated from current release state                            |
| Inspect a past release commit    | `mc release-record --from <ref>`                            | You need the durable release declaration from git history                                                |
| Check package publish readiness  | `mc publish-readiness --from HEAD --output <path>`          | You need a validated readiness artifact before package publication                                       |
| Plan ready package publishing    | `mc publish-plan --readiness <path>`                        | You want rate-limit batches that exclude non-ready package work                                          |
| Publish packages to registries   | `mc publish --readiness <path> --output <path>`             | You want `cargo publish`, `npm publish`, `deno publish`, or `dart pub publish` style package publication |
| Bootstrap release packages       | `mc publish-bootstrap --from HEAD --output <path>`          | You need a release-record-scoped placeholder bootstrap artifact before rerunning readiness               |
| Create post-merge release tags   | `mc tag-release --from HEAD`                                | You merged a monochange release commit and now need to create and push its declared tag set              |
| Repair a recent release          | `mc repair-release --from <tag> --target <commit>`          | You need to retarget a just-created release to a later commit                                            |
| Publish hosted/provider releases | `mc publish-release`                                        | You want GitHub/GitLab/Gitea release objects from prepared release state                                 |

<!-- {/projectCommandAutomationMatrix} -->

A practical rule of thumb:

- use **`mc publish-readiness`** and **`mc publish --readiness <path>`** for registry packages
- use **`mc publish-release`** for hosted releases from prepared release state
- use **`mc release-pr`** when you want a provider-backed release request branch
- use **`mc commit-release`** when you want a durable local release commit in git history
- use **`mc tag-release`** when that durable release commit has merged and you want to create its tag set on the default branch

## The three automation layers

monochange has three related but different automation layers:

1. **Release planning** — `mc release --dry-run`, `mc release`, `mc diagnostics`
2. **Package registries** — `mc publish-readiness`, `mc publish-bootstrap --from HEAD`, `mc publish-plan --readiness <path>`, `mc publish --readiness <path>`, and lower-level `mc placeholder-publish`
3. **Hosted providers** — `mc release-pr`, `mc publish-release`, `mc repair-release`

Keeping those layers separate is important. Package publication and hosted-release publication are not the same job.

## Registry and provider capability snapshot

<!-- {=projectCapabilityMatrix} -->

| Capability                                                               | Current status                                                                                |
| ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------- |
| Multi-ecosystem discovery                                                | Cargo, npm/pnpm/Bun, Deno, Dart, Flutter, Python                                              |
| Package release planning                                                 | Built in                                                                                      |
| Grouped/shared versioning                                                | Built in                                                                                      |
| Dry-run release diff previews                                            | Built in via `mc release --dry-run --diff`                                                    |
| Durable release history and post-merge tagging                           | Built in via `ReleaseRecord`, `mc release-record`, `mc tag-release`, and `mc repair-release`  |
| Hosted provider releases                                                 | GitHub, GitLab, Gitea                                                                         |
| Hosted release requests                                                  | GitHub, GitLab, Gitea                                                                         |
| Python release planning                                                  | Built in for discovery, version rewrites, dependency rewrites, and lockfile command inference |
| Built-in registry publishing                                             | `crates.io`, `npm`, `jsr`, `pub.dev`; use external mode for PyPI and custom registries        |
| GitHub npm trusted-publishing automation                                 | Built in                                                                                      |
| GitHub trusted-publishing guidance for `crates.io`, `jsr`, and `pub.dev` | Built in, but manual registry enrollment is still required                                    |
| GitLab trusted-publishing auto-derivation                                | Not built in today                                                                            |
| Release-retarget sync for hosted releases                                | GitHub first                                                                                  |

<!-- {/projectCapabilityMatrix} -->

## CI setup assumption

The workflow sketches below assume the job already has:

- the `monochange` CLI available as `mc`
- the native ecosystem toolchain it needs (`npm`/`pnpm`, `cargo`, `deno`, `dart`, `flutter`, `uv`, `poetry`, or your Python publishing tool)
- repository checkout with enough history for release-record inspection

In the monochange repository itself, that usually means entering the `devenv` shell. In other repositories, it may mean installing `@monochange/cli` or `monochange` explicitly before the publish step.

## GitHub flows

### Common GitHub shape

For GitHub Actions, the most common structure is:

1. a workflow prepares or updates a release PR branch
2. a release commit lands on `main`
3. a post-merge workflow detects the release commit
4. that workflow creates the declared tags and publishes packages from the durable release commit
5. hosted release objects or extra assets come either from downstream tag-driven workflows or from a separate workflow that still uses `mc publish-release`

The important current implementation detail is that `mc publish-readiness` can write a readiness artifact from the `ReleaseRecord` on `HEAD`, `mc publish-bootstrap --from HEAD --output <path>` can run release-record-scoped first-time placeholder setup and record the result, `mc publish --readiness <path>` can validate readiness before package registry mutation, `mc tag-release` can create the declared release tags from that same durable record, and `mc publish-release` still works from prepared release state when you want a manifest-driven hosted-release job.

If the same post-merge job is responsible for both tags and package publication, run `mc tag-release --from HEAD` immediately after release-commit detection, then run `mc publish-readiness --from HEAD --output <path>`, use `mc publish-bootstrap --from HEAD --output <path>` only when first-time package setup is required, optionally inspect `mc publish-plan --readiness <path>`, and finally run `mc publish --readiness <path> --output .monochange/publish-result.json`. If a registry command fails after some packages were published, fix the cause and rerun `mc publish --readiness <path> --resume .monochange/publish-result.json --output .monochange/publish-result.json`; monochange skips completed package versions from the previous result and retries the remaining release work.

### GitHub + npm trusted publishing

Config:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.npm.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
```

Workflow sketch:

```yaml
name: publish-npm

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
      - name: checkout
        uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - name: setup repo tooling
        uses: ./.github/actions/devenv
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

      - name: publish npm packages
        run: |
          devenv shell -- mc publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- mc publish --readiness .monochange/readiness.json
```

What monochange does here:

- resolves the GitHub workflow context
- checks current npm trust configuration
- runs `npm trust github ...` when trust is missing
- uses `pnpm exec npm trust ...` in pnpm workspaces
- verifies the trust result after configuration

Use this when you want the most automated trusted-publishing path monochange currently supports.

### GitHub + Cargo (`crates.io`) trusted publishing

Config for monochange-managed release planning:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.cargo.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
```

Monochange-oriented post-merge workflow sketch:

```yaml
name: publish-cargo

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

      - name: publish Cargo packages
        run: |
          devenv shell -- mc publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- mc publish --readiness .monochange/readiness.json
```

More copy-pasteable registry-native example:

If you want to follow the crates.io documentation more literally, let the official auth action own the token exchange and keep monochange focused on release planning. In that case, prefer `mode = "external"` for Cargo publication.

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.cargo.publish]
enabled = true
mode = "external"
trusted_publishing = true
```

```yaml
name: publish-cargo

on:
  push:
    tags:
      - "v*"

jobs:
  publish:
    runs-on: ubuntu-latest
    environment: release
    permissions:
      contents: read
      id-token: write
    steps:
      - uses: actions/checkout@v6
      - uses: rust-lang/crates-io-auth-action@v1
        id: auth
      - run: cargo publish --package my_crate
        env:
          CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
```

For monorepos with multiple Cargo packages, split this into one job per published crate or have an external script decide which crates should publish for the current tag. For a broader decision guide across built-in and external multi-package flows, see [Multi-package publishing patterns](./14-multi-package-publishing.md).

Important current behavior:

- monochange can carry the trust expectation in config
- monochange can report the setup URL and enforce that trust is configured before built-in release publishing continues
- for built-in crates.io publishing, `mc publish-readiness` now blocks packages whose current `Cargo.toml` cannot be published: `publish = false`, `publish = [...]` without `crates-io`, missing `description`, or missing both `license` and `license-file`
- workspace-inherited Cargo metadata such as `description = { workspace = true }` and `license = { workspace = true }` is accepted when `[workspace.package]` supplies the value
- already-published Cargo versions remain non-blocking and are skipped when current readiness and the saved readiness artifact agree
- monochange does **not** currently auto-configure `crates.io` trust the way it can for npm on GitHub
- if you want the most literal crates.io/OIDC workflow today, `mode = "external"` plus `rust-lang/crates-io-auth-action@v1` is the clearest path

Recommended setup:

1. configure `trusted_publishing = true`
2. bootstrap missing release packages with `mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json` if needed, then rerun readiness
3. manually enroll the repository/workflow in `crates.io`
4. choose either:
   - `mode = "builtin"` and let monochange own the publish command, or
   - `mode = "external"` and use the official crates.io auth action directly

### GitHub + Deno / JSR trusted publishing

Config:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.deno.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
registry = "jsr"
```

Workflow sketch:

```yaml
name: publish-jsr

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

      - name: publish JSR packages
        run: |
          devenv shell -- mc publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- mc publish --readiness .monochange/readiness.json
```

Current behavior matches Cargo more than npm:

- monochange can validate the trust expectation and report the setup URL
- monochange does **not** auto-configure JSR trust on GitHub for you today
- manual registry enrollment is still required before the built-in publish can proceed

### GitHub + Dart / Flutter (`pub.dev`) trusted publishing

Config for monochange-managed release planning:

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.dart.publish]
enabled = true
mode = "builtin"
trusted_publishing = true
registry = "pub.dev"
```

Monochange-oriented post-merge workflow sketch:

```yaml
name: publish-pub-dev

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

      - name: publish pub.dev packages
        run: |
          devenv shell -- mc publish-readiness --from HEAD --output .monochange/readiness.json
          devenv shell -- mc publish --readiness .monochange/readiness.json
```

More copy-pasteable registry-native example:

If you want the workflow shape recommended by the Dart team, prefer the reusable workflow from `dart-lang/setup-dart` and keep monochange focused on release planning. In that case, `mode = "external"` is usually the clearest fit.

```toml
[source]
provider = "github"
owner = "owner"
repo = "repo"

[ecosystems.dart.publish]
enabled = true
mode = "external"
trusted_publishing = true
registry = "pub.dev"
```

```yaml
name: publish-pub-dev

on:
  push:
    tags:
      - "my_package-v[0-9]+.[0-9]+.[0-9]+"

jobs:
  publish:
    permissions:
      id-token: write
    uses: dart-lang/setup-dart/.github/workflows/publish.yml@v1
    with:
      working-directory: packages/my_package
      # environment: pub.dev
```

If you need custom generation or build steps before publishing, switch to a custom workflow that runs `dart pub publish --force` or `flutter pub publish --force` after the OIDC-authenticated setup. For monorepos that mix package-specific tags, working directories, and external-mode jobs, see [Multi-package publishing patterns](./14-multi-package-publishing.md).

Current behavior:

- monochange can enforce the configured trust expectation
- monochange reports the manual setup URL when trust is not configured
- monochange does **not** auto-configure `pub.dev` trusted publishing today
- if you want the most copy-pasteable pub.dev flow today, `mode = "external"` plus the reusable `dart-lang/setup-dart` workflow is the clearest path

### GitHub post-merge package publish flow

If you want package publication to happen **after** the release PR merges, the simplest current pattern is:

1. merge the release PR so the monochange release commit lands on `main`
2. run `mc release-record --from HEAD --format json` in CI
3. if the command succeeds, run `mc publish-readiness --from HEAD --output .monochange/readiness.json`
4. run `mc publish --readiness .monochange/readiness.json` only after readiness succeeds
5. if release-record detection or readiness fails, exit early before registry mutation

That pattern works well because `mc publish-readiness` and `mc publish --readiness` consume the durable `ReleaseRecord` from `HEAD` and validate the same package set before publishing.

## GitLab flows

### Current GitLab reality

GitLab is a supported source provider for hosted releases and release requests.

For package publishing, monochange can still run built-in package publication commands from GitLab CI, but the trust auto-derivation and npm `trust github` automation are GitHub-specific today.

That means the practical GitLab pattern is:

- keep `mode = "builtin"` when monochange's package publish command already matches what you need
- keep `trusted_publishing = false` unless the registry workflow is one you manage externally
- use CI secrets or external publishing logic when the registry requires a setup monochange does not automate on GitLab

### GitLab + npm

Config:

```toml
[ecosystems.npm.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
```

Workflow sketch:

```yaml
publish_npm:
  image: node:22
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - corepack enable
    - git fetch --force --tags origin
    - |
        set -euo pipefail
        if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
          mc tag-release --from HEAD
          mc publish-readiness --from HEAD --output .monochange/readiness.json
          mc publish --readiness .monochange/readiness.json
        else
          echo "not a release commit"
        fi
```

If your npm flow needs registry-token setup or a custom `.npmrc`, do that in CI before running `mc publish-readiness` and `mc publish --readiness`.

### GitLab + Cargo

Config:

```toml
[ecosystems.cargo.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
```

Workflow sketch:

```yaml
publish_cargo:
  image: rust:1.90
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
        set -euo pipefail
        if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
          mc tag-release --from HEAD
          mc publish-readiness --from HEAD --output .monochange/readiness.json
          mc publish --readiness .monochange/readiness.json
        else
          echo "not a release commit"
        fi
```

If you need a crates.io token or a more customized release process, inject the credential in GitLab CI or switch the package to `mode = "external"`.

### GitLab + Deno / JSR

Config:

```toml
[ecosystems.deno.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
registry = "jsr"
```

Workflow sketch:

```yaml
publish_jsr:
  image: denoland/deno:latest
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
        set -euo pipefail
        if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
          mc tag-release --from HEAD
          mc publish-readiness --from HEAD --output .monochange/readiness.json
          mc publish --readiness .monochange/readiness.json
        else
          echo "not a release commit"
        fi
```

If your JSR auth bootstrap is more specialized than the built-in path expects, prefer `mode = "external"` and run the native publish command yourself.

### GitLab + Dart / Flutter

Config:

```toml
[ecosystems.dart.publish]
enabled = true
mode = "builtin"
trusted_publishing = false
registry = "pub.dev"
```

Workflow sketch:

```yaml
publish_pub_dev:
  image: dart:stable
  stage: publish
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
        set -euo pipefail
        if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
          mc tag-release --from HEAD
          mc publish-readiness --from HEAD --output .monochange/readiness.json
          mc publish --readiness .monochange/readiness.json
        else
          echo "not a release commit"
        fi
```

As with JSR, use `mode = "external"` when you need CI-specific auth or publish orchestration outside monochange's built-in assumptions.

## Long-running release PR branch flow

This is the flow you described:

1. every merge to `main` updates a dedicated release branch and PR
2. that branch contains the prepared release commit and release files
3. the release PR stays open and keeps tracking the latest releasable state
4. when the PR merges, publication happens from that merged release commit

### What monochange supports now

monochange now supports the core post-merge pieces of this shape directly:

- `mc release-pr` can open or update a release request branch from current release state
- `mc commit-release` can create a durable monochange release commit with an embedded `ReleaseRecord`
- `mc release-record --from HEAD` can detect whether the latest commit is a monochange release commit
- `mc tag-release --from HEAD` can create and push the declared tag set from that merged release commit
- `mc publish-readiness` can write a readiness artifact from that same durable record on `HEAD`, and `mc publish --readiness <path>` validates it before publishing package registries

### The important tag semantics

Tags are **not branch-scoped**.

A git tag points at a commit object, not at a branch name.

That means:

- if you create a tag on a release-PR commit, the tag exists immediately even before merge
- if that exact commit is later merged into `main`, the tag still points at the same commit and is now reachable from `main`
- if the release branch is later rebased, force-pushed, or regenerated, the old tag does **not** move automatically

That is why pre-merge tagging on a long-running release PR is usually the wrong move.

### Recommended workflow

For the long-running release PR model, the recommended shape is now:

1. on every push to `main`, run `mc release-pr` to refresh the dedicated release PR branch
2. do **not** create tags on the release PR branch
3. merge the release PR when you are ready
4. on the post-merge workflow, run `mc release-record --from HEAD --format json`
5. if the latest commit is a release commit, run `mc tag-release --from HEAD`
6. after tags exist, run `mc publish-readiness --from HEAD --output <path>` and then `mc publish --readiness <path>` for package registries and let tag-triggered workflows create hosted releases or other downstream assets

That keeps tag creation on the default branch side of the merge, which is much safer than tagging the PR branch early.

### GitHub Actions reference sketch

```yaml
name: release

on:
  push:
    branches: [main]

jobs:
  release:
    runs-on: ubuntu-latest
    permissions:
      contents: write
      pull-requests: write
      id-token: write
    steps:
      - uses: actions/checkout@v6
        with:
          fetch-depth: 0

      - name: fetch tags
        run: git fetch --force --tags origin

      - name: detect merged release commit
        id: release_record
        shell: bash
        run: |
          set -euo pipefail
          if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
            echo "is_release_commit=true" >> "$GITHUB_OUTPUT"
          else
            echo "is_release_commit=false" >> "$GITHUB_OUTPUT"
          fi

      - name: refresh release PR
        if: steps.release_record.outputs.is_release_commit != 'true'
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        run: mc release-pr

      - name: create release tags
        if: steps.release_record.outputs.is_release_commit == 'true'
        run: mc tag-release --from HEAD

      - name: publish packages
        if: steps.release_record.outputs.is_release_commit == 'true'
        run: |
          mc publish-readiness --from HEAD --output .monochange/readiness.json
          mc publish --readiness .monochange/readiness.json
```

### GitLab CI reference sketch

```yaml
release_pr_or_publish:
  stage: release
  rules:
    - if: "$CI_COMMIT_BRANCH == $CI_DEFAULT_BRANCH"
  script:
    - git fetch --force --tags origin
    - |
        set -euo pipefail
        if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then
          mc tag-release --from HEAD
          mc publish-readiness --from HEAD --output .monochange/readiness.json
          mc publish --readiness .monochange/readiness.json
        else
          mc release-pr
        fi
```

## Choosing a CI pattern

Use this decision rule:

- **Need human review before release files land?** → use `mc release-pr`
- **Need a durable local release commit?** → use `mc commit-release`
- **Need package registries after merge?** → detect `ReleaseRecord` on `HEAD`, run `mc tag-release --from HEAD`, then run `mc publish-readiness --from HEAD --output <path>` and `mc publish --readiness <path>`
- **Need hosted provider releases from prepared release state?** → use `mc publish-release`
- **Need to bootstrap release packages that do not exist yet?** → use `mc publish-bootstrap --from HEAD --output <path>`; reserve names outside a release with lower-level `mc placeholder-publish`
- **Need GitHub npm trusted publishing with the least custom glue?** → use `trusted_publishing = true` with `mc publish-readiness` and `mc publish --readiness <path>`
- **Need GitLab CI with custom auth/bootstrap?** → keep `mode = "external"` as the escape hatch

## Related guides

- [GitHub automation](./08-github-automation.md)
- [Configuration reference](./04-configuration.md)
- [Release planning](./06-release-planning.md)
- [Repairable releases](./12-repairable-releases.md)
