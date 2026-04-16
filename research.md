# Publish rate-limit research notes

This document captures the evidence used for monochange's built-in publish rate-limit planner.

## Scope

The planner currently focuses on package-registry publish operations for:

- `crates.io`
- `npm`
- `jsr`
- `pub.dev`

The goal is not to predict every transient registry failure. The goal is to give monochange enough evidence to:

- warn when a planned publish set is likely to exceed a known registry window
- batch publishes into explicit follow-up windows for CI
- distinguish strong official/source-backed limits from conservative heuristics

## Evidence summary

| Registry    | Current monochange policy | Confidence | Evidence type                                   |
| ----------- | ------------------------- | ---------- | ----------------------------------------------- |
| `crates.io` | `10 publishes / 60s`      | High       | Source code                                     |
| `npm`       | No numeric cap encoded    | Low        | Official workflow docs without numeric quota    |
| `jsr`       | `20 publishes / 24h`      | High       | Official docs                                   |
| `pub.dev`   | `12 publishes / 24h`      | Medium     | Official automation docs + operational guidance |

## Registry notes

### `crates.io`

- Evidence URL: <https://github.com/rust-lang/crates.io>
- Evidence kind: source code
- Confidence: high

Why monochange uses this:

- crates.io exposes its server implementation publicly.
- The upload path is source-backed, which is stronger than second-hand blog posts or forum comments.
- The codebase shows publish throttling behavior and distinguishes publish flows in a way that supports planning around a known window.

Current monochange policy:

- model `Publish` as `10` uploads per `60` seconds
- treat this as a strong, source-backed planning window for release batches

Notes:

- crates.io has more nuance than a single integer because first publish vs update paths are not identical in practice.
- monochange currently encodes the stable, easy-to-explain window needed for monorepo release planning instead of trying to mirror every branch of server behavior.

### `npm`

- Evidence URL: <https://docs.npmjs.com/trusted-publishers>
- Evidence kind: official docs
- Confidence: low

What the official docs provide:

- guidance for trusted publishing
- workflow expectations for CI-based publishing
- security and credential recommendations

What they do **not** provide clearly:

- a numeric per-package publish quota
- a documented per-minute or per-day publish window that monochange can safely enforce

Current monochange policy:

- do **not** encode a numeric publish cap
- surface npm as advisory planning only
- encourage sequential CI publishing and explicit batch execution when users want extra safety

Reasoning:

- inventing a fake hard quota would create more user pain than value
- official workflow guidance exists, but not official package-publish throttling numbers

### `jsr`

- Evidence URL: <https://jsr.io/docs/publishing-packages>
- Evidence kind: official docs
- Confidence: high

Why monochange uses this:

- jsr documents publish constraints clearly enough to support deterministic planning.
- The documented limit is straightforward to map into CI batches.

Current monochange policy:

- model `Publish` as `20` publishes per `24h`

Reasoning:

- this is an official, directly documented publish window
- it is appropriate for both planning output and optional enforcement

### `pub.dev`

- Evidence URL: <https://dart.dev/tools/pub/automated-publishing>
- Evidence kind: official docs plus operational guidance
- Confidence: medium

What the official docs provide:

- official automated publishing workflow guidance
- CI and authentication expectations

What is weaker here:

- the linked official page is about automation, not a crisp numeric quota page
- the commonly cited daily publish limit is operational knowledge rather than a prominent first-class limit in the automation docs themselves

Current monochange policy:

- model `Publish` as `12` publishes per `24h`
- mark it as medium confidence

Reasoning:

- pub.dev users consistently plan around a daily publish cap
- monochange needs a conservative batching strategy for Flutter and Dart monorepos
- medium confidence communicates that this is useful planning metadata, but not as strong as jsr or crates.io source-backed evidence

## Product decisions supported by this research

These findings support the current monochange rollout:

1. **Planner first**
   - some registries have strong numeric evidence, some do not
   - a planner lets monochange expose confidence and evidence without over-promising enforcement

2. **Advisory by default**
   - npm and pub.dev do not justify blanket hard-fail behavior for every workspace
   - users can opt into enforcement where their package set and registry mix make it useful

3. **Batch output for CI**
   - windows alone are not enough
   - users need concrete package batches they can execute in later workflow runs

4. **Evidence + confidence in the report**
   - registry guidance is uneven
   - monochange should show why a limit exists and how trustworthy it is

## Future follow-up

Potential later improvements:

- track crates.io first-publish vs update nuance separately if monochange needs it
- collect stronger official pub.dev rate-limit citations if Google publishes clearer quota docs
- add ecosystem-specific backoff guidance for npm even if hard limits remain undocumented
- allow user overrides for registry policy metadata when teams have internal operational evidence
