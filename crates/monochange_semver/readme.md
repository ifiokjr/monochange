<!-- {=monochangeSemverCrateDocs} -->

# `monochange_semver`

Semver and compatibility helpers for `monochange`.

## Responsibilities

- collect compatibility assessments from providers
- merge bump severities deterministically
- calculate direct and propagated bump severities
- provide a shared abstraction for ecosystem-specific compatibility providers

## Example

```rust
use monochange_core::BumpSeverity;
use monochange_semver::direct_release_severity;
use monochange_semver::merge_severities;

let merged = merge_severities(BumpSeverity::Patch, BumpSeverity::Minor);
let direct = direct_release_severity(Some(BumpSeverity::Minor), None);

assert_eq!(merged, BumpSeverity::Minor);
assert_eq!(direct, BumpSeverity::Minor);
```

<!-- {/monochangeSemverCrateDocs} -->
