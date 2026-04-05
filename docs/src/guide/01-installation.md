# Installation

MonoChange can be installed either from npm as a prebuilt CLI or from Cargo.

## npm

Install the published CLI package:

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

Install the bundled skill package when you want reusable agent guidance for Pi or other assistants:

```bash
npm install -g @monochange/skill
monochange-skill --print-install
monochange-skill --copy ~/.pi/agent/skills/monochange
```

## Cargo

```bash
cargo install monochange
monochange --help
mc --help
```

## Repository development

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
mc release
```

<!-- {/repoDevEnvironmentSetupCode} -->

Useful commands:

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

## CLI names

The main CLI is `monochange` and the short alias is `mc`.

MonoChange also ships built-in assistant setup helpers:

```bash
mc assist pi
mc mcp
```
