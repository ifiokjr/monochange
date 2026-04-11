---
monochange: minor
---

Add `--provider` flag to `mc init` for automated CI setup. When `--provider github` is specified, the init command populates the `[source]` config section (auto-detecting owner/repo from the git remote), adds a `commit-release` CLI command, and generates two GitHub Actions workflows: `changeset-policy.yml` for PR changeset verification and `release.yml` for automated release PR management.
