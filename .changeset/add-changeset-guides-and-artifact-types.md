---
"@monochange/skill": docs
---

#### add artifact-type-aware changeset guides and skill package expansion

Introduces three new documents to the skill package and shared mdt template blocks for changeset generation rules:

- **CHANGESET-GUIDE.md** — lifecycle management guide covering create, update, replace, and remove workflows with decision matrix
- **ARTIFACT-TYPES.md** — per-type rules for libraries, applications, CLI tools, and LSP/MCP servers, including UX changelog section configuration and screenshot support
- **`.templates/changeset.t.md`** — shared mdt template blocks for changeset philosophy, artifact tables, lifecycle rules, granularity rules, templates, and MCP tool integration

Key additions:

- **UX changelog section** (`type: ux`) for applications and websites, with S3-compatible screenshot upload configuration
- **LSP/MCP artifact type** added to the artifact type table with protocol-focused changeset guidance
- **`caused_by` frontmatter field** documented for dependency propagation context (replaces automatic "dependency changed → patch" with human-readable explanation)
- **`bump: none` with `caused_by`** workflow for `mc affected` packages with no meaningful changes
- Shared blocks propagate to `SKILL.md`, `REFERENCE.md`, and `docs/agents/changeset-generation.md` via `mdt`

**Before:** Skill package had only `SKILL.md` and `REFERENCE.md` with no artifact-type differentiation or lifecycle management guidance.

**After:** Agents can follow per-type rules, manage changeset lifecycles, configure UX sections with screenshots, and provide dependency propagation context.
