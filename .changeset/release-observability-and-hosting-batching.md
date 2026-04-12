---
main: patch
---

Improve `mc release` ergonomics and hosted-source performance by batching GitHub review-request enrichment into a single GraphQL request, adding step names plus live progress output, and keeping Cargo lockfile refresh opt-in instead of falling back to `cargo generate-lockfile` automatically.
