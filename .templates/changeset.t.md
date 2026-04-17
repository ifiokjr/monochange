<!-- {@changesetPhilosophy} -->

**A changeset is a user-facing record of change, not a code diff summary.**

Different artifact types have different user-facing boundaries:

<!-- {/changesetPhilosophy} -->

<!-- {@changesetArtifactTypeTable} -->

| Artifact Type   | User-Facing Changes                                                      | Internal Changes                               |
| --------------- | ------------------------------------------------------------------------ | ---------------------------------------------- |
| **Library**     | Public API signatures, types, traits, module exports                     | Private functions, internal refactoring        |
| **Application** | UI behavior, user workflows, navigation, visual design                   | Internal state management, component structure |
| **CLI tool**    | Commands, flags, output format, exit codes, prompts                      | Internal command dispatch, error handling      |
| **LSP / MCP**   | Protocol methods, capability declarations, tool schemas, response shapes | Internal message routing, transport layer      |

<!-- {/changesetArtifactTypeTable} -->

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

<!-- {@changesetArtifactTypeLibrary} -->

Libraries expose a public API surface. Changesets should focus on what consumers of the library will experience:

**Breaking changes (major bump):**

- New published library package or crate is introduced
- Public function removed or renamed
- Public type removed or has fields removed
- Public trait signature changed
- Public constant removed or changed type
- Module removed from public API

**New features (minor bump):**

- New public function, type, trait, or constant added
- New module added to public API
- Public function gains optional parameters (non-breaking)
- New trait implementation on existing type

**Patches (patch bump):**

- Bug fix in public function behavior
- Documentation improvement for public API
- Performance improvement with no API change
- Internal refactoring with no public-facing impact

**When to create vs. update:**

- Each new public addition → create a new changeset
- Each new published package or crate → create a new changeset with a `major` bump for that new package
- If a function was added then modified before release → update the existing changeset
- If a function was added then removed before release → delete the changeset

<!-- {/changesetArtifactTypeLibrary} -->

<!-- {@changesetArtifactTypeApplication} -->

Applications expose user-facing behavior through UI, navigation, and interaction design. Changesets should describe what users see and do:

**Breaking changes (major bump):**

- Route removed or URL structure changed without redirect
- User workflow significantly altered
- Feature removed that users depend on
- API endpoint removed or changed without versioning

**New features (minor bump):**

- New page, route, or screen added
- New interactive component added
- New API endpoint exposed
- New user-facing setting or preference

**Patches (patch bump):**

- Bug fix in UI behavior
- Copy or text improvement
- Accessibility improvement
- Performance improvement in page load or interaction

**UX changelog section:**

Applications and websites should configure a `ux` changelog section type for changes that affect the user experience visually or interactively. This includes:

- Visual redesigns or layout changes
- New screenshots or UI mockups
- Interaction pattern changes (drag-and-drop, gestures, keyboard shortcuts)
- Accessibility improvements visible to users
- User flow changes (onboarding, checkout, navigation)

Configure the section in `monochange.toml`:

```toml
extra_changelog_sections = [
	{ name = "User Experience", types = ["ux"], default_bump = "minor" },
]
```

Use `--type ux` when creating changesets:

```bash
mc change --package web-app --bump minor --type ux --reason "redesign settings page navigation"
```

**Screenshot support:**

For user-facing changes, include screenshots or screen recordings in the changeset details. The project should configure an S3-compatible upload service in `monochange.toml` for hosting images:

```toml
[defaults.screenshots]
provider = "s3"
bucket = "changelog-screenshots"
region = "us-east-1"
path_prefix = "{{ package_id }}/{{ version }}/"
public_url_template = "https://cdn.example.com/{{ path }}"
```

When screenshots are configured, reference them in changeset details using relative paths or markdown image syntax:

```markdown
#### redesign settings page navigation

Settings page now uses a tab-based layout instead of accordion sections.

![New settings navigation](settings-tabs-redesign.png)

- Tab-based navigation replaces accordion sections
- Search field is now persistent across all tabs
- Mobile layout stacks tabs vertically
```

<!-- {/changesetArtifactTypeApplication} -->

<!-- {@changesetArtifactTypeCli} -->

CLI tools expose commands, flags, output format, and exit codes. Changesets should focus on what developers and automation scripts will experience:

**Breaking changes (major bump):**

- Command removed or renamed
- Flag removed or changed meaning
- Default output format changed
- Exit code semantics changed
- Configuration file format has incompatible changes

**New features (minor bump):**

- New command added
- New flag or option added
- New output format added (e.g., `--format json`)
- New configuration file option
- New interactive prompt or autocompletion

**Patches (patch bump):**

- Bug fix in command behavior
- Error message improvement
- Performance improvement
- Documentation improvement for command usage

**When to create vs. update:**

- Each new command or flag → new changeset
- If a command was added then renamed before release → update the existing changeset
- If a command was added then removed before release → delete the changeset

**Agent-focused changes:**

CLI tools used by agents (like `mc` itself) should document changes that affect automation workflows:

