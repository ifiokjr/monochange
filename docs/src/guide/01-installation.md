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

You do not need assistant tooling to use MonoChange.

When you want reusable agent guidance for Pi or other assistants, install the bundled skill package:

```bash
npm install -g @monochange/skill
monochange-skill --print-install
monochange-skill --copy ~/.pi/agent/skills/monochange
```

Assistant-specific setup is covered in [Advanced: Assistant setup and MCP](./09-assistant-setup.md).

## CLI names

The main CLI is `monochange` and the short alias is `mc`.

## Repository development

If you are working on the MonoChange repository itself, use the reproducible development shell:

<!-- {=repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc validate
mc discover --format json
mc change --package monochange --bump minor --reason "add release planning"
mc diagnostics --format json
mc release --dry-run --format json
mc release-manifest --dry-run
mc publish-release --dry-run --format json
mc release-pr --dry-run --format json
mc release-record --from v1.2.3
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
docs:verify
docs:doctor
mc validate
lint:all
test:all
coverage:all
build:all
build:book
```

<!-- {/repoCommonDevelopmentCommands} -->
