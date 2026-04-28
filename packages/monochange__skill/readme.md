# @monochange/skill

Installable agent skill for monochange.

This package bundles:

- `SKILL.md` — concise entrypoint for agents
- `skills/readme.md` — index of focused deep dives
- `skills/adoption.md` — setup-depth questions, migration guidance, and recommendation patterns
- `skills/changesets.md` — changeset authoring and lifecycle guidance
- `skills/commands.md` — built-in command catalog and usage patterns
- `skills/configuration.md` — `monochange.toml` setup and editing guidance
- `skills/linting.md` — `mc check`, `[lints]`, presets, and manifest-focused rule explanations with examples
- `examples/readme.md` — condensed scenario examples for quick recommendations
- `skills/reference.md` — high-context reference with broader examples
- `skills/trusted-publishing.md` — GitHub/OIDC setup guidance for `npm`, `crates.io`, `jsr`, `pub.dev`, PyPI, and Go module tags
- `skills/multi-package-publishing.md` — monorepo-oriented publishing patterns for multiple public packages
- `monochange-skill` — helper executable for printing or copying the bundled skill

## Install

For project-local installation through the upstream `skills add` flow, prefer:

```bash
mc help skill
mc skill
mc skill -a pi -y
```

If you need the lower-level package helper directly, install it with:

```bash
npm install -g @monochange/skill
```

## Helper usage

```bash
monochange-skill --print-install
monochange-skill --print-skill
monochange-skill --copy ~/.pi/agent/skills/monochange
```

`monochange-skill --copy` copies the full skill bundle, including `SKILL.md`, `skills/reference.md`, `skills/trusted-publishing.md`, `skills/multi-package-publishing.md`, the `skills/` deep-dive folder, and the bundled `examples/` folder.

## What the skill teaches

The bundled skill explains how to:

- plan adoption in `quickstart`, `standard`, `full`, or `migration` mode
- create or refine `monochange.toml` with `mc init`, generated `[cli.*]` workflow commands, `mc step:*`, and `mc validate`
- inspect the normalized model with `mc discover --format json`
- create, update, and audit explicit change files with `mc change` and `mc diagnostics`, including dependency-follow-up notes with `caused_by`
- preview release effects with `mc release --dry-run --format json` and `mc release --dry-run --diff`
- inspect durable release history with `mc release-record`
- understand groups, package ids, changelogs, linting policy, package publishing, and source-provider release flows
- set up trusted publishing / OIDC-backed package publishing for the registries that monochange supports
- choose sane multi-package publish patterns when one repository ships multiple public packages
- point users at condensed bundled examples and fuller repository-level example indexes

Open [SKILL.md](./SKILL.md) first, then use [skills/readme.md](./skills/readme.md), [examples/readme.md](./examples/readme.md), [skills/reference.md](./skills/reference.md), [skills/trusted-publishing.md](./skills/trusted-publishing.md), and [skills/multi-package-publishing.md](./skills/multi-package-publishing.md) for the deeper sections.

For fuller repo-shaped examples in the monochange repository, see <https://github.com/monochange/monochange/tree/main/examples>.