- New or changed command exit codes
- New or changed output formats (`--format json`, structured output)
- New or changed MCP tool schemas
- New or changed configuration options that affect behavior

<!-- {/changesetArtifactTypeCli} -->

<!-- {@changesetArtifactTypeLspMcp} -->

LSPs and MCPs expose protocol methods, capability declarations, tool schemas, and response shapes. Changesets should focus on what client integrations will experience:

**Breaking changes (major bump):**

- Protocol method removed or has changed signature
- Tool schema field removed or changed type
- Capability declaration removed or changed semantics
- Response shape has fields removed or renamed
- Required field added to request schema

**New features (minor bump):**

- New protocol method or notification added
- New tool or resource added
- New capability declared
- New optional field added to response schema
- New notification type added

**Patches (patch bump):**

- Bug fix in protocol method behavior
- Documentation improvement for tool schemas
- Performance improvement in response time
- Error message improvement in diagnostics

**Developer-focused changes:**

LSP and MCP servers serve developers and their tools. Changesets should emphasize:

- How the integration surface changes
- What client code needs to update
- Whether the change is backward compatible

**Example changeset for an MCP tool addition:**

```markdown
---
monochange: minor
---

#### add `monochange_validate_changeset` MCP tool

Introduces a new MCP tool for validating changeset content against the current semantic diff.

- **Tool name:** `monochange_validate_changeset`
- **New parameters:** `path`, `changeset_path`
- **Returns:** validation result with severity-tagged issues and lifecycle status
- **Current semantic support:** Cargo, npm, Deno, and Dart/Flutter packages
- **Backward compatible:** existing tools unchanged
```

<!-- {/changesetArtifactTypeLspMcp} -->

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

<!-- {@changesetTemplateLibrary} -->

```markdown
---
{ { package_id } }: minor
---

#### add {{feature_name}}

{{one_sentence_summary}}.

- **`{{symbol}}`** — {{what_it_does}} {{#each additional_symbols}}
- **`{{symbol}}`** — {{what_it_does}} {{/each}}

**Before:** {{before_state}}

**After:** {{after_state}}
```

<!-- {/changesetTemplateLibrary} -->

<!-- {@changesetTemplateApplication} -->

```markdown
---
{ { package_id } }: minor
---

#### add {{feature_name}}

{{one_sentence_summary}}.

<!-- TYPE: ux -->

{{#if has_screenshots}} ![{{screenshot_alt}}]({{screenshot_url}}) {{/if}}

- {{what_changed}} {{#each additional_changes}}
- {{what_changed}} {{/each}}

**Before:** {{before_state}}

**After:** {{after_state}}
```

<!-- {/changesetTemplateApplication} -->

<!-- {@changesetTemplateCliLspMcp} -->

```markdown
---
{ { package_id } }: minor
---

#### add {{feature_name}}

{{one_sentence_summary}}.

- **{{tool_or_command}}:** `{{name}}` — {{what_it_does}} {{#each additional_items}}
- **{{tool_or_command}}:** `{{name}}` — {{what_it_does}} {{/each}}

**Before:** {{before_state}}

**After:** {{after_state}}

**Integration impact:** {{backward_compat_note}}
```

<!-- {/changesetTemplateCliLspMcp} -->

<!-- {@changesetUxSectionConfig} -->

For applications and websites, configure a `ux` changelog section in `monochange.toml`:

```toml
# Per-package configuration for an application
[package.web-app]
path = "apps/web"
type = "cargo"

[package.web-app.extra_changelog_sections]
ux = { name = "User Experience", types = ["ux"], default_bump = "minor", description = "Visual and interaction changes that affect the user experience" }
```

Or at the group level:

```toml
[group.main]
extra_changelog_sections = [
	{ name = "User Experience", types = ["ux"], default_bump = "minor", description = "Visual and interaction changes that affect the user experience" },
	{ name = "Testing", types = ["test"], default_bump = "none", description = "Changes that only modify tests" },
	{ name = "Documentation", types = ["docs"], default_bump = "none", description = "Changes that only modify documentation" },
]
```

When creating a changeset for a UX change, use `--type ux`:

```bash
mc change --package web-app --bump minor --type ux --reason "redesign settings page navigation"
```

<!-- {/changesetUxSectionConfig} -->

<!-- {@changesetUxScreenshots} -->

For visual changes, include screenshots in changeset details. Configure an S3-compatible upload service in `monochange.toml`:

```toml
[defaults.screenshots]
provider = "s3"
bucket = "changelog-screenshots"
region = "us-east-1"
path_prefix = "{{ package_id }}/{{ version }}/"
public_url_template = "https://cdn.example.com/{{ path }}"
```

Reference uploaded screenshots in changeset markdown:

```markdown
![New settings navigation](https://cdn.example.com/web-app/2.3.0/settings-tabs.png)
```

Or use relative paths if screenshots are committed alongside changesets:

```markdown
![New settings navigation](.changeset/screenshots/settings-tabs-redesign.png)
```

