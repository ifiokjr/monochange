# Contributing to monochange

Thank you for contributing.

## Development environment

This repository uses `devenv` for a reproducible shell.

<!-- {=repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc step:validate
mc step:discover --format json
mc step:create-change-file --package monochange --bump minor --reason "add release planning"
mc step:diagnose-changesets --format json
mc step:prepare-release --dry-run --format json
mc step:publish-release --dry-run --format json
mc step:open-release-request --dry-run --format json
mc step:release-record --from v1.2.3
mc step:tag-release --from HEAD --dry-run --format json
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:placeholder-publish --from HEAD --output .monochange/bootstrap-result.json
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:plan-publish-rate-limits --readiness .monochange/readiness.json --format json
mc step:publish-packages --output .monochange/publish-result.json
mc step:retarget-release --from v1.2.3 --target HEAD --dry-run
mc step:prepare-release
```

<!-- {/repoDevEnvironmentSetupCode} -->

## Documentation workflow

Shared documentation blocks live in `.templates/` and are synchronized with `mdt`.

- edit provider blocks in `.templates/` when you want one change to update multiple docs
- run `docs:update` after changing shared docs or consumer blocks
- run `docs:check` before opening a PR to confirm everything is synchronized

## Expected workflow

1. Create a feature branch from `main`.
2. Write failing tests first for non-trivial behavior.
3. Implement the smallest change that makes the tests pass.
4. Update docs, READMEs, fixtures, changeset examples, and templates when behavior changes.
5. Run the full local validation suite before opening a PR.

## Core commands

<!-- {=contributingCoreCommands} -->

```bash
monochange --help
mc --help
docs:check
docs:update
mc step:validate
mc step:create-change-file --package monochange --bump patch --reason "describe the change"
lint:all
test:all
coverage:all
coverage:patch
build:all
build:book
```

<!-- {/contributingCoreCommands} -->

## Product rules

- Keep `crates/monochange` as the CLI package.
- Keep `crates/monochange_core` focused on shared domain types.
- Put adapter-specific manifest behavior in ecosystem crates.
- Preserve fixture-first validation for discovery and planning behavior.
- Treat `docs/` as a product surface, not an afterthought.
- Prefer configured package ids and group ids over raw manifest paths in changesets and docs.

## Testing requirements

- Every non-trivial behavior change starts with a failing test.
- Release-planning logic needs realistic fixture coverage.
- Cross-ecosystem behavior should remain consistent across Cargo, npm-family, Deno, Dart, and Flutter.
- `mc step:validate` should stay green alongside the rest of the validation suite.

## Safety and linting constraints

- `unsafe_code` is denied.
- `unstable_features` is denied.
- strict clippy and formatting checks stay enabled.
- explicit panic context is preferred over bare `.expect(...)`.
