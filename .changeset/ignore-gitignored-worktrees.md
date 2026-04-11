---
"monochange": patch
"monochange_cargo": patch
"monochange_core": patch
"monochange_dart": patch
"monochange_deno": patch
"monochange_npm": patch
---

Discovery now skips repository paths ignored by `.gitignore` and automatically ignores nested git worktrees.

Before, commands like `mc release` could walk into nested worktree directories and treat their manifests as part of the current repository scan.

After this change, `mc discover` and `mc release` ignore gitignored paths such as `.claude/worktrees/` and also skip nested worktree roots even when they are not listed in `.gitignore`.
