# GitLab migration

This is a repo-shaped example for adopting monochange into an existing GitLab-hosted repository without replacing the current publish script on day one.

## What this example includes

- `monochange.toml` with GitLab source config, Cargo discovery, external publishing, and top-level `[lints]`
- a small Cargo workspace with a library crate and a CLI crate
- `.gitlab-ci.yml` that introduces validation and dry-run planning before replacing the legacy publish path
- `scripts/legacy-release.sh` to show a coexistence phase where monochange handles planning while an older release script still owns the publish step
- a sample `.changeset/*.md` file so the workspace has pending release intent

## File layout

```text
.changeset/
crates/
  cli/
  core/
scripts/
.gitlab-ci.yml
Cargo.toml
monochange.toml
```

## Recommended validation flow

From this directory, run:

```bash
mc validate
mc check
mc release --dry-run --diff
```

## Why this example is opinionated

- it models migration, so package publishing stays `mode = "external"`
- it uses GitLab CI for validation, preview, and release detection, but keeps the actual publish command in a legacy script during the first phase
- it uses `[lints].use = ["cargo/recommended"]` plus a scoped override rather than a large handwritten lint table
- it shows that monochange can be introduced before the team is ready to replace existing release automation completely

## Migration notes

- the `legacy-release.sh` script is intentionally simple; in a real migration it could be a pre-existing script or wrapper around current jobs
- once the team trusts monochange release planning, the legacy publish step can be replaced with `mc publish` or another registry-native workflow
- if GitLab auth/bootstrap differs strongly from monochange's built-in assumptions, staying on `mode = "external"` is still the clearest path
