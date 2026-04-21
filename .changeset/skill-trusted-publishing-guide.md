---
"@monochange/skill": docs
---

#### add trusted publishing setup guidance for supported registries

The packaged skill now ships a dedicated `TRUSTED-PUBLISHING.md` guide for setting up GitHub-based trusted publishing / OIDC flows across the registries that monochange supports.

**Before:** The skill explained that `publish.trusted_publishing = true` existed, but it did not show the exact registry fields or commands needed to finish setup.

**After:** The package now includes step-by-step guidance for:

- `npm` trusted publishing, including the exact `npm trust github ...` and `pnpm exec npm trust ...` commands that monochange models
- `crates.io` trusted publishing fields and the `rust-lang/crates-io-auth-action@v1` workflow pattern
- `jsr` repository linking and GitHub Actions publishing
- `pub.dev` automated publishing with repository and tag-pattern requirements

The skill README, `SKILL.md`, and `REFERENCE.md` also point agents to the new guide when they need secure package-publishing setup details.

The mdBook user guide now mirrors that content in a dedicated trusted-publishing chapter so the same setup guidance is available in both the packaged skill and the docs site.
