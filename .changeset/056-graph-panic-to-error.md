---
monochange_graph: patch
---

Convert panics to proper errors when changesets reference packages or groups not found in the workspace. Warn instead of silently skipping unresolvable version group members during graph traversal.
