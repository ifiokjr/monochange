---
"monochange": patch
---

#### replace Python-based CLI test harnesses with Rust PTY helpers

monochange no longer relies on `python3` for the interactive CLI integration tests that exercise TTY progress output and `mc change --interactive` flows.

This matters for contributors and CI environments because the test suite can now run without a Python runtime installed just to drive terminal interactions.

**Before:** the Rust tests spawned inline Python scripts that opened a PTY and sent interactive input.

```bash
cargo test -p monochange --test cli_progress --test changeset_target_metadata
```

Those tests required a working `python3` binary in `PATH`.

**After:** the same test commands use a Rust-native PTY helper instead.

```bash
cargo test -p monochange --test cli_progress --test changeset_target_metadata
```

No Python interpreter is required for those integration tests anymore.
