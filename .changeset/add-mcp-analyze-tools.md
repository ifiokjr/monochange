---
monochange: minor
---

#### add MCP tools for change analysis and validation

Introduces two new MCP tools to help agents generate and validate changesets programmatically:

**`monochange_analyze_changes`**

Analyzes git diffs and suggests granular changeset structure. Supports multiple detection modes:

```json
{
  "path": "/path/to/repo",
  "frame": "main...feature-branch",
  "detection_level": "signature",
  "max_suggestions": 10
}
```

The tool automatically detects:
- Which packages have changes
- What type of artifact each package is (library, app, CLI)
- Semantic changes (new functions, modified components, etc.)
- Appropriate grouping based on configurable thresholds

**`monochange_validate_changeset`**

Validates that a changeset accurately describes the actual code changes:

```json
{
  "path": "/path/to/repo",
  "changeset_path": ".changeset/feature.md"
}
```

Checks:
- Does the summary match the actual diff content?
- Is the bump level appropriate for the change type?
- Are there undocumented API changes?

**Before:**

Agents had to manually inspect diffs and decide what changesets to create.

**After:**

```bash
# Start MCP server
mc mcp

# Then use the analyze_changes tool to get suggestions
# for all packages with modifications
```

These tools integrate with the new `monochange_analysis` crate to provide intelligent, context-aware changeset recommendations.