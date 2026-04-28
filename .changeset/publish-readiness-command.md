---
"monochange": patch
---

#### Add initial publish readiness command

Adds `mc publish-readiness` as a non-mutating preflight command for package registry publishing. The command reads a release record from `--from`, dry-runs registry publish checks for the selected package set, reports ready/already-published/unsupported package states, and can write a JSON readiness artifact with `--output`.
