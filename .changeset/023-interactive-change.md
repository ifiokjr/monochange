---
monochange: minor
monochange_core: minor
---

#### add interactive mode for mc change

Add `mc change --interactive` (`-i`) that guides users through package/group selection, per-target bump choices, optional explicit versions, change type, and release-note summary. Conflicting selections (a group and one of its members) are automatically prevented. Adds `short` alias support to CLI input definitions.
