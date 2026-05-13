---
monochange: test
"@monochange/skill": test
---

# add path-filter gate to CI jobs

Introduce a `changes` job using `dorny/paths-filter` so that expensive CI jobs are skipped when a pull request only touches files outside their scope.

**Filters**

- `rust` — `**.rs`, `**/Cargo.toml`, `Cargo.lock`, and other Rust tooling configs.
- `fixtures` — anything under `fixtures/**`, `crates/**/tests/**`, `crates/**/__tests__/**`, or snapshot directories.
- `global` — `.github/workflows/ci.yml`, `monochange.toml`, and `.changeset/**`.
- `js` — JS/TS source files and lockfiles.
- `docs` — `docs/**`, `**.md`, `book.toml`, and `scripts/**`.
- `workflows` — `.github/workflows/**` and `.github/actions/**`.

**Gating rules**

- `merge_group` events always run every job (bypassing the filters).
- `security` runs when `workflows` or `global` files change.
- `lint` runs when `rust`, `js`, `docs`, or `global` files change.
- `test` and `coverage` run when `rust`, `fixtures`, or `global` files change.
- `build` and `benchmark` run when `rust` or `global` files change.
- PR-only jobs (`benchmark-binary`, `release-test`, `release-records`, `release-lint`) also check the relevant filters.
- Main-branch jobs (`release-pr`, `release-post-merge`) are left as-is so release automation is never accidentally skipped.
