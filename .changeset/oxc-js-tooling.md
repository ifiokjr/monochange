---
"@monochange/root": patch
---

Add OXC tooling for all JavaScript and TypeScript in the project

- Integrate `dprint-plugin-oxc` as the formatter for JS/TS files, replacing
  the dprint typescript plugin.
- Add `oxfmt` and `oxlint` configuration (`.oxfmtrc.json` and
  `.oxlintrc.json`) with rules adapted from the sibling `actions` repo.
- Add `tsgo` for type-checking and `tsdown` for bundling, with a root
  `tsdown.config.json`.
- Wire everything into `devenv.nix` with new scripts: `lint:js`,
  `lint:js:syntax`, `lint:js:types`, `fix:js`, and `build:js`. Update
  `lint:all` and `fix:all` to include the JS checks.
- Add JS devDependencies (`oxfmt`, `oxlint`, `@rslint/tsgo`, `tsdown`)
  to `package.json`.
- Add JS dependency installation (`pnpm install --frozen-lockfile`) to the
  shared `devenv` GitHub Action so CI has the tools available.
- Add `lint:js:syntax` check to the CI `lint` job.
