# Monochange assistant distribution implementation plan

## Stage 1 — exploration artifacts

- [ ] capture the `mdt` MCP, assist, release, and npm packaging patterns
- [ ] draft Monochange-specific architecture and publish plan
- [ ] draft the skill content that teaches correct Monochange usage
- [ ] decide package names:
  - [ ] `@monochange/cli`
  - [ ] `@monochange/skill`

## Stage 2 — assistant-facing UX

- [ ] add a built-in `assist` subcommand outside config-defined commands
- [ ] support assistants:
  - [ ] `generic`
  - [ ] `claude`
  - [ ] `cursor`
  - [ ] `copilot`
  - [ ] `pi`
- [ ] include in output:
  - [ ] install instructions for `@monochange/cli`
  - [ ] install instructions for `@monochange/skill`
  - [ ] MCP config snippet for `monochange mcp`
  - [ ] repo-local guidance
  - [ ] assistant-specific notes
- [ ] document the command in docs and README

## Stage 3 — skill distribution package

- [ ] add tracked skill source files
- [ ] add `@monochange/skill` package metadata
- [ ] add a helper executable such as `monochange-skill`
- [ ] support helper modes:
  - [ ] print install instructions
  - [ ] print bundled skill content
  - [ ] copy skill files to a target directory
- [ ] document install flows for Pi and generic agents

## Stage 4 — npm CLI distribution

- [ ] add npm launcher script for `monochange`
- [ ] expose `mc` alias through the same launcher if feasible
- [ ] add scripts to build root and platform npm packages from GitHub release assets
- [ ] add scripts to publish npm packages safely and idempotently
- [ ] document the npm install path in README and docs

## Stage 5 — GitHub release automation

- [ ] add `release.yml` modeled after `mdt`
- [ ] build target release archives for the `monochange` binary
- [ ] upload checksums and archives to GitHub releases
- [ ] emit a metadata artifact for downstream npm publishing
- [ ] add `npm-publish.yml` modeled after `mdt`
- [ ] publish CLI platform packages before the root package
- [ ] publish `@monochange/skill`

## Stage 6 — MCP server

- [ ] add a new `monochange_mcp` crate
- [ ] add `rmcp` workspace dependency
- [ ] add `monochange mcp` and `mc mcp` entrypoints
- [ ] expose first-slice tools:
  - [ ] validate
  - [ ] discover
  - [ ] change
  - [ ] release preview
  - [ ] release manifest
  - [ ] verify changesets
- [ ] prefer dry-run and inspection-first behavior
- [ ] add crate docs and end-user docs

## Stage 7 — validation and docs

- [ ] add unit coverage for assistant payload generation
- [ ] add tests for npm launcher resolution
- [ ] add tests for skill helper script behavior
- [ ] add MCP smoke coverage where practical
- [ ] update docs, README, and agent guidance
- [ ] add changesets for user-visible behavior

## Notes

- Use `mdt` as the reference pattern, not a byte-for-byte copy.
- Keep Monochange’s existing config-defined CLI model intact; `assist` and `mcp` should be explicitly built-in.
- Treat `@monochange/skill` as a documentation-and-guidance distribution package, not as a replacement for the MCP server.
