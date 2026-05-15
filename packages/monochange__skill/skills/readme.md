# monochange skill modules

Use these focused guides when the top-level [SKILL.md](../SKILL.md) is not enough context.

## Task routing

| If the user asks about...                                                                                              | Open this first                                                                         |
| ---------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------- |
| Which `mc` command to run, whether a command is built in, or how `[cli.*].steps` compose                               | [commands.md](./commands.md)                                                            |
| Writing or reviewing `monochange.toml`, package/group ids, providers, ecosystems, versioned files, or custom workflows | [configuration.md](./configuration.md)                                                  |
| Creating, editing, validating, or diagnosing `.changeset/*.md` release intent                                          | [changesets.md](./changesets.md)                                                        |
| Manifest policy, lint presets, `mc check`, or rule explanations                                                        | [linting.md](./linting.md)                                                              |
| Publish readiness, bootstrap placeholders, publish plans, partial retries, or registry package publishing              | [multi-package-publishing.md](./multi-package-publishing.md)                            |
| OIDC and trusted-publishing setup                                                                                      | [trusted-publishing.md](./trusted-publishing.md)                                        |
|  Migrating an existing monorepo/release workflow into monochange, or from knope specifically                                                        | [adoption.md](./adoption.md)                                                            |
| Release note style and changeset wording quality                                                                       | [artifact-types.md](./artifact-types.md) and [changeset-guide.md](./changeset-guide.md) |
| A broader end-to-end operating reference                                                                               | [reference.md](./reference.md)                                                          |
| Copyable scenario examples                                                                                             | [../examples/readme.md](../examples/readme.md)                                          |

## Module list

- [commands.md](./commands.md) — command inventory and `[cli.*]` step composition.
- [configuration.md](./configuration.md) — `monochange.toml` examples and configuration rules.
- [changesets.md](./changesets.md) — authoring release intent.
- [reference.md](./reference.md) — complete usage reference.
- [linting.md](./linting.md) — `mc check` and manifest lint configuration.
- [multi-package-publishing.md](./multi-package-publishing.md) — package publishing workflows.
- [trusted-publishing.md](./trusted-publishing.md) — OIDC/trusted-publishing notes.
- [adoption.md](./adoption.md) — adoption and migration guide for existing monorepos.
- [artifact-types.md](./artifact-types.md) and [changeset-guide.md](./changeset-guide.md) — writing high-quality release notes.
- [../examples/readme.md](../examples/readme.md) — copyable scenario examples.
