# GitHub templates

Use this guidance when creating pull requests, filing issues, or editing `.github/PULL_REQUEST_TEMPLATE.md` and `.github/ISSUE_TEMPLATE/*`.

## Pull request template expectations

When opening a PR, make the body easy for maintainers and CI reviewers to scan:

- Use a conventional-commit PR title such as `feat: add release planner option`, `fix: handle missing changesets`, or `docs: update CLI guide`.
- Start with a concise summary of user-visible changes.
- Explain why the change is needed and link related issues with `Closes #...` when applicable.
- Identify the change type and affected areas, especially CLI commands, configuration schema, release planning, package ecosystem adapters, source providers, CI, docs, and agent skill/package docs.
- Include exact validation commands and relevant output. Prefer repository scripts such as `mc validate`, `lint:all`, `build:all`, `docs:update`, and `docs:check` where they apply.
- State changeset status explicitly: added, not needed because the change is internal-only, or not needed because the touched paths are ignored by changeset policy.
- For docs changes, mention whether shared `.templates/` blocks changed and whether `docs:update` and `docs:check` were run.
- Include risk and rollout notes for compatibility, migrations, release behavior, or operational changes.

If a PR template does not exist or needs replacement, prefer one default `.github/PULL_REQUEST_TEMPLATE.md` over multiple PR templates unless the project has clearly distinct contribution paths.

## Issue template expectations

When filing issues or creating issue templates, preserve the repository title rules:

- Issue titles use sentence case.
- Issue titles must not end with a full stop.
- Issue titles must not use conventional-commit prefixes such as `feat:` or `fix:`.

Prefer GitHub issue forms under `.github/ISSUE_TEMPLATE/` for structured reports:

- `bug_report.yml` for broken or unexpected behavior.
- `feature_request.yml` for user-facing behavior changes.
- `documentation.yml` for missing, stale, confusing, or incorrect docs.
- `release_or_ci.yml` for changeset policy, coverage, benchmarks, publishing, GitHub release assets, or cargo-binstall issues.
- `config.yml` to disable blank issues when discussions are a better place for questions.

Issue forms should request the smallest useful reproduction: monochange version, affected area, command or workflow, relevant `monochange.toml`/changeset/package manifest snippets, expected behavior, actual behavior, logs, and environment details.

## Template maintenance

When editing GitHub templates:

- Keep the project name lowercase as `monochange` in prose.
- Keep PR titles, issue titles, and commit titles aligned with the naming rules in `AGENTS.md`.
- Keep validation checklists aligned with current repository scripts and CLI commands.
- Include `mc step:affected-packages --verify --changed-paths <files>` guidance for changeset coverage when the template discusses published package changes.
- Remind contributors that docs shared across README, guide, and package docs usually flow through `.templates/` and require `docs:update`.
- Run `dprint fmt` before committing template or agent-doc changes.
