# @monochange/skill

Installable agent skill for monochange.

This package bundles:

- `SKILL.md` — concise agent instructions
- `REFERENCE.md` — deeper usage and installation guidance
- `monochange-skill` — helper executable for printing or copying the bundled skill

## Install

```bash
npm install -g @monochange/skill
```

## Helper usage

```bash
monochange-skill --print-install
monochange-skill --print-skill
monochange-skill --copy ~/.pi/agent/skills/monochange
```

## What the skill teaches

The bundled skill explains how to:

- read `monochange.toml`
- validate a workspace with `mc validate`
- inspect the normalized model with `mc discover --format json`
- create explicit change files with `mc change`
- preview release effects with `mc release --dry-run --format json`
- understand groups, package ids, changelogs, and source-provider release flows
