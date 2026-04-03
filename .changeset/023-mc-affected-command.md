---
monochange: minor
monochange_core: minor
monochange_config: patch
---

Rename `mc verify` to `mc affected` with new `--verify`, `--since`, and `--changed-paths` flags.

- default mode is informational: shows affected packages and changeset coverage without failing
- `--verify` flag enforces changeset coverage and exits non-zero when packages are uncovered
- `--since <rev>` computes changed files from a git revision instead of requiring explicit `--changed-paths`
- `--since` takes priority over `--changed-paths` when both are provided (with a warning)
- simplify `[source.bot.changesets]` config: package paths are now automatically included
- add `Boolean` variant to `CliInputKind` for flag-style CLI inputs
- keep `VerifyChangesets` as a backward-compatible alias for the `AffectedPackages` step type
