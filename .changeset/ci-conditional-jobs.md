---
monochange: test
"@monochange/skill": test
---

# add realistic path-filter gates to CI jobs

Introduce a `changes` job using `dorny/paths-filter` so CI can skip expensive work for pull requests that do not touch relevant project surfaces.

The filters now map to monochange's actual repo shape:

- `rust` covers Rust source, workspace manifests, lockfile, and Rust tooling config.
- `fixtures` covers repository fixture and test data folders that feed integration tests.
- `global` covers repo-wide CI/runtime config such as `.github/workflows/ci.yml`, `monochange.toml`, and devenv files.
- `release` covers changesets and generated release-record state.
- `js` covers scripts, npm package sources, TypeScript config, and pnpm files.
- `docs` covers book/docs, repository docs, agent docs, and shared-doc templates.
- `workflows` covers GitHub workflow and action definitions.

The workflow also adds step-level gates inside mixed jobs, plus a lightweight `build-docs` path for documentation-only changes, so docs edits can verify documentation and book builds without launching the cross-platform Rust build matrix. Merge-queue events still bypass the filters and run the full suite.
