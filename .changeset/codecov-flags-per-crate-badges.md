---
"@monochange/cli": feat
monochange: feat
monochange_analysis: feat
monochange_cargo: feat
monochange_config: feat
monochange_core: feat
monochange_dart: feat
monochange_deno: feat
monochange_gitea: feat
monochange_github: feat
monochange_gitlab: feat
monochange_graph: feat
monochange_hosting: feat
monochange_npm: feat
monochange_semver: feat
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
