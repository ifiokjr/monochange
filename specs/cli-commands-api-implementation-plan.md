# CLI Commands API Implementation Plan

## Goals

- rename the configuration namespace from `workflows` to `cli`
- use a command-keyed map shape: `[cli.<command>]`
- keep the current step model and runtime behavior intact
- improve naming clarity without reducing coverage

## Implementation slices

1. **Core model rename**
   - rename workflow domain types to CLI command types
   - rename default command helpers
   - rename runtime result wording from workflow to command

2. **Config parser migration**
   - parse `[cli.<command>]` into command definitions
   - reject legacy `[[workflows]]` with a migration error
   - rename `dry_run` to `dry_run_command` while accepting the old field as an alias for one transition window

3. **CLI runtime update**
   - build clap subcommands from configured CLI commands
   - keep implicit `--dry-run` support
   - preserve default synthesized commands when config omits `cli`

4. **Init output update**
   - emit `[cli.<command>]` definitions from `mc init`

5. **Docs and contracts**
   - update root config, mdBook, READMEs, quickstart, and contracts
   - replace workflow terminology with CLI command terminology where appropriate

6. **Test and coverage update**
   - update parser and init tests for the new map shape
   - add a regression test for rejecting legacy `[[workflows]]`
   - keep CLI output snapshots passing with updated wording and JSON keys

## Acceptance criteria

- `monochange.toml` uses `[cli.<command>]`
- `mc init` emits `[cli.<command>]`
- `mc release` and related commands still work unchanged from the user’s point of view
- legacy `[[workflows]]` yields a helpful error
- `cargo test --workspace --all-features` passes
- `devenv shell -- lint:all` passes
- `devenv shell -- docs:verify` passes
- `devenv shell -- build:book` passes
