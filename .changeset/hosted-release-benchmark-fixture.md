---
monochange: patch
---

#### add hosted release benchmark fixture tooling and docs

The benchmark script can now run against an existing repository checkout with `run-fixture`, which makes it practical to compare `main` and PR binaries against a real hosted benchmark repository instead of only the synthetic CI fixtures.

This release also adds `scripts/setup_hosted_benchmark_fixture.sh`, a repeatable way to seed a monochange benchmark repository with multiple packages, more than 200 commits, and release changesets introduced from PR-shaped branches, plus documentation for using that fixture when investigating hosted-provider latency.
