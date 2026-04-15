# Advanced: CI, package publishing, and release PR flows

This guide brings together the practical CI patterns around `mc publish`, `mc placeholder-publish`, `mc release-pr`, `mc commit-release`, and provider release automation.

It also captures an important proposed workflow for long-running release PR branches.

## Start with the command surface

These commands solve different automation problems:

<!-- {=projectCommandAutomationMatrix} -->

| Goal                             | Command                                                     | Use it when                                                                                              |
| -------------------------------- | ----------------------------------------------------------- | -------------------------------------------------------------------------------------------------------- |
| Validate config and changesets   | `mc validate`                                               | You changed `monochange.toml` or `.changeset/*.md` files                                                 |
| Inspect package ids and groups   | `mc discover --format json`                                 | You need the normalized workspace model                                                                  |
| Create release intent            | `mc change --package <id> --bump <severity> --reason "..."` | You need a new `.changeset/*.md` file                                                                    |
| Audit pending release context    | `mc diagnostics --format json`                              | You need git provenance, PR/MR links, or related issues                                                  |
| Preview the release plan         | `mc release --dry-run --diff`                               | You want changelog/version patches without mutating the repo                                             |
| Create a durable release commit  | `mc commit-release`                                         | You want a monochange-managed release commit with an embedded `ReleaseRecord`                            |
| Open or update a release request | `mc release-pr`                                             | You want a long-lived release PR/MR branch updated from current release state                            |
| Publish packages to registries   | `mc publish`                                                | You want `cargo publish`, `npm publish`, `deno publish`, or `dart pub publish` style package publication |
| Bootstrap missing packages       | `mc placeholder-publish`                                    | A package must exist in its registry before later automation can work                                    |
| Inspect a past release commit    | `mc release-record --from <ref>`                            | You need the durable release declaration from git history                                                |
| Repair a recent release          | `mc repair-release --from <tag> --target <commit>`          | You need to retarget a just-created release to a later commit                                            |
| Publish hosted/provider releases | `mc publish-release`                                        | You want GitHub/GitLab/Gitea release objects from prepared release state                                 |

<!-- {/projectCommandAutomationMatrix} -->

A practical rule of thumb:

- use **`mc publish`** for registry packages
- use **`mc publish-release`** for hosted releases
- use **`mc release-pr`** when you want a provider-backed release request branch
- use **`mc commit-release`** when you want a durable local release commit in git history

## The three automation layers

monochange has three related but different automation layers:

1. **Release planning** — `mc release --dry-run`, `mc release`, `mc diagnostics`
2. **Package registries** — `mc placeholder-publish`, `mc publish`
3. **Hosted providers** — `mc release-pr`, `mc publish-release`, `mc repair-release`

Keeping those layers separate is important. Package publication and hosted-release publication are not the same job.

## Registry and provider capability snapshot

<!-- {=projectCapabilityMatrix} -->

| Capability                                                               | Current status                                                             |
| ------------------------------------------------------------------------ | -------------------------------------------------------------------------- |
| Multi-ecosystem discovery                                                | Cargo, npm/pnpm/Bun, Deno, Dart, Flutter                                   |
| Package release planning                                                 | Built in                                                                   |
| Grouped/shared versioning                                                | Built in                                                                   |
| Dry-run release diff previews                                            | Built in via `mc release --dry-run --diff`                                 |
| Durable release history                                                  | Built in via `ReleaseRecord`, `mc release-record`, and `mc repair-release` |
| Hosted provider releases                                                 | GitHub, GitLab, Gitea                                                      |
| Hosted release requests                                                  | GitHub, GitLab, Gitea                                                      |
| Built-in registry publishing                                             | `crates.io`, `npm`, `jsr`, `pub.dev`                                       |
| GitHub npm trusted-publishing automation                                 | Built in                                                                   |
| GitHub trusted-publishing guidance for `crates.io`, `jsr`, and `pub.dev` | Built in, but manual registry enrollment is still required                 |
| GitLab trusted-publishing auto-derivation                                | Not built in today                                                         |
| Release-retarget sync for hosted releases                                | GitHub first                                                               |

<!-- {/projectCapabilityMatrix} -->

## CI setup assumption

The workflow sketches below assume the job already has:

- the `monochange` CLI available as `mc`
- the native ecosystem toolchain it needs (`npm`/`pnpm`, `cargo`, `deno`, `dart`, or `flutter`)
- repository checkout with enough history for release-record inspection

In the monochange repository itself, that usually means entering the `devenv` shell. In other repositories, it may mean installing `@monochange/cli` or `monochange` explicitly before the publish step.

## GitHub flows

### Common GitHub shape

For GitHub Actions, the most common structure is:

1. a workflow prepares or updates a release PR branch
2. a release commit lands on `main`
3. a post-merge workflow detects the release commit
4. package publication runs from that durable release commit
5. provider release and tag automation runs in a dedicated release workflow

The important current implementation detail is that `mc publish` can publish from the `ReleaseRecord` on `HEAD`, while `mc publish-release` still works from prepared release state.

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
        run: devenv shell -- mc publish
```

What monochange does here:

- resolves the GitHub workflow context
- checks current npm trust configuration
- runs `npm trust github ...` when trust is missing
- uses `pnpm exec npm trust ...` in pnpm workspaces
- verifies the trust result after configuration

Use this when you want the most automated trusted-publishing path monochange currently supports.

### GitHub + Cargo (`crates.io`) trusted publishing

Config:

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

Workflow sketch:

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
        run: devenv shell -- mc publish
```

