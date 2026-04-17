---
"@monochange/skill": minor
---

#### reorganize bundled skill docs into lowercased skill paths

Moves the bundled deep-dive markdown files under `skills/` and lowercases the published markdown filenames so the installable skill has a more consistent layout.

**Updated structure includes:**

- top-level `SKILL.md` remains the entrypoint
- deep-dive guides such as `reference.md`, `changeset-guide.md`, `artifact-types.md`, `trusted-publishing.md`, and `multi-package-publishing.md` now live in `skills/`
- package and example readme files now use lowercase `readme.md`
- internal links and published package file paths were updated to match the new structure
