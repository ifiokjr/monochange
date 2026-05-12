# @monochange/skill

Agent guidance for using monochange as a CLI-driven release-planning harness.

The package is intentionally task-oriented: start with the short [SKILL.md](./SKILL.md) at runtime, then open the focused files under [skills/](./skills/readme.md) only when a task needs more context. The [examples](./examples/readme.md) are small enough to copy into a new repository and then tailor to its package graph.

monochange discovers packages in a monorepo, reads release intent from `.changeset/*.md`, computes package and group versions, updates versioned files, creates release records, and can drive provider/package publishing workflows configured in `monochange.toml`.

## Start here

- [SKILL.md](./SKILL.md) — short runtime instructions for agents.
- [skills/readme.md](./skills/readme.md) — index of focused modules and when to open each one.
- [skills/commands.md](./skills/commands.md) — verified built-in commands, step commands, and step types.
- [skills/configuration.md](./skills/configuration.md) — authoring `monochange.toml` with copyable examples.
- [skills/changesets.md](./skills/changesets.md) — creating and maintaining `.changeset/*.md` files.
- [skills/reference.md](./skills/reference.md) — complete reference for day-to-day operation.
- [skills/linting.md](./skills/linting.md) — `mc check`, lint presets, rule severity, and manifest policy.
- [skills/multi-package-publishing.md](./skills/multi-package-publishing.md) — readiness, bootstrap, and package publishing flows.
- [skills/trusted-publishing.md](./skills/trusted-publishing.md) — OIDC/trusted-publishing notes for package registries.
- [examples/readme.md](./examples/readme.md) — scenario examples.

## Important distinction

The CLI has three command classes:

1. **Binary commands** wired by the binary, such as `mc init`, `mc check`, and `mc mcp`; typed operations such as validation and publish readiness are exposed as `mc step:*` commands.
2. **Step commands** generated from built-in step variants, such as `mc step:discover` and `mc step:prepare-release`.
3. **User-defined workflow commands** created by `[cli.<name>]` in `monochange.toml`, such as `mc release` or `mc publish` in repositories that define them.

Always inspect `mc help` or `monochange.toml` before assuming a user-defined workflow command exists. A repository can expose friendly commands such as `mc release`, `mc change`, or `mc publish`, but those names are configuration, not CLI guarantees. The step commands remain the portable fallback.
