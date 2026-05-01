# Installation

If you want the fastest path to a first successful run, install the prebuilt CLI from npm.

## Fastest path: npm

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

Then continue with [Start here](./00-start-here.md) or [Your first release plan](./02-setup.md).

## Alternative: Cargo

If you prefer to install from Rust tooling instead:

```bash
cargo install monochange
monochange --help
mc --help
```

## Optional: assistant skill package

You do not need assistant tooling to use monochange.

When you want reusable agent guidance for Pi or other assistants, install the bundled skill into the current project with:

```bash
mc help skill
mc skill
mc skill --list
mc skill -a pi -y
```

`mc skill` forwards the remaining arguments to the upstream `skills add` flow, so you can keep the interactive prompts or pass the native `--agent`, `--skill`, `--copy`, `--all`, `--global`, and `--yes` flags directly.

<!-- {=assistantSkillBundleContents} -->

After copying the bundled skill, you get a small documentation set that is designed to load in layers:

- `SKILL.md` — concise entrypoint for agents
- `REFERENCE.md` — broader high-context reference with more examples
- `skills/README.md` — index of focused deep dives
- `skills/adoption.md` — setup-depth questions, migration guidance, and recommendation patterns
- `skills/changesets.md` — changeset authoring and lifecycle guidance
- `skills/commands.md` — built-in command catalog and workflow selection
- `skills/configuration.md` — `monochange.toml` setup and editing guidance
- `skills/linting.md` — `[lints]` presets, `mc check`, and manifest-focused examples
- `examples/README.md` — condensed scenario examples for quick recommendations

This layout keeps the top-level skill small while still making the richer guidance available when an assistant needs more context.

<!-- {/assistantSkillBundleContents} -->

Assistant-specific setup is covered in [Advanced: Assistant setup and MCP](./09-assistant-setup.md).

## CLI names

The main CLI is `monochange` and the short alias is `mc`.

## Repository development

If you are working on the monochange repository itself, use the reproducible development shell:

<!-- {=repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc validate
mc discover --format json
mc change --package monochange --bump minor --reason "add release planning"
mc diagnostics --format json
mc release --dry-run --format json
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc release-record --from v1.2.3
mc tag-release --from HEAD --dry-run --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc publish-plan --readiness .monochange/readiness.json --format json
mc publish --output .monochange/publish-result.json
mc repair-release --from v1.2.3 --target HEAD --dry-run
mc release
```

<!-- {/repoDevEnvironmentSetupCode} -->

Useful repository-development commands:

<!-- {=repoCommonDevelopmentCommands} -->

```bash
monochange --help
mc --help
docs:check
docs:update
mc validate
lint:all
test:all
coverage:all
coverage:patch
build:all
build:book
```

<!-- {/repoCommonDevelopmentCommands} -->
