---
"@monochange/skill": docs
---

#### refresh crates.io and pub.dev trusted publishing guidance

The packaged trusted-publishing guide now includes more complete GitHub/OIDC setup details for `crates.io` and `pub.dev`.

**Updated guidance includes:**

- crates.io prerequisites, workflow filename handling, environment matching, and the `rust-lang/crates-io-auth-action@v1` release-job pattern
- crates.io notes about the short-lived publish token flow and first-publish bootstrap requirements
- pub.dev prerequisites, tag-push-only requirements, recommended reusable `dart-lang/setup-dart` workflow usage, optional GitHub environment hardening, and multi-package repository guidance

The mdBook trusted-publishing chapter was updated to mirror the same information.
