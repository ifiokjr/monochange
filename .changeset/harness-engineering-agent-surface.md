---
monochange: patch
"@monochange/skill": patch
---

#### expand the agent-facing harness around diagnostics, lint metadata, and repo guidance

`monochange` now exposes more of its review and planning surface directly through the repo and MCP so assistants can work from structured data instead of shell-only conventions.

**Before:**

- assistants could call the MCP server for validation, discovery, change creation, release previews, and affected-package checks
- lint metadata and changeset diagnostics still depended on `mc lint ...` and `mc diagnostics --format json`
- the repo guidance lived mostly in `AGENTS.md` and scattered docs without a dedicated plans directory or top-level architecture map

**After:**

- MCP now includes `monochange_diagnostics`, `monochange_lint_catalog`, and `monochange_lint_explain`
- the packaged skill and assistant setup docs now list the full MCP surface, including semantic analysis tools
- the repository now keeps an explicit `ARCHITECTURE.md` map plus `docs/plans/` for active plans, completed plans, and tech-debt tracking
- `docs:check` now verifies that the agent-facing docs stay aligned with the live MCP tool surface, and `lint:architecture` checks that provider/ecosystem dispatch stays inside the documented allowlist
