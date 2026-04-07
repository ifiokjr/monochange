---
monochange: patch
monochange_core: patch
monochange_github: patch
monochange_gitlab: patch
monochange_gitea: patch
---

Improved snapshot-based test coverage across CLI, MCP, changelog rendering, and source-provider payload tests.

Added reusable external `insta` snapshot helpers with shared redactions for unstable values such as temp paths, dates, and commit SHAs so snapshots stay reviewable across platforms.
