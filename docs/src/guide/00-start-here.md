# Start here

monochange is easiest to learn with one safe local walkthrough.

In about 10 minutes you will:

- install the CLI
- generate a starter `monochange.toml` with `mc init`
- validate the workspace
- discover package ids
- create one change file
- preview a release plan with `--dry-run`

This first run is safe: nothing is published.

## 1. Install the CLI

The fastest path is the prebuilt npm package:

```bash
npm install -g @monochange/cli
monochange --help
mc --help
```

If you prefer a Rust-native install, use:

```bash
cargo install monochange
monochange --help
mc --help
```

## 2. Generate a starter config

Run `mc init` at the repository root:

```bash
mc init
```

`mc init` scans the repository, detects packages, and writes an annotated starter `monochange.toml`.

Start with the generated file instead of hand-authoring your first config.

## 3. Validate the workspace

```bash
mc validate
```

This checks `monochange.toml` and your `.changeset/*.md` files together.

## 4. Discover package ids

```bash
mc discover --format json
```

Look for the package ids you will use in changesets and CLI commands.

If you do not know which id to target later, rerun discovery and copy one directly from the output.

## 5. Create one change file

```bash
mc change --package <id> --bump patch --reason "describe the change"
```

Most first changes should target a package id.

Use group ids only when the change is intentionally owned by the whole group.

## 6. Preview the release plan safely

```bash
mc release --dry-run --format json
```

Stop here on your first run. This previews the release plan without publishing anything.

## Package ids first, groups later

A good first-time mental model is:

1. monochange discovers packages.
2. You author explicit changes against package ids.
3. monochange propagates dependent bumps for you.
4. Groups synchronize packages that intentionally share release identity.

That is why most beginner flows should start with package ids, not groups.

## If you hit a problem

- `mc init` says a config already exists: keep the existing `monochange.toml` and continue with `mc validate`, or pass `--force` to regenerate.
- `mc validate` reports problems: fix the reported config or changeset issue, then rerun `mc validate`.
- `mc change` rejects your target: rerun `mc discover --format json` and copy a valid package id.
- You are not sure what to do next: continue with [Your first release plan](./02-setup.md).

## Next steps

- [Installation](./01-installation.md) — install paths, optional assistant tooling, and repository development setup
- [Your first release plan](./02-setup.md) — a fuller walkthrough built around `mc init`
- [Discovery](./03-discovery.md) — what discovery finds and how ids are rendered
- [Configuration](./04-configuration.md) — evolve the generated config once the basics feel familiar
- [Advanced: GitHub automation](./08-github-automation.md) — provider publishing, release PRs, and automation
- [Advanced: Assistant setup and MCP](./09-assistant-setup.md) — optional AI-assisted workflows
