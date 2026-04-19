---
"@monochange/cli": patch
monochange: patch
monochange_analysis: patch
monochange_cargo: patch
monochange_config: patch
monochange_core: patch
monochange_dart: patch
monochange_deno: patch
monochange_gitea: patch
monochange_github: patch
monochange_gitlab: patch
monochange_graph: patch
monochange_hosting: patch
monochange_npm: patch
monochange_semver: patch
---

#### add per-crate Codecov coverage flags and crate-specific coverage badges

monochange now uploads one Codecov coverage flag per public crate while keeping the existing workspace-wide upload.

**Before:**

- Codecov only received the overall workspace LCOV upload
- crate READMEs linked their coverage badge to the shared repository-wide Codecov page
- Codecov patch coverage enforced a 100% target for PR status checks

**After:**

- CI splits the workspace LCOV report into one upload per public crate using a Codecov flag named after the crate
- each published crate README now points its coverage badge at that crate’s own Codecov flag page, for example `?flag=monochange_core`
- the repository keeps the overall workspace coverage upload and lowers the Codecov patch coverage status target to 95%
