---
"@monochange/skill": docs
---

#### add changeset cleanup job guide with mc diagnostics workflow

Adds a comprehensive "Changeset cleanup job" section to `skills/changesets.md` that teaches agents how to audit, deduplicate, and clean up changesets before release using `mc diagnostics --format json`.

**New workflow includes:**

- Step-by-step guide using `mc diagnostics --format json` to export changeset data
- jq filter examples for finding duplicates, short summaries, missing git context
- Decision matrix for when to merge, remove, or update changesets
- Concrete bash examples for merging duplicate changesets
- Validation checklist for pre-release changeset hygiene

Updates the root `SKILL.md` reference to highlight "auditing, cleaning up" alongside creation.
