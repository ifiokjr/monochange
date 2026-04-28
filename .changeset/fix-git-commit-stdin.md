---
"monochange": patch
"monochange_core": patch
---

#### make release commits handle large messages reliably

Release commit creation now streams the generated commit message through standard input instead of passing the full release record as a command-line argument. This avoids operating-system argument length limits for large release records.

Git command spawning now reuses a stable path that contains `git`, and benchmark fixture git commands now use monochange's sanitized git command helper with automatic garbage collection disabled for deterministic synthetic history setup in CI.
