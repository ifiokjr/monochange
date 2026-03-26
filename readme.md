# monochange

> manage versions and releases for your multiplatform, multilanguage monorepo

## Note

⚠️ This project is still under active development while the API and workflows are being discovered.

## Motivation

Managing versions in multi-language monorepos is difficult.

- coordinate version increments across the monorepo
- manage cross-language package dependencies
- automate tagging, changelogs, and release orchestration
- keep release intent explicit and auditable

## Development

[`devenv`](https://devenv.sh/) provides the reproducible development environment for this project. Follow the [getting started instructions](https://devenv.sh/getting-started/) and then enter the shell.

```bash
devenv shell
install:all
```

Useful commands:

```bash
monochange --help
mc --help
build:all
build:book
lint:all
test:all
snapshot:review
snapshot:update
```

To setup recommended editor configuration:

```bash
setup:vscode
setup:helix
```
