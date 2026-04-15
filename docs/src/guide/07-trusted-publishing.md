# Trusted publishing and OIDC

monochange supports built-in package publishing for the canonical public registries of the ecosystems it manages:

- Cargo → `crates.io`
- npm packages → `npm`
- Deno packages → `jsr`
- Dart / Flutter packages → `pub.dev`

For those registries, monochange can also manage or verify **trusted publishing** when the registry supports publishing directly from a verified GitHub Actions identity.

Different registries use different names for the same general pattern:

- **trusted publishing**
- **OIDC publishing**
- **automated publishing**
- **trusted publishers**

The goal is the same in every case:

- publish from CI instead of local machines
- avoid long-lived registry tokens where possible
- restrict publish rights to a specific repository, workflow, and sometimes environment

## What monochange can automate today

| Ecosystem      | Registry  | Registry term             | GitHub-based OIDC available | What monochange can automate                                                          |
| -------------- | --------- | ------------------------- | --------------------------- | ------------------------------------------------------------------------------------- |
| npm            | npm       | Trusted publishing        | Yes                         | Can verify existing trust and run `npm trust github ...` or `pnpm exec npm trust ...` |
| cargo          | crates.io | Trusted Publishing        | Yes                         | Can publish with a temporary token after you finish registry-side setup               |
| deno           | jsr       | GitHub Actions publishing | Yes                         | Reports the setup URL; repository linking is still manual                             |
| dart / flutter | pub.dev   | Automated publishing      | Yes                         | Reports the setup URL; admin-page setup is still manual                               |

npm is currently the only ecosystem where monochange performs bulk trusted-publishing setup itself.

For `crates.io`, `jsr`, and `pub.dev`, monochange reports the setup URL for each package and blocks the next built-in registry publish until the trust configuration has been completed manually.

## monochange configuration

Start by enabling trusted publishing for the relevant ecosystem or package.

```toml
[source]
provider = "github"
owner = "ifiokjr"
repo = "monochange"

[ecosystems.npm.publish]
trusted_publishing = true

[ecosystems.cargo.publish]
trusted_publishing = true

[ecosystems.deno.publish]
trusted_publishing = true

[ecosystems.dart.publish]
trusted_publishing = true

[package.cli.publish.trusted_publishing]
workflow = "publish.yml"
environment = "publisher"
```

monochange resolves the GitHub trust context from:

- `publish.trusted_publishing.repository`
- `publish.trusted_publishing.workflow`
- `publish.trusted_publishing.environment`
- otherwise `[source]`
- otherwise GitHub Actions runtime values such as `GITHUB_REPOSITORY`, `GITHUB_WORKFLOW_REF`, and `GITHUB_JOB`

If your workflow filename or environment cannot be inferred reliably, set them explicitly in `monochange.toml`.

## GitHub Actions baseline

Most registries require the publish job to request an OIDC token.

```yaml
permissions:
  contents: read
  id-token: write
```

If you use a protected deployment environment, keep the workflow and registry settings aligned:

```yaml
jobs:
  publish:
    environment: publisher
```

Use the same environment name in GitHub Actions and in the registry configuration.

## Recommended rollout

Use this sequence when adopting trusted publishing for an existing workspace:

1. Set `publish.trusted_publishing = true` for the target ecosystem or package.
2. Run `mc placeholder-publish --dry-run` to see which packages do not exist yet.
3. If needed, run `mc placeholder-publish` so the package exists in the registry first.
4. Complete the registry-side trusted-publishing setup for each package.
5. Run `mc publish --dry-run` to confirm monochange now sees the expected trust configuration.
6. Publish from CI with `mc publish`.

Placeholder publishing is especially useful when the package name must exist in the registry before trusted publishing can be configured.

## npm

### Registry-side setup

On npm, trusted publishing can be configured from the package settings page or through the CLI.

**UI path**

- `npmjs.com` → package → **Settings** → **Trusted publishing**

**Fields to enter for GitHub Actions**

- **Organization or user** — GitHub owner
- **Repository** — GitHub repository name
- **Workflow filename** — for example `publish.yml`
- **Environment name** — optional, for example `publisher`

Only the workflow filename is required, not the full `.github/workflows/...` path.

### CLI setup commands

These are the same commands monochange models for npm trusted publishing.

List the current trusted publishers for a package:

```bash
npm trust list <package-name> --json
```

Configure a package for a GitHub workflow:

```bash
npm trust github <package-name> \
  --repo owner/repo \
  --file publish.yml \
  --yes
```

Add an environment restriction:

```bash
npm trust github <package-name> \
  --repo owner/repo \
  --file publish.yml \
  --env publisher \
  --yes
```

If the workspace uses pnpm, use the pnpm-wrapped form:

```bash
pnpm exec npm trust github <package-name> \
  --repo owner/repo \
  --file publish.yml \
  --env publisher \
  --yes
```

### Workflow requirements

At minimum, the publish workflow should have:

```yaml
permissions:
  contents: read
  id-token: write
```

### monochange notes

- monochange verifies the current trust configuration first.
- If trust is missing, monochange can run the trust command automatically before `npm publish` or `pnpm publish`.
- If approval is still required in the browser, npm's own flow may still require human confirmation.
- pnpm workspaces stay on `pnpm exec npm trust ...` and `pnpm publish` so workspace protocol and catalog dependency handling stay aligned with the workspace manager.

