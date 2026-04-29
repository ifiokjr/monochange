## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-29)

### Changed

#### Document supported ecosystem capabilities

The documentation now includes a dedicated ecosystem guide that compares Cargo, npm-family, Deno, Dart / Flutter, and Python support across discovery, manifest updates, lockfile handling, and built-in registry publishing. Python is documented as a supported release-planning ecosystem with uv workspace discovery, Poetry and PEP 621 `pyproject.toml` parsing, Python dependency normalization, manifest version rewrites, internal dependency rewrites, and inferred `uv lock` / `poetry lock --no-update` lockfile commands.

The guide also clarifies ecosystem publishing boundaries, including canonical public registry support and the external-mode escape hatch for private registries or custom publication flows.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #307](https://github.com/monochange/monochange/pull/307) _Introduced in:_ [`11c628c`](https://github.com/monochange/monochange/commit/11c628cd2afb7c9509c31a8cfc043be63a9f2a75) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67)

#### Add Python ecosystem support

monochange now discovers and manages Python packages from uv workspaces, Poetry projects, and standalone `pyproject.toml` files.

**Configuration:**

```toml
[defaults]
package_type = "python"

[package.core]
path = "packages/core"

[ecosystems.python]
enabled = true
lockfile_commands = [{ command = "uv lock" }]
```

**What it discovers:**

- uv workspaces via `[tool.uv.workspace].members` glob patterns
- Poetry projects via `[tool.poetry]` sections
- Standalone `pyproject.toml` files with PEP 621 `[project]` metadata

**Version management:**

- Reads and updates `[project].version` in `pyproject.toml`
- Parses PEP 440 versions and maps to semver (e.g., `1.2` → `1.2.0`)
- Updates dependency version constraints in `[project].dependencies`
- Handles `dynamic = ["version"]` gracefully (reports `None` for dynamic versions)

**Lockfile commands:**

- Infers `uv lock` for uv projects (detected by `uv.lock`)
- Infers `poetry lock --no-update` for Poetry projects (detected by `poetry.lock`)
- Configurable via `[ecosystems.python].lockfile_commands`

**Dependency extraction:**

- PEP 621 `[project].dependencies` and `[project.optional-dependencies]`
- Poetry `[tool.poetry.dependencies]` and `[tool.poetry.group.*.dependencies]`
- PEP 503 name normalization for cross-package dependency matching
- PEP 508 version specifier parsing with extras support

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #152](https://github.com/monochange/monochange/pull/152) _Introduced in:_ [`81b0882`](https://github.com/monochange/monochange/commit/81b0882525ab51d74b0e8cc2a0114aac0fdb3a7f) _Last updated in:_ [`b33a82d`](https://github.com/monochange/monochange/commit/b33a82d8e26da20fb2dfbb94bc5f4040c27f2c67) _Closed issues:_ [#132](https://github.com/monochange/monochange/issues/132)
