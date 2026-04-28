---
monochange: feat
monochange_core: feat
monochange_lint: feat
---

#### Improve lint and check progress output

Add beautiful interactive progress reporting for `mc check` and `mc lint`.

- Introduced `LintProgressReporter` trait in `monochange_core::lint` with 14 lifecycle hooks from planning through summary.
- Added `NoopLintProgressReporter` for silent / backward-compatible operation.
- Updated `Linter::lint_workspace` to emit planning, suite, file, rule, and summary events to the reporter.
- Created `HumanLintProgressReporter` in `monochange` that writes animated spinners, suite-level progress, fix tracking, and a styled summary to stderr.
- Enhanced `format_check_report` to list which files were fixed when `--fix` is active.
- Respects `NO_COLOR` and `MONOCHANGE_NO_PROGRESS` environment variables.
