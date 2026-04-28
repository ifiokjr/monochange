---
monochange: patch
monochange_python: patch
"@monochange/skill": patch
---

#### Document supported ecosystem capabilities

The documentation now includes a dedicated ecosystem guide that compares Cargo, npm-family, Deno, Dart / Flutter, and Python support across discovery, manifest updates, lockfile handling, and built-in registry publishing. Python is documented as a supported release-planning ecosystem with uv workspace discovery, Poetry and PEP 621 `pyproject.toml` parsing, Python dependency normalization, manifest version rewrites, internal dependency rewrites, and inferred `uv lock` / `poetry lock --no-update` lockfile commands.

The guide also clarifies the current publishing boundary: Python packages participate in monochange release planning, but PyPI publication remains external for now and should use `mode = "external"` or custom CI until built-in PyPI publishing exists.
