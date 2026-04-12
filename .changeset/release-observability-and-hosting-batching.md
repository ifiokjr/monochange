---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### add streamed release progress, named steps, and real-path release timing diagnostics

`mc release` and `mc commit-release` now stream step progress while they run instead of waiting for a whole command to finish. Built-in steps carry stable display names, long-running steps show loading indicators in TTY sessions, command stdout and stderr stream underneath the active step, and `PrepareRelease` emits per-phase timings so slow release phases are visible without taking a separate trace.

**Before:**

```bash
mc release --dry-run
```

Progress output was only available in the default auto mode, step labels often fell back to the raw step kind, and machine-readable timing data was not available for the real `mc release` path.

**After:**

```bash
mc release --progress-format unicode
mc release --progress-format json
MONOCHANGE_PROGRESS_FORMAT=ascii mc commit-release
```

The human renderers now use named steps plus streamed command output, while `--progress-format json` writes newline-delimited lifecycle events to stderr:

```json
{"event":"step_started","command":"release","stepKind":"PrepareRelease","stepDisplayName":"plan release"}
{"event":"step_finished","command":"release","stepKind":"PrepareRelease","phaseTimings":[{"label":"discover release workspace","durationMs":97}]}
```

Custom CLI definitions can opt into clearer output by attaching explicit step names:

```toml
[cli.release]
steps = [
	{ name = "plan release", type = "PrepareRelease" },
	{ name = "stream summary", type = "Command", command = "printf 'done\n'", show_progress = true },
]
```

Hosted GitHub enrichment now batches review-request lookups into a single GraphQL request, reducing repeated network overhead during release preparation. The benchmark workflow also measures both `mc release --dry-run` and the real `mc release` path, and Cargo lockfile refresh stays opt-in instead of forcing `cargo generate-lockfile` automatically.
