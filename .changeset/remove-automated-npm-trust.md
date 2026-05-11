---
monochange_publish: minor
monochange: minor
monochange_npm: minor
---

# Remove automated npm trust configuration during publish

Removed the `npm trust` command execution from the publish loop. Trust configuration for npm packages must now be done manually or via separate tooling — `mc publish` no longer runs `npm trust github` or `npm trust list` automatically.

When trusted publishing is enabled for npm packages, the publish command now uses `npm` directly instead of `pnpm` (already the case via `npm_publish_program`). An environment variable override for forcing pnpm during trusted publishing can be added in a future release.

Removed `PublishTrustHandler::configure_successful_publish_trust` from the trait and its `CliPublishTrustHandler` implementation. Removed `configure_npm_trusted_publishing` from `package_publish`. Removed `build_npm_trust_list_command` from `monochange_npm`. The `trust_outcome_for_skip` and `planned_trust_outcome` methods remain, showing informational messages about how to manually configure trust.
