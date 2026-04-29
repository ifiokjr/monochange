---
"monochange": patch
---

# Fix release merge blocker workflow

Replace the release PR merge blocker action with an inline shell guard so normal pull requests are not blocked by missing action dependencies.
