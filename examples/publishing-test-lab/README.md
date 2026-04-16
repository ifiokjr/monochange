# Publishing test lab

This directory is a seed for a follow-up issue, not a real publish fixture yet.

See [ISSUE.md](./ISSUE.md) for a draft issue that can be filed or adapted.

## Goal

Stand up a separate repository that exercises real registry publication across the ecosystems monochange supports, especially where rate limits, trust enrollment, and delayed publish windows can break otherwise-correct release plans.

## Why a separate repository

- package names and auth need stronger isolation than in-repo fixtures can provide
- publish tests should be allowed to fail, retry, and back off without affecting the main monochange repository
- different ecosystems may need different namespaces, registries, or cleanup rules

## Recommended issue scope

- mirror a small set of monochange-controlled fixture packages into an external test repo
- validate GitHub and GitLab publish shapes where possible
- test npm, `crates.io`, `jsr`, and `pub.dev` separately
- add scenarios that intentionally hit publish ordering and rate-limit edges
- document when to use sandbox registries, alternate registries, or dedicated public test namespaces

## Registry strategy recommendation

Use non-production registries or sandbox modes where the ecosystem supports them. When a public production registry is unavoidable, keep a tightly controlled set of dedicated test packages and treat them as long-lived test infrastructure.
