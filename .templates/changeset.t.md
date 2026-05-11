<!-- {@changesetPhilosophy} -->

**A changeset is a user-facing record of change, not a code diff summary.**

Different artifact types have different user-facing boundaries:

<!-- {/changesetPhilosophy} -->

<!-- {@changesetLifecycleRules} -->

As features are added and removed, changesets must be actively managed throughout the development lifecycle:

1. **Analyze existing changesets** before creating new ones — read every `.changeset/*.md` file and understand what each covers
2. **Determine the appropriate action** for each change:
   - **Create new** — For genuinely new changes (preferred)
   - **Update existing** — When expanding the scope of a change already described
   - **Remove obsolete** — When the feature was reverted or the change no longer exists
   - **Replace** — When the same intent is now implemented differently

**Golden rule:** Err on the side of creating a new changeset. It's easier to consolidate later than to split apart.

**New package rule:** When a PR introduces a new published package or crate, the first changeset for that package must use a `major` bump for the new package entry.

<!-- {/changesetLifecycleRules} -->

<!-- {@changesetLifecycleDecisionMatrix} -->

| Scenario                          | Action                   | Rationale                                       |
| --------------------------------- | ------------------------ | ----------------------------------------------- |
| New feature added                 | **Create new**           | Granular tracking of distinct changes           |
| New published package or crate    | **Create new**           | First release note should use a `major` bump    |
| Existing feature expanded         | **Update existing**      | Keep related changes together                   |
| Feature removed or reverted       | **Remove changeset**     | Don't release notes for removed features        |
| Same change, different approach   | **Replace changeset**    | Document the actual implementation              |
| Multiple small related changes    | **Create new** (grouped) | Summarize when exceeding threshold              |
| Bug found in unreleased feature   | **Update existing**      | Combine fix with feature, not a separate entry  |
| Refactor of unreleased change     | **Update existing**      | Rewrite description to reflect new structure    |
| Changeset references removed code | **Remove changeset**     | Stale changesets create confusing release notes |

<!-- {/changesetLifecycleDecisionMatrix} -->

<!-- {@changesetGranularityRules} -->

When deciding how many changesets to create for a single PR or branch:

| Change type                    | Library         | Application                 | CLI / LSP / MCP |
| ------------------------------ | --------------- | --------------------------- | --------------- |
| Single new feature             | Separate        | Separate                    | Separate        |
| Multiple related API additions | 3+ → group      | 2+ → group                  | 2+ → group      |
| Internal refactoring only      | Patch           | Patch                       | Patch           |
| Breaking + non-breaking mixed  | Separate        | Separate                    | Separate        |
| New routes/pages               | N/A             | 2+ → summarize              | N/A             |
| New commands/tools             | N/A             | N/A                         | 2+ → summarize  |
| **Documentation-only**         | 10+ → summarize | 10+ → summarize             | 10+ → summarize |
| **UX / visual changes**        | N/A             | Separate (with screenshots) | N/A             |

**Summarize** = Create a single changeset with a grouped description. **Separate** = Create individual changesets (or mark as breaking).

<!-- {/changesetGranularityRules} -->
