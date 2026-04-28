# Ecosystems

monochange uses ecosystem adapters to translate native package-manager files into one release-planning model. Each adapter answers the same questions:

- which package manifests exist in the repository?
- which packages depend on other packages?
- where should a package version and internal dependency references be rewritten during a release?
- should lockfiles be rewritten directly, refreshed with a command, or left to external tooling?
- can monochange publish the package directly, or should publication stay external?

## Capability matrix

| Ecosystem      | Package type      | Discovery sources                                                                       | Version and dependency updates                                                             | Lockfile behavior                                                                                                                                       | Built-in registry publishing |
| -------------- | ----------------- | --------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------- |
| Cargo          | `cargo`           | `Cargo.toml` workspaces and standalone crates                                           | `Cargo.toml` package versions and internal dependency requirements                         | Direct `Cargo.lock` rewrite by default; configure `cargo generate-lockfile`, `cargo check`, or another command when you need package-manager resolution | `crates.io`                  |
| npm-family     | `npm`             | npm workspaces, pnpm workspaces, Bun workspaces, and standalone `package.json` packages | `package.json` versions and dependency ranges                                              | Direct `package-lock.json`, `pnpm-lock.yaml`, `bun.lock`, and `bun.lockb` updates by default; command overrides support package-manager refreshes       | `npm`                        |
| Deno           | `deno`            | Deno workspaces and standalone `deno.json` / `deno.jsonc` packages                      | Deno manifest versions, exports/imports metadata, and dependency references                | Direct `deno.lock` update when possible; no inferred lockfile command                                                                                   | `jsr`                        |
| Dart / Flutter | `dart`, `flutter` | Dart and Flutter workspaces plus standalone `pubspec.yaml` packages                     | `pubspec.yaml` versions and dependency ranges                                              | Direct `pubspec.lock` update by default; configure `dart pub get` or `flutter pub get` when you need full solver refreshes                              | `pub.dev`                    |
| Python         | `python`          | uv workspaces, Poetry projects, and standalone `pyproject.toml` packages                | PEP 621 `[project]` and Poetry `[tool.poetry]` package versions plus dependency specifiers | Does not mutate `uv.lock` or `poetry.lock` directly; infers `uv lock` and `poetry lock --no-update` commands; unknown Python lockfiles are skipped      | `pypi`                       |

The built-in publishing column is intentionally narrower than release planning. It lists only the canonical public registry for each supported ecosystem; private registries and custom publication flows should use `mode = "external"`.

## Shared behavior across ecosystems

All supported ecosystems feed the same planner. After discovery, monochange can:

- render package ids and manifest paths relative to the repository root
- normalize dependency edges into one graph
- apply `[group.<id>]` version synchronization rules
- propagate dependent bumps through internal dependency edges
- update native manifests during `mc release`
- update extra `versioned_files` entries, including regex-managed files
- render changelogs and release notes from `.changeset/*.md`
- create durable release records and post-merge tags

`[ecosystems.<name>]` configuration currently controls settings such as dependency-version prefixes, extra versioned files, publish defaults, and lockfile commands. Discovery still scans every supported ecosystem regardless of `[ecosystems.*].enabled`, `roots`, or `exclude` toggles.

## Cargo

Cargo support is designed for Rust crates that keep version data in `Cargo.toml` and dependency resolution in `Cargo.lock`.

Use Cargo support when your repository has:

- a root Cargo workspace with `members`
- standalone crates outside a workspace
- internal crate dependencies that should move together when one crate is released
- crates published to `crates.io`

Cargo-specific behavior:

- package ids come from each crate manifest
- dependency references use Cargo's native requirement style; the default dependency version prefix is empty
- `Cargo.lock` is updated directly by default for fast release preparation
- incomplete or complex lockfile cases can be delegated to explicit lockfile commands
- built-in publishing targets `crates.io`
- publish readiness validates common crates.io requirements, including `publish`, `description`, and license metadata

## npm, pnpm, and Bun

The npm-family adapter covers JavaScript and TypeScript packages that share `package.json` as their manifest format.

Use npm-family support when your repository has:

- npm workspaces declared in `package.json`
- pnpm workspaces declared in `pnpm-workspace.yaml`
- Bun workspaces and Bun lockfiles
- standalone `package.json` packages
- internal workspace dependencies that use npm-compatible version ranges

npm-family behavior:

