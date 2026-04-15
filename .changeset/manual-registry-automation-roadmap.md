---
"@monochange/skill": patch
---

#### document future automation boundaries for manual registries

Adds a short roadmap-style section to the trusted-publishing docs describing where monochange may add stronger automation or validation later for `crates.io`, `jsr`, and `pub.dev`.

It also makes the current boundary explicit:

- npm is still the only registry with built-in trusted-publishing enrollment
- manual registries remain guidance- and diagnostics-first today
- registry-side admin or browser-confirmed steps are still treated as manual unless the registry exposes a safer automation path later
