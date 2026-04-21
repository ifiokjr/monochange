---
"@monochange/skill": docs
---

#### prefer official trusted-publishing workflows in the packaged skill

The packaged skill now explicitly recommends the registry-maintained GitHub publishing workflows for manual trusted-publishing registries.

**Updated guidance:**

- prefers `rust-lang/crates-io-auth-action@v1` for `crates.io`
- prefers `dart-lang/setup-dart/.github/workflows/publish.yml@v1` for `pub.dev`
- clarifies that `mode = "external"` is often the clearest fit when those workflows should own the publish command directly

These recommendations were added to the main skill entrypoint, the configuration deep dive, and the packaged trusted-publishing guide.
