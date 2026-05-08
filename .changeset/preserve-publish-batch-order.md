---
monochange: fix
---

# Preserve publish batch dependency order

Carry prior packages into later publish-plan batches so dependency-ordered publish requests remain available when registry rate limits split a release into multiple jobs.

This fixes publish plans for releases that are split by registry rate limits. Dependent packages now continue to see their earlier dependency-ordered predecessors in later publish jobs instead of publishing before required package versions are available.
