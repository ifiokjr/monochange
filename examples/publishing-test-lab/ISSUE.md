# Create end-to-end publishing verification in an external test repository

## Suggested labels

- `enhancement`
- `testing`
- `publishing`
- `ci`

## Summary

Create a dedicated external repository for end-to-end publish verification across the ecosystems monochange supports.

The goal is to verify real registry publication, trusted-publishing enrollment, publish ordering, and rate-limit behavior without coupling those tests to the main monochange repository.

## Problem

monochange can already validate release planning and dry-run publish behavior locally, but real publishing still has gaps that only show up when talking to real registries:

- trusted-publishing enrollment can drift from CI configuration
- package ordering bugs may only appear when a registry enforces real dependencies or visibility timing
- rate limits and delayed package availability can break otherwise-correct publish plans
- different ecosystems need different test namespaces, auth models, and retry strategies

Those concerns are awkward and risky to test inside this repository.

## Proposal

Create an external repository that is intentionally dedicated to publish verification.

The repository should:

- contain a small set of monochange-controlled fixture packages
- mirror representative package shapes from this codebase on a controlled cadence
- exercise GitHub and GitLab publish flows where possible
- keep production package names and production release workflows isolated from test runs

## Scope

### In scope

- GitHub-based publish verification for `npm`, `crates.io`, `jsr`, and `pub.dev`
- GitLab-based publish verification where monochange supports a realistic external workflow
- trusted-publishing / OIDC enrollment checks
- publish ordering tests for multi-package releases
- rate-limit and delayed-availability scenarios
- documentation for which registry strategy each ecosystem should use

### Out of scope

- replacing unit, fixture, or dry-run tests in the main monochange repository
- publishing the main monochange packages from the test repository
- pretending that one registry strategy fits every ecosystem equally well

## Recommended registry strategy

Prefer the safest non-production target that still exercises the behavior you care about.

### npm

- prefer a dedicated public test scope if canonical npm trusted publishing must be exercised
- use a private registry such as Verdaccio for fast non-production protocol checks that do not require npm-hosted trust state

### crates.io

- use dedicated long-lived public test crate names for canonical publish verification
- use alternate Cargo registries only for non-canonical protocol checks, not as a substitute for real crates.io verification

### JSR

- use a dedicated JSR scope or organization for long-lived test packages

### pub.dev

- use dedicated long-lived test packages and uploader/admin ownership that is clearly separate from production packages

## Acceptance criteria

- a separate repository exists and is documented from monochange
- at least one end-to-end publish scenario exists per supported public registry
- GitHub trusted-publishing verification is covered where supported
- rate-limit or delayed-availability scenarios are explicitly exercised
- failures in the test repository do not block unrelated monochange repository work by default
- the repository documents how fixtures are refreshed from monochange

## Suggested implementation plan

1. create the external repository and document ownership
2. seed one minimal fixture per ecosystem
3. wire GitHub publish verification first
4. add GitLab verification for the flows that remain meaningful outside GitHub-specific trust automation
5. add rate-limit and delayed-availability scenarios
6. document which scenarios hit public registries vs sandbox or alternate registries

## Open questions

- which package names or namespaces should be reserved for long-lived public test packages?
- how often should fixture content be synced from monochange?
- should registry-intensive scenarios run on a schedule, manually, or only for tagged releases?
- which failures should notify maintainers automatically versus stay as manual diagnostics?