## crates.io

### Registry-side setup

crates.io supports trusted publishing for GitHub Actions.

**Important:** the crate must already exist on `crates.io` before you can finish trusted-publishing setup. If it does not exist yet, bootstrap it first with a real initial release or `mc placeholder-publish`.

**UI path**

- crate page → **Settings** → **Trusted Publishing**

**Fields to enter for GitHub Actions**

- **Repository owner** — GitHub owner
- **Repository name** — GitHub repository name
- **Workflow filename** — for example `publish.yml`
- **Environment** — optional, for example `publisher`

### Workflow setup

A GitHub Actions job can use the official token-exchange action:

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: actions/checkout@v6

  - uses: rust-lang/crates-io-auth-action@v1
    id: auth

  - run: cargo publish
    env:
      CARGO_REGISTRY_TOKEN: ${{ steps.auth.outputs.token }}
```

### monochange notes

- monochange does not create the `crates.io` trusted-publisher record for you yet.
- Once the registry-side configuration exists, monochange can publish with the temporary token exposed by `rust-lang/crates-io-auth-action@v1`.
- The current monochange GitHub publish workflow already uses this pattern.

Useful references:

- `https://crates.io/docs/trusted-publishing`
- `https://rust-lang.github.io/rfcs/3691-trusted-publishing-cratesio.html`

## jsr

### Registry-side setup

JSR supports tokenless publishing from GitHub Actions.

**Manual setup step**

- go to the package on `jsr.io`
- open **Settings**
- link the package to the GitHub repository that is allowed to publish it

monochange currently reports the package URL and expects this repository-linking step to be completed manually.

### Workflow setup

A minimal GitHub Actions job looks like this:

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: actions/checkout@v6
  - run: npx jsr publish
```

You can also publish with:

```bash
deno publish
```

### monochange notes

- JSR's tokenless publishing is currently GitHub Actions focused.
- Other CI providers still need token-based publishing.
- monochange does not yet link the package to the repository for you.
- If the package does not exist yet, placeholder publishing can bootstrap the registry entry before you finish the repository-link step.

Useful references:

- `https://jsr.io/docs/publishing-packages`
- `https://jsr.io/docs/trust`

## pub.dev

### Registry-side setup

pub.dev calls this **automated publishing**.

**Important:** the package must already exist before you can enable automated publishing. If the package does not exist yet, publish it once first or use `mc placeholder-publish`.

**UI path**

- `https://pub.dev/packages/<package>/admin`
- find the **Automated publishing** section
- click **Enable publishing from GitHub Actions**

**Fields to enter**

- **Repository** — `owner/repo`
- **Tag pattern** — a string containing `{{version}}`

Examples:

- single-package repo: `v{{version}}`
- monorepo package-specific tag: `my_package-v{{version}}`

For a monorepo, give each package its own tag pattern so a tag for one package cannot publish another package by accident.

### Workflow requirements

pub.dev only accepts GitHub Actions automated publishing when the workflow was triggered by a **tag push**.

That means the GitHub workflow trigger must align with the configured tag pattern.

Example tag trigger for `v{{version}}`:

```yaml
on:
  push:
    tags:
      - "v[0-9]+.[0-9]+.[0-9]+"

permissions:
  contents: read
  id-token: write
```

The publish step can then run:

```bash
dart pub publish --force
```

For Flutter packages, the equivalent publish command is:

```bash
flutter pub publish --force
```

### monochange notes

- monochange does not configure pub.dev automated publishing for you yet.
- monochange reports the package admin URL so you can finish the setup manually.
- pub.dev is stricter than the others here because the workflow must be tag-triggered, not just manually dispatched or branch-triggered.
- Keep the Git tag, `pubspec.yaml` version, and tag pattern aligned.

Useful references:

- `https://dart.dev/tools/pub/automated-publishing`
- `https://pub.dev/packages/<package>/admin`

## Mapping monochange config to registry values

Use this cheat sheet when a registry asks for workflow details.

| Registry field                              | Value to use                                                                                                  |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------- |
| repository owner / organization / namespace | GitHub owner from `[source]` or `publish.trusted_publishing.repository`                                       |
| repository name / project                   | repository part of `owner/repo`                                                                               |
| workflow filename                           | `publish.trusted_publishing.workflow`, for example `publish.yml`                                              |
| environment                                 | `publish.trusted_publishing.environment`, for example `publisher`                                             |
| pub.dev tag pattern                         | choose a tag rule that matches your release workflow, for example `v{{version}}` or `my_package-v{{version}}` |

If monochange cannot infer the GitHub repository or workflow for a package, set them explicitly in `monochange.toml`.

## Security recommendations

- prefer trusted publishing over long-lived registry tokens whenever the registry supports it
- keep `id-token: write` only on the publish job instead of the entire workflow when possible
- use a protected GitHub environment such as `publisher` for high-value publish jobs
- restrict tag creation and release workflows to trusted maintainers
- use package-specific tags in monorepos when a registry supports tag-based publish authorization

## When to keep `mode = "external"`

Keep a package on `mode = "external"` when:

- the registry is private or custom
- you need retry or delayed requeue behavior that monochange does not manage yet
- your registry requires a CI pattern that differs substantially from monochange's built-in publish flow

In those cases, you can still use the same registry-side trusted-publishing setup while letting your own workflow own the actual publish command.
