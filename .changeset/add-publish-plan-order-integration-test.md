---
main: patch
---

# add publish-plan dependency-order integration coverage

This release adds regression coverage for publish planning across rate-limit batches. The new fixture exercises a chain of crates so `mc publish-plan --format json` must keep dependency-ordered packages together when later batches include earlier packages.

Command:

```bash
mc publish-plan --format json
```

The behavior is unchanged for users, but future changes to publish planning now have integration coverage that snapshots the grouped `publishRateLimits` output and verifies cumulative batch ordering.
