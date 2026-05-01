---
monochange_github: patch
monochange_hosting: patch
---

# Verified release PR commits and heading hierarchy fix

## Verified release PR commits

Release PR commits for GitHub now create fresh blobs and trees via the GitHub Git Database API instead of reusing the tree of the original unsigned commit. This ensures the replacement commit is properly signed and verified.

The implementation reads tracked files from disk, detects file modes (regular, executable, symlink), and creates new blobs for each file before assembling a fresh tree. It also correctly handles root commits that have no parents.

## Heading hierarchy fix

The PR body section titles were rendering at `####` (h4), which made them siblings to individual change entries. They now render at `###` (h3), restoring the correct heading hierarchy.
