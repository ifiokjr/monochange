---
"monochange": patch
"@monochange/skill": patch
---

#### Harden publish readiness artifact freshness

Adds a publish input fingerprint to `mc publish-readiness` artifacts. `mc publish` and readiness-backed `mc publish-plan` now reject artifacts when workspace config, package manifests, lockfiles, or registry/tooling inputs changed after the artifact was written.