<!-- {/changesetUxScreenshots} -->

<!-- {@changesetLifecycleSteps} -->

Before creating any changeset, follow these steps:

1. **Read all existing changesets** in `.changeset/` to understand current coverage
2. **Identify which packages are affected** by the current diff
3. **For each affected package**, determine:
   - Does an existing changeset already describe this change? → **Update it**
   - Does an existing changeset describe a removed feature? → **Remove it**
   - Is this a genuinely new change? → **Create a new changeset**
4. **Classify each change** by artifact type to choose the right bump level and section type
5. **Write the changeset** following the artifact-type template
6. **Validate** by running `mc validate` or `mc diagnostics --format json`

When removing a changeset for a reverted feature, delete the file entirely — do not leave empty or zero-bump changesets as they create noise in release notes.

<!-- {/changesetLifecycleSteps} -->

<!-- {@changesetAnalysisMcpTools} -->

The `monochange_analysis` crate provides MCP tools for semantic diff inspection and changeset validation.

`monochange_analyze_changes` and `monochange_validate_changeset` now return real semantic analysis for Cargo, npm, Deno, and Dart/Flutter packages. They surface ecosystem-specific evidence such as Rust public API diffs, JS/TS export changes, `package.json` and `deno.json` export metadata, and `pubspec.yaml` dependency or plugin-platform changes.

**`monochange_analyze_changes`** — Analyzes git diffs and returns granular semantic changes for affected packages:

```json
{
	"path": "/path/to/repo",
	"frame": "main...feature-branch",
	"detection_level": "signature",
	"max_suggestions": 10
}
```

Current output includes:

- Cargo public Rust API diffs plus `Cargo.toml` dependency and manifest metadata changes
- npm-family JS/TS exported symbol diffs plus `package.json` exports, commands, dependency, and script changes
- Deno JS/TS exported symbol diffs plus `deno.json` exports, import aliases, task, and compiler-option changes
- Dart and Flutter public `lib/` API diffs plus `pubspec.yaml` executables, dependency, environment, and plugin-platform changes

This tool intentionally **does not decide** whether the diff is patch/minor/major. It returns semantic evidence for the agent to interpret.

**`monochange_validate_changeset`** — Validates that a changeset matches the current semantic diff:

```json
{
	"path": "/path/to/repo",
	"changeset_path": ".changeset/feature.md"
}
```

The validator uses the same Cargo, npm, Deno, and Dart/Flutter analyzer registry as `monochange_analyze_changes`, so stale symbol checks and missing-item suggestions stay aligned with the semantic evidence surfaced for each ecosystem.

**Lifecycle integration:**

These tools integrate with the changeset lifecycle:

- Use `monochange_analyze_changes` to inspect semantic evidence before creating or updating a changeset
- Use `monochange_validate_changeset` to catch stale symbol references or underspecified summaries
- Treat the returned semantic model as evidence for the agent, not an automatic bump decision

<!-- {/changesetAnalysisMcpTools} -->

<!-- {@changesetCausedByField} -->

### Dependency propagation with `caused_by`

When a dependency changes, monochange automatically patches all dependents. This creates release notes with no context for _why_ the dependent is being updated.

The `caused_by` field in changeset frontmatter provides that context. It lists the root package(s) or group(s) that triggered this dependent change. Because `caused_by` is part of the object form, use table syntax instead of scalar shorthand whenever you need it:

```markdown
---
monochange_config:
  bump: patch
  caused_by: ["monochange_core"]
---

#### update dependency on monochange_core

Bumps `monochange_core` dependency to v2.1.0 after the public API change to `ChangelogFormat`.
```

**How it works:**

1. Without `caused_by`: a dependent gets an automatic "dependency changed → patch" record with no explanation
2. With `caused_by`: the authored changeset **replaces** the matching automatic propagation — it provides human-readable context instead
3. A changeset with `caused_by` and `bump: patch` suppresses the automatic "dependency changed → patch" record for that package when the same upstream package or group triggered it
4. A changeset with `caused_by` and `bump: none` suppresses that matching propagation entirely — the package is acknowledged as affected but no version bump is warranted
5. Unrelated upstream sources can still propagate normally; `caused_by` is not a global opt-out for every dependency edge

**`none` bump with `caused_by` — the "nothing meaningful changed" case:**

When `mc affected` flags a package but the change is not meaningful (just a lockfile update or a re-export), use `bump: none` with `caused_by`:

```markdown
---
monochange_config:
  bump: none
  caused_by: ["monochange_core"]
  type: deps
---

#### update monochange_core dependency

No user-facing changes. Dependency version updated to match the group release.
```

This tells monochange: "this package is affected, but the change doesn't warrant a version bump for consumers. Suppress the matching automatic patch propagation entirely."

CLI authoring supports one or more `--caused-by` flags:

- `mc change --package <id> --bump patch --caused-by monochange_core --reason "update dependency"`
- `mc change --package <id> --bump none --caused-by sdk --reason "dependency-only follow-up"`

<!-- {/changesetCausedByField} -->
