# Installation

`monochange` is currently developed from source inside this repository.

## Repository development

<!-- {=repoDevEnvironmentSetupCode} -->

```bash
devenv shell
install:all
mc validate
mc discover --format json
mc change --package monochange --bump minor --reason "add release planning"
mc release --dry-run --format json
mc publish-release --dry-run --format json
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
