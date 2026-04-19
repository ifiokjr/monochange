# `mc skill` subcommand

## Goal

Add a built-in `mc skill` command that installs the monochange skill bundle into the current project through the upstream `skills add` workflow.

## Scope

- add CLI wiring for `mc skill`
- detect an available launcher from `npx`, `pnpm dlx`, or `bunx`
- forward interactive or non-interactive `skills add` flags to the upstream installer
- document the new command in CLI help and assistant-facing docs
- add tests and fixtures that cover runner selection, forwarded flags, and failure paths

## Non-goals

- replacing `mc subagents` or `mc mcp`
- reimplementing the upstream `skills add` prompt flow inside monochange
- adding a global skill-management surface beyond installing the monochange skill source

## Files and crates

- `crates/monochange/src/cli.rs`
- `crates/monochange/src/lib.rs`
- `crates/monochange/src/skill.rs` or equivalent helper module
- `crates/monochange/src/__tests.rs`
- `fixtures/tests/skill/...`
- assistant-facing docs and READMEs
- `.changeset/*.md`

## Checklist

- [x] add a failing CLI test for `mc skill` parsing and forwarded flags
- [x] implement runner detection and source resolution for the monochange skill bundle
- [x] execute `skills add <monochange-source>` with forwarded args in the workspace root
- [x] cover runner fallback and missing-runner failures with tests
- [x] update docs and skill references to point at `mc skill`
- [x] add or update the changeset
- [x] run validation, including patch coverage

## Validation

- `devenv shell cargo test -p monochange skill -- --nocapture`
- `devenv shell cargo test -p monochange`
- `devenv shell fix:all`
- `devenv shell mc validate`
- `devenv shell coverage:patch`

## Notes

- `mc skill` should stay thin and let the upstream `skills` CLI own the interactive install UX.
- The command should prefer local project installation by default, while still allowing forwarded upstream flags such as `-g`, `-a`, `--skill`, `--copy`, `--list`, `--all`, and `-y`.
- After rebasing `feat/skill-command` onto the latest `main`, `devenv shell coverage:patch` completed successfully but reported `PATCH_COVERAGE 0/0 (100.00%)` because the branch no longer had committed diff hunks yet; committed patch coverage should be rechecked again after the feature commit is created.
