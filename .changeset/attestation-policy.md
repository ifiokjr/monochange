---
"monochange_core": patch
"monochange_config": patch
"monochange": patch
---

# add attestation policy configuration

Add first-class package and release attestation policy settings. Package publish settings now support `publish.attestations.require_registry_provenance`, inherited from ecosystem publish settings and overridable per package.

When registry provenance is required, built-in release publishing fails before invoking registry commands unless trusted publishing is enabled, the current CI/OIDC identity is verifiable, the provider/registry capability matrix reports registry-native provenance support, and the built-in publisher can require that provenance. npm release publishes add `--provenance` only when this policy is enabled.

GitHub release asset attestation intent is modeled under `[source.releases.attestations]` with `require_github_artifact_attestations`, which is accepted only for GitHub sources.
