---
"main": patch
---

# Preserve publish batch dependency order

Carry prior packages into later publish-plan batches so dependency-ordered publish requests remain available when registry rate limits split a release into multiple jobs.
