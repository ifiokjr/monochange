---
monochange: patch
monochange_cargo: patch
monochange_config: patch
monochange_core: patch
monochange_dart: patch
monochange_deno: patch
monochange_gitea: patch
monochange_github: patch
monochange_gitlab: patch
monochange_graph: patch
monochange_npm: patch
---

Add `tracing` instrumentation for performance profiling and debugging. Tracing is off by default with near-zero overhead and activates via `--log-level <FILTER>` or the `RUST_LOG` environment variable. Spans with timing cover CLI dispatch, workspace discovery, release preparation, git operations, provider API calls, changelog rendering, and lockfile materialization.
