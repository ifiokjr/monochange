# Installation

`monochange` is currently developed from source inside this repository.

## Repository development

<!-- {=repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc check --root .
mc workspace discover --root . --format json
mc changes add --root . --package monochange --bump minor --reason "add release planning"
mc release --dry-run
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
mc check --root .
lint:all
test:all
coverage:all
build:all
build:book
```

<!-- {/repoCommonDevelopmentCommands} -->

## CLI names

The main CLI is `monochange` and the short alias is `mc`.
