# monochange

> manage versions and releases for you multiplatform, multilanguage monorepo

## Motivation

Managing versions in multi-language monorepos is difficult.

- Manage version increments across the monorepo
- Manage package / crate dependencies even across languages. For example a node package might depend
  on a rust crate and the version should be incremented with the rust version.
- Manage git tagging, releases on npm / github, crates.io.

## Contributing

[`devenv`](https://devenv.sh/) is used to provide a reproducible development environment for this
project. Follow the [getting started instructions](https://devenv.sh/getting-started/).

To automatically load the environment you should
[install direnv](https://devenv.sh/automatic-shell-activation/) and then load the `direnv`.

```bash
# The security mechanism didn't allow to load the `.envrc`.
# Since we trust it, let's allow it execution.
direnv allow .
```

At this point you should see the `nix` commands available in your terminal.

To setup recommended configuration for your favourite editor run the following commands.

```bash
setup:vscode # Setup vscode
setup:helix  # Setup helix configuration
```