- package ids come from `package.json` names
- internal dependency ranges default to the `^` prefix
- `dependencies`, `devDependencies`, and `peerDependencies` participate in dependency updates
- direct lockfile support covers `package-lock.json`, `pnpm-lock.yaml`, `bun.lock`, and `bun.lockb`
- built-in publishing targets the public `npm` registry
- GitHub npm trusted-publishing automation is built in; pnpm workspaces use pnpm-compatible trust and publish commands

## Deno

Deno support is for packages described by `deno.json` or `deno.jsonc`, including workspaces that publish to JSR.

Use Deno support when your repository has:

- `deno.json` or `deno.jsonc` package manifests
- Deno workspace members
- `imports` entries that connect internal packages
- packages published to `jsr`

Deno behavior:

- internal dependency ranges default to `^`
- `imports` are the primary dependency field
- `deno.lock` can be updated directly when present
- monochange does not infer a default Deno lockfile command; configure one if your release flow needs `deno cache`, `deno task`, or another resolver step
- built-in publishing targets `jsr`

## Dart and Flutter

Dart and Flutter share the Dart ecosystem settings because both use `pubspec.yaml` and Pub's version constraints.

Use Dart / Flutter support when your repository has:

- pure Dart packages
- Flutter packages with a `flutter` section
- Dart or Flutter workspace layouts
- packages published to `pub.dev`

Dart / Flutter behavior:

- package type is `dart` for pure Dart packages and `flutter` for Flutter packages
- internal dependency ranges default to `^`
- `dependencies` and `dev_dependencies` participate in dependency updates
- `pubspec.lock` can be rewritten directly by default
- configure `dart pub get` or `flutter pub get` as lockfile commands when you need the Pub solver to refresh files instead of the direct updater
- built-in publishing targets `pub.dev`

## Python

Python support is centered on `pyproject.toml`. It covers modern PEP 621 projects, Poetry projects, uv workspaces, and standalone packages discovered by scanning for manifests.

Use Python support when your repository has:

- a uv workspace declared under `[tool.uv.workspace]`
- PEP 621 package metadata under `[project]`
- Poetry package metadata under `[tool.poetry]`
- standalone Python packages with `pyproject.toml`
- internal dependencies that should receive version bumps alongside released workspace packages

Python discovery works in two passes:

1. If the repository root has a `pyproject.toml` with uv workspace members, monochange expands the member globs and reads each member manifest.
2. monochange then scans for standalone `pyproject.toml` files that were not already included by the uv workspace pass.

When a manifest has both PEP 621 and Poetry metadata, monochange prefers `[project]`. If `[project].dynamic` contains `"version"`, monochange treats the package version as dynamic and does not rewrite the version field.

Python version and dependency behavior:

- package names and dependency names are normalized using Python's PEP 503 style normalization for dependency graph matching
- PEP 440 versions are parsed into the shared semantic-version model when possible
- internal dependency ranges default to the `>=` prefix
- PEP 621 `dependencies` are runtime dependencies
- PEP 621 `optional-dependencies` are development/optional dependency edges for release-planning purposes
- Poetry `dependencies` are runtime dependencies, except the special `python` constraint is skipped
- Poetry dependency groups under `[tool.poetry.group.<name>.dependencies]` are development dependencies
- release preparation rewrites `pyproject.toml` package versions and matching dependency specifiers while preserving extras such as `httpx[cli]`

Python lockfile behavior is command-based by design:

- `uv.lock` infers `uv lock`
- `poetry.lock` infers `poetry lock --no-update`
- unknown Python lockfile names are ignored rather than guessed
- configuring `[ecosystems.python].lockfile_commands` overrides the inferred commands

Built-in Python publishing targets PyPI. monochange builds Python artifacts with `uv build --out-dir dist` and publishes them with `uv publish`, using `--trusted-publishing always` when trusted publishing is enabled and `--trusted-publishing never` otherwise. Placeholder publishing creates a minimal Hatchling project with a normalized module directory under `src/`.

Example Python package configuration:

```toml
[package.api]
path = "services/api"
type = "python"
changelog = true

[package.api.publish]
enabled = true
mode = "builtin"
registry = "pypi"
trusted_publishing = true

[ecosystems.python]
dependency_version_prefix = ">="
# Optional: override inferred uv/Poetry lockfile commands.
lockfile_commands = [{ command = "uv lock", cwd = "." }]
```

## Choosing external publishing

Use `mode = "external"` when an ecosystem or registry is not handled by monochange's built-in publisher, or when your organization needs custom signing, provenance, approval, rate-limit, private-registry behavior, or a Python publishing toolchain other than the built-in `uv build` / `uv publish` flow.

That keeps the package in release planning while leaving upload mechanics to your existing publishing workflow.
