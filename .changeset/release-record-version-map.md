---
monochange: patch
monochange_core: patch
monochange_schema: patch
---

# Replace release record `groupVersion` with `versions`

Release records now include a `versions` map keyed by released package or group id, and no longer write the redundant `groupVersion` field.