Important current behavior:

- monochange can carry the trust expectation in config
- monochange can report the setup URL and enforce that trust is configured before built-in release publishing continues
- monochange does **not** currently auto-configure `crates.io` trust the way it can for npm on GitHub

Recommended setup:

1. configure `trusted_publishing = true`
2. bootstrap missing packages with `mc placeholder-publish` if needed
3. manually enroll the repository/workflow in `crates.io`
4. let `mc publish` perform the actual publish after the release commit lands

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
        run: devenv shell -- mc publish
```

Current behavior matches Cargo more than npm:

- monochange can validate the trust expectation and report the setup URL
- monochange does **not** auto-configure JSR trust on GitHub for you today
- manual registry enrollment is still required before the built-in publish can proceed

### GitHub + Dart / Flutter (`pub.dev`) trusted publishing

Config:

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

Workflow sketch:

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
        run: devenv shell -- mc publish
```

Current behavior:

- monochange can enforce the configured trust expectation
- monochange reports the manual setup URL when trust is not configured
- monochange does **not** auto-configure `pub.dev` trusted publishing today

### GitHub post-merge package publish flow

If you want package publication to happen **after** the release PR merges, the simplest current pattern is:

1. merge the release PR so the monochange release commit lands on `main`
2. run `mc release-record --from HEAD --format json` in CI
3. if the command succeeds, run `mc publish`
4. if it fails, exit early because HEAD is not a release commit

That pattern works well because `mc publish` can consume the durable `ReleaseRecord` from `HEAD`.

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
    - if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then mc publish; else echo "not a release commit"; fi
```

If your npm flow needs registry-token setup or a custom `.npmrc`, do that in CI before running `mc publish`.

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
    - if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then mc publish; else echo "not a release commit"; fi
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
    - if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then mc publish; else echo "not a release commit"; fi
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
    - if mc release-record --from HEAD --format json >/tmp/release-record.json 2>/dev/null; then mc publish; else echo "not a release commit"; fi
```

As with JSR, use `mode = "external"` when you need CI-specific auth or publish orchestration outside monochange's built-in assumptions.

## Long-running release PR branch flow

This is the flow you described:

1. every merge to `main` updates a dedicated release branch and PR
2. that branch contains the prepared release commit and release files
3. the release PR stays open and keeps tracking the latest releasable state
4. when the PR merges, publication happens from that merged release commit

### What monochange supports today

monochange already supports a large part of this shape:

- `mc release-pr` can open or update a release request branch from current release state
- `mc commit-release` can create a durable monochange release commit with an embedded `ReleaseRecord`
- `mc release-record --from HEAD` can detect whether the latest commit is a monochange release commit
- `mc publish` can publish package registries from that durable record on `HEAD`

### The important tag semantics

Tags are **not branch-scoped**.

A git tag points at a commit object, not at a branch name.

That means:

- if you create a tag on a release-PR commit, the tag exists immediately even before merge
- if that exact commit is later merged into `main`, the tag still points at the same commit and is now reachable from `main`
- if the release branch is later rebased, force-pushed, or regenerated, the old tag does **not** move automatically

That is why pre-merge tagging on a long-running release PR is usually the wrong move.

### Recommended current approach

For the long-running release PR model, the safest current shape is:

1. on every push to `main`, run `mc release-pr` to refresh the dedicated release PR branch
2. do **not** create tags on the release PR branch
3. merge the release PR when you are ready
4. on the post-merge workflow, run `mc release-record --from HEAD --format json`
5. if the latest commit is a release commit, run `mc publish` for package registries
6. handle tag creation and hosted-release publication in a dedicated follow-up workflow

### Captured future workflow

The missing piece for a fully first-class version of this pattern is a built-in post-merge tagger.

The strongest future shape would be:

1. `mc release-pr` keeps the long-running release PR branch up to date
2. the release PR merges into `main`
3. a post-merge monochange command inspects `ReleaseRecord` on `HEAD`
4. that command creates and pushes the declared tag set
5. tag-triggered workflows publish hosted releases and any additional downstream assets

That would keep tag creation on the default branch side of the merge, which is much safer than tagging the PR branch early.

### Captured implementation gap

Today monochange does **not** ship a dedicated top-level command that says "read the `ReleaseRecord` on `HEAD` and create/push the release tags now".

That gap is important to capture because it affects exactly this release-PR-merge flow.

A future enhancement in this area would likely include:

- a built-in post-merge release-commit detector and tagger
- a smoother first-class long-running release PR workflow
- branch-refresh semantics that make repeated release-branch updates more explicit
- possibly force-refresh behavior for workflows that intentionally rewrite the dedicated release branch over time

## Choosing a CI pattern

Use this decision rule:

- **Need human review before release files land?** → use `mc release-pr`
- **Need a durable local release commit?** → use `mc commit-release`
- **Need package registries after merge?** → detect `ReleaseRecord` on `HEAD`, then run `mc publish`
- **Need hosted provider releases from prepared release state?** → use `mc publish-release`
- **Need to bootstrap a package that does not exist yet?** → use `mc placeholder-publish`
- **Need GitHub npm trusted publishing with the least custom glue?** → use `trusted_publishing = true` with `mc publish`
- **Need GitLab CI with custom auth/bootstrap?** → keep `mode = "external"` as the escape hatch

## Related guides

- [GitHub automation](./08-github-automation.md)
- [Configuration reference](./04-configuration.md)
- [Release planning](./06-release-planning.md)
- [Repairable releases](./12-repairable-releases.md)
