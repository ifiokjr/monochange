---
"@monochange/skill": minor
monochange: patch
---

#### update skill and docs to reflect current features

The bundled agent skill (`SKILL.md` and `REFERENCE.md`) has been comprehensively updated to cover all current monochange features:

- **CLI commands**: added `mc init`, `mc diagnostics`, `mc assist`, `mc mcp`, `mc commit-release`, `mc release-pr`, `mc affected`, and `mc repair-release` to the command reference table
- **CLI step types**: documented all 13 step types (`Validate`, `Discover`, `CreateChangeFile`, `PrepareRelease`, `CommitRelease`, `RenderReleaseManifest`, `PublishRelease`, `OpenReleaseRequest`, `CommentReleasedIssues`, `AffectedPackages`, `DiagnoseChangesets`, `RetargetRelease`, `Command`) with prerequisite guidance
- **MCP tools**: replaced the outdated "planned MCP setup" placeholder with the actual 6 implemented tools (`monochange_validate`, `monochange_discover`, `monochange_change`, `monochange_release_preview`, `monochange_release_manifest`, `monochange_affected_packages`)
- **CLI step composition**: added input types (`string`, `string_list`, `path`, `choice`, `boolean`), `Command` step template interpolation, `shell` options, and step output references
- **Changeset authoring**: documented object syntax with `bump`, `version`, and `type` fields
- **Configuration**: added regex `versioned_files`, lockfile commands, `release_title`/`changelog_version_title` templates, group changelog `include` filters, and `[changesets.verify]`

The assistant setup guide (`docs/src/guide/09-assistant-setup.md`) now uses the correct MCP tool name (`monochange_affected_packages` instead of the outdated `monochange_verify_changesets`).

Six new reusable mdt snippets (`mcpToolsList`, `mcpConfigSnippet`, `recommendedCommandFlow`, `assistantRepoGuidance`, `cliStepTypes`, `releaseTitleConfig`) are shared between the skill files and docs book so these sections stay in sync automatically.

The `monochange.init.toml` template now documents `release_title` and `changelog_version_title` with full placeholder reference and migration guidance for the breaking changelog heading format change.
