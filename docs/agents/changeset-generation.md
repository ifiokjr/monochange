# Changeset generation

Agent rules for creating granular, well-composed changesets that accurately describe user-facing changes across libraries, applications, CLI tools, and LSP/MCP servers.

## Philosophy

<!-- {=changesetPhilosophy} -->

**A changeset is a user-facing record of change, not a code diff summary.**

Different artifact types have different user-facing boundaries:

<!-- {/changesetPhilosophy} -->

The agent must adapt its analysis based on what type of package it is examining.

## Core principles

### Package-centric granularity

Every changeset must emanate from a specific package. Group related changes together; split unrelated changes apart.

**Good:** Single changeset describing all related changes to a package

```markdown
---
"@monochange/core": minor
---

#### add file diff preview support

Introduces unified diff output for dry-run releases.

- **`--show-diff` flag** — Include file diffs in dry-run output
- **`PreparedFileDiff` type** — Structured diff data for consumers
- **Lockfile suppression** — Lockfile changes omitted from preview
```

**Bad:** Multiple changesets for the same package describing the same feature

```markdown
# ❌ Don't do this

---

## "@monochange/core": minor

#### add --show-diff flag

---

## "@monochange/core": minor

#### add PreparedFileDiff type

---

## "@monochange/core": minor

#### suppress lockfile diffs
```

**Also bad:** One changeset mixing changes from multiple unrelated packages

```markdown
# ❌ Don't do this

---

"@monochange/core": minor "@monochange/cli": patch "@monochange/config": minor

---

#### various updates
```

### Changeset lifecycle management

<!-- {=changesetLifecycleRules} -->

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

#### Decision matrix

<!-- {=changesetLifecycleDecisionMatrix} -->

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

**Golden rule:** Create a new changeset for a genuinely new user-facing change, but collapse near-duplicate notes before finalizing. If several packages changed for the same outward reason and would otherwise produce the same release note with different names, combine them into one related multi-package changeset.

**New package rule:** When a PR introduces a new published package or crate, the first changeset for that package must use a `major` bump for the new package entry.

#### Similar-note consolidation

Do not produce several changesets that all say the same thing with only the package id swapped out.

**Bad:** cloned compatibility notes for adjacent packages

```markdown
---
monochange_github: patch
---

#### align release manifest example with package publications

---

## monochange_gitlab: none

#### align release manifest fixture with package publications

---

## monochange_gitea: none

#### align release manifest fixture with package publications
```

**Good:** one combined changeset for the shared outward change

```markdown
---
monochange_github: patch
monochange_gitlab: none
monochange_gitea: none
monochange_hosting: none
---

#### align provider and hosting release manifests with package publications

All affected provider-facing crates now use the expanded `ReleaseManifest` shape consistently.
```

#### Example workflow

**Scenario 1: Feature evolves during development**

```markdown
# Initial changeset

---

## "@monochange/core": minor

#### add config validation

Adds basic validation for monochange.toml files.
```

Later, validation expands to include schema checking:

```markdown
# ✅ Update existing (same feature, expanded scope)

---

## "@monochange/core": minor

#### add config validation with schema checking

Adds validation for monochange.toml files including:

- Basic syntax validation
- Schema conformance checking
- Helpful error messages for common mistakes
```

**Scenario 2: Feature is removed**

If the config validation feature is removed before release:

```bash
# ✅ Remove the obsolete changeset
rm .changeset/config-validation.md
```

**Scenario 3: Similar but distinct feature**

```markdown
# ✅ Create new changeset (don't update existing)

---

## "@monochange/core": minor

#### add lockfile validation

Validates Cargo.lock and package-lock.json consistency.
```

Even though both are "validation" features, they target different files and should be tracked separately.

#### MCP tool integration for lifecycle management

The `monochange_analyze_changes` tool should:

1. **Scan existing changesets** in `.changeset/` directory
2. **Compare with current code changes**
3. **Suggest actions** for each changeset:
   - "This changeset references `ConfigValidator` which no longer exists — suggest removal"
   - "New changes to config validation — suggest updating existing changeset"
   - "New lockfile validation feature — suggest creating new changeset"

**Output format:**

```json
{
	"ok": true,
	"lifecycle_analysis": {
		"existing_changesets": [
			{
				"path": ".changeset/config-validation.md",
				"status": "stale",
				"reason": "Referenced code was removed in commit abc123",
				"suggestion": "Remove this changeset"
			},
			{
				"path": ".changeset/cli-flags.md",
				"status": "update_candidate",
				"reason": "New changes expand the scope of this feature",
				"suggestion": "Update to include new --verbose flag"
			}
		],
		"new_changes": [
			{
				"package": "@monochange/core",
				"suggestion": "create_new",
				"summary": "add lockfile validation"
			}
		]
	}
}
```

### Artifact-aware detection

The agent must detect the artifact type and apply appropriate analysis:

#### Library analysis

Focus on **public API surface**:

- `pub fn`, `pub struct`, `pub enum`, `pub trait`, `pub type`, `pub const`
- Changes to function signatures (parameters, return types)
- Changes to struct/enum fields (public visibility)
- New exports in `lib.rs` or `prelude.rs`
- Breaking: removed items, changed signatures

#### Application analysis

Focus on **user interaction points**:

- Route changes (new pages, removed pages, URL changes)
- Component changes that affect rendered output
- State management that affects user-visible behavior
- API endpoints (for backend apps)
- Form handling and validation changes

#### CLI analysis

Focus on **command-line interface**:

- New commands, subcommands
- New or removed flags/options
- Changes to output format or exit codes
- Changes to configuration file format
- Interactive prompts and TUI changes

### Adaptive granularity

<!-- {=changesetGranularityRules} -->

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

### Before and after examples

Every user-facing changeset must include:

1. **What changed** — High-level description
2. **Why it matters** — Impact on users
3. **Before/after examples** — Concrete migration path when the example itself changed

When the command, config snippet, or code example is identical before and after, do **not** print it twice. Show the unchanged example once, then highlight only the changed output, behavior, or migration note.

See examples section below for detailed templates.

## Change frames

Changes must be analyzed within a specific contextual frame depending on the workflow:

| Workflow          | Change Frame                     | Base                         | Head                                  |
| ----------------- | -------------------------------- | ---------------------------- | ------------------------------------- |
| **Unstaged work** | Working directory vs last commit | `HEAD`                       | Working directory (unstaged + staged) |
| **Local branch**  | Branch vs main                   | `main` (or default)          | Current branch + unstaged             |
| **PR/MR review**  | PR vs target                     | Target branch (e.g., `main`) | PR branch                             |
| **Pre-commit**    | Staged only                      | `HEAD`                       | Staged changes only                   |

The analysis must respect the frame boundaries:

- **Unstaged files** are included in local branch analysis
- **Staged-only** mode for pre-commit hooks (don't include unstaged)
- **Merge-base** detection for long-running branches (find common ancestor)

### Frame detection

```rust
pub enum ChangeFrame {
	/// Working directory changes vs HEAD
	WorkingDirectory,
	/// Branch comparison: main..feature-branch
	BranchRange { base: String, head: String },
	/// PR comparison: target..pr-branch
	PullRequest { target: String, pr_branch: String },
	/// Staged changes only
	StagedOnly,
	/// Custom revision range
	CustomRange { base: String, head: String },
}

impl ChangeFrame {
	/// Detect the appropriate frame based on git state
	pub fn detect(repo: &Repository) -> Result<Self, FrameError> {
		// Check for PR environment variables first
		if let Some(pr_info) = detect_pr_environment() {
			return Ok(Self::PullRequest {
				target: pr_info.target_branch,
				pr_branch: pr_info.source_branch,
			});
		}

		// Check if we're on a feature branch
		let current = repo.current_branch()?;
		let default = repo.default_branch()?;

		if current != default {
			return Ok(Self::BranchRange {
				base: default,
				head: current,
			});
		}

		// Default to working directory
		Ok(Self::WorkingDirectory)
	}

	/// Get the git revision range for diff commands
	pub fn revision_range(&self) -> String {
		match self {
			Self::WorkingDirectory => "HEAD".to_string(),
			Self::BranchRange { base, head } => format!("{}...{}", base, head),
			Self::PullRequest { target, pr_branch } => format!("{}...{}", target, pr_branch),
			Self::StagedOnly => "--staged".to_string(),
			Self::CustomRange { base, head } => format!("{}...{}", base, head),
		}
	}
}
```

## Analysis pipeline

### Step 0: Determine change frame

Before analyzing changes, establish the frame:

1. Check environment variables for CI/CD (PR number, target branch)
2. Check git state (branch name, upstream tracking)
3. Accept explicit overrides from CLI/MCP parameters
4. Default to working directory vs HEAD

### Step 1: Classify the artifact

Determine the primary artifact type for each changed package:

| Detection Pattern                                   | Artifact Type          |
| --------------------------------------------------- | ---------------------- |
| `lib.rs` with `pub` exports, no `main.rs`           | Library                |
| `main.rs` + command parsing (clap, structopt, etc.) | CLI                    |
| `main.rs` + web framework (axum, actix, etc.)       | Application (backend)  |
| `index.html`, `App.tsx`, `main.dart`                | Application (frontend) |
| `Cargo.toml` with `[[bin]]` sections                | CLI (multi-binary)     |
| `Cargo.toml` with `crate-type = ["cdylib"]`         | Library (FFI)          |

### Step 2: Extract semantic changes

Use the appropriate extraction strategy:

#### Libraries: AST-based extraction

Parse the diff to extract:

```rust
pub struct ApiChange {
	pub kind: ApiChangeKind,
	pub visibility: Visibility,
	pub name: String,
	pub signature: Option<String>,
	pub doc_comment: Option<String>,
	pub is_breaking: bool,
}

pub enum ApiChangeKind {
	FunctionAdded,
	FunctionModified,
	FunctionRemoved,
	TypeAdded,
	TypeModified,
	TypeRemoved,
	TraitAdded,
	TraitModified,
	TraitRemoved,
	ConstantAdded,
	ConstantModified,
	ConstantRemoved,
}
```

#### Applications: Heuristic extraction

Analyze file paths and content patterns:

```rust
pub struct AppChange {
	pub kind: AppChangeKind,
	pub route: Option<String>,
	pub component: Option<String>,
	pub description: String,
	pub is_user_visible: bool,
}

pub enum AppChangeKind {
	RouteAdded,
	RouteRemoved,
	RouteModified,
	ComponentAdded,
	ComponentModified,
	ComponentRemoved,
	ApiEndpointAdded,
	ApiEndpointModified,
	ApiEndpointRemoved,
	StateManagementChanged,
	FormValidationChanged,
}
```

Detection heuristics:

- **Routes**: Files under `routes/`, `pages/`, or containing `Route`, `path:` patterns
- **Components**: Files under `components/`, or containing JSX/template patterns
- **APIs**: Files under `api/`, `endpoints/`, or containing handler function patterns
- **State**: Changes to stores, contexts, or state management files

#### CLI tools: Command extraction

Parse command definitions and flag structures:

```rust
pub struct CliChange {
	pub kind: CliChangeKind,
	pub command: Option<String>,
	pub flag: Option<String>,
	pub description: String,
	pub is_breaking: bool,
}

pub enum CliChangeKind {
	CommandAdded,
	CommandRemoved,
	CommandModified,
	FlagAdded,
	FlagRemoved,
	FlagModified,
	OutputFormatChanged,
	ExitCodeChanged,
	ConfigFileChanged,
}
```

Detection sources:

- Clap derive macros (`#[derive(Parser)]`, `#[command()]`, `#[arg()]`)
- Builder patterns (`Command::new()`, `.arg()`, `.subcommand()`)
- Config parsing changes
- Output/logging modifications

### Step 3: Group and categorize

Apply the adaptive granularity rules:

```rust
pub fn group_changes(
	changes: Vec<SemanticChange>,
	artifact_type: ArtifactType,
) -> Vec<ChangeGroup> {
	// Group by proximity (same module/file) and kind
	// Apply thresholds to decide summarize vs. separate
	// Mark breaking changes for separate handling
}
```

### Step 4: Generate changeset content

For each group, generate:

1. **Summary headline** — `#### add <description>`
2. **Impact description** — Why this matters to users
3. **Before/after examples** — Concrete usage patterns
4. **Migration notes** — If breaking or significant

## Configuration

Add to `monochange.toml`:

```toml
[changeset.generation]
# Detection level for different ecosystems
detection_level = "signature" # Options: "basic", "signature", "semantic"

# Override thresholds per package
[changeset.generation.thresholds]
default = { group_public_api = 3, group_internal = 5, group_ui = 3 }

[changeset.generation.thresholds."@monochange/core"]
group_public_api = 5 # Larger public API, allow more before summarizing
group_internal = 8

[changeset.generation.thresholds."my-frontend-app"]
group_ui_components = 4 # More granular UI tracking
```

## MCP tool integration

### Tool: `monochange_analyze_changes`

Analyzes a git diff and returns ecosystem-specific semantic evidence for the affected packages.

**Input:**

```json
{
	"path": "/path/to/repo",
	"frame": "main...feature-branch",
	"detection_level": "signature",
	"max_suggestions": 10
}
```

**Output:**

```json
{
	"ok": true,
	"action": "analyze_changes",
	"frame": "working directory",
	"summary": "Analyzed 1 package(s) and found 6 semantic change(s)",
	"analysis": {
		"packageAnalyses": {
			"core": {
				"packageId": "core",
				"ecosystem": "cargo",
				"changedFiles": ["Cargo.toml", "src/lib.rs"],
				"semanticChanges": [
					{
						"category": "public_api",
						"kind": "modified",
						"itemKind": "function",
						"itemPath": "greet",
						"beforeSignature": "fn greet() -> &'static str",
						"afterSignature": "fn greet(name: &str) -> String"
					},
					{
						"category": "dependency",
						"kind": "added",
						"itemKind": "dependency",
						"itemPath": "tracing"
					}
				],
				"warnings": []
			}
		},
		"warnings": []
	}
}
```

This tool intentionally surfaces **semantic diffs, not bump decisions**. Agents should inspect the returned evidence and decide how to author or update a changeset.

Current analyzer coverage includes:

- Cargo public Rust API diffs plus `Cargo.toml` dependency and manifest metadata changes
- npm-family JS/TS exported symbol diffs plus `package.json` exports, commands, dependency, and script changes
- Deno JS/TS exported symbol diffs plus `deno.json` exports, import aliases, task, and compiler-option changes
- Dart and Flutter public `lib/` API diffs plus `pubspec.yaml` executables, dependency, environment, and Flutter plugin-platform changes

### Tool: `monochange_change` (enhanced)

Add `auto_analyze` parameter:

```json
{
	"path": "/path/to/repo",
	"package": ["@monochange/core"],
	"bump": "minor",
	"reason": "add ChangelogFormat enum",
	"auto_analyze": true,
	"analyzed_changes": null,
	"lifecycle_action": "create" // Options: "create", "update", "remove"
}
```

When `auto_analyze: true`:

1. Run analysis on current git state
2. Check existing changesets for lifecycle conflicts
3. Suggest actions (create/update/remove) for each changeset
4. Agent reviews and approves/modifies
5. Execute lifecycle actions based on approved suggestions

### Tool: `monochange_validate_changeset`

Validates that a changeset still matches the current semantic diff for the packages it targets.

The validator uses the same Cargo, npm, Deno, and Dart/Flutter analyzer registry as `monochange_analyze_changes`, so stale symbol checks and missing-item suggestions stay aligned with the semantic evidence surfaced for each ecosystem.

**Input:**

```json
{
	"path": "/path/to/repo",
	"changeset_path": ".changeset/feature.md"
}
```

**Validation checks:**

- Does the changeset still target packages that appear in the current diff?
- Does it mention any of the semantic items that actually changed?
- Does it reference backticked symbols that no longer exist in the current diff?
- Should it be refreshed because the semantic scope of the change has moved?

**Output:**

```json
{
	"ok": false,
	"valid": false,
	"lifecycle_status": "stale",
	"issues": [
		{
			"severity": "warning",
			"message": "changeset does not mention any detected semantic item for `core`",
			"suggestion": "mention one or more changed items such as `Greeter` or `greet`"
		},
		{
			"severity": "error",
			"message": "changeset references `OldGreeter` but that item was not found in the current semantic diff",
			"suggestion": "remove the stale reference or update it to match the current code change"
		}
	]
}
```

## Workflow

```
Agent detects code changes (via PR, commit, or manual trigger)
            │
            ▼
    ┌───────────────────┐
    │ 1. Analyze diff   │◄──── monochange_analyze_changes
    │    Detect artifact│
    │    types, extract │
    │    semantic changes│
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 2. Check existing │◄──── Scan .changeset/ directory
    │    changesets     │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 3. Determine      │
    │    lifecycle      │
    │    actions        │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 4. Apply          │
    │    granularity    │
    │    rules          │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 5. Generate       │
    │    suggested      │
    │    changesets     │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 6. Agent reviews  │
    │    and refines    │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 7. Execute        │◄──── monochange_change
    │    lifecycle      │    (create/update/remove)
    │    actions        │
    └─────────┬─────────┘
              │
              ▼
    ┌───────────────────┐
    │ 8. Validate       │◄──── monochange_validate_changeset
    │    (optional)     │
    └───────────────────┘
```

## Examples

### Example pattern: unchanged command, changed output

````markdown
---
"@monochange/cli": patch
---

#### update release planning output

Command:

```bash
mc publish-plan --format json
```

**Before (output):**

```json
{ "publishRateLimits": { "batches": ["private", "public"] } }
```

**After (output):**

```json
{ "publishRateLimits": { "batches": ["public"] } }
```
````

Do not repeat the same `mc publish-plan --format json` invocation in both sections.

### Example 1: Library with new API

**Diff:**

```diff
+ pub enum ChangelogFormat {
+     KeepAChangelog,
+     Monochange,
+     Custom(String),
+ }
+
+ impl ChangelogFormat {
+     pub fn from_path(path: &Path) -> Option<Self> { ... }
+     pub fn to_string(&self) -> String { ... }
+     fn parse_custom(s: &str) -> Option<String> { ... }
+ }
```

**Generated changeset:**

````markdown
---
"@monochange/core": minor
---

#### add `ChangelogFormat` enum

Introduces a new enum for supported changelog formats with automatic detection from file paths.

**Before:**

Changelog format was determined by file extension only, with no explicit type system.

**After:**

```rust
use monochange_core::ChangelogFormat;

// Auto-detect from path
let fmt = ChangelogFormat::from_path(Path::new("CHANGELOG.md"));

// Or construct directly
let fmt = ChangelogFormat::KeepAChangelog;
```
````

**Migration:** No action required. Existing behavior is preserved with `ChangelogFormat::KeepAChangelog` as the default.

`````
### Example 2: Lifecycle - Feature removed

**Scenario:** A changeset exists for a feature that was later reverted.

```bash
# Check existing changesets
$ ls .changeset/
feature-x.md  config-update.md

# Analyze current state
$ monochange analyze --check-existing
# Output: "feature-x.md references code removed in commit abc123"

# ✅ Remove stale changeset
$ rm .changeset/feature-x.md
```

### Example 3: Lifecycle - Feature expanded

**Initial changeset:**

```markdown
---
"@monochange/cli": minor
---

#### add --verbose flag

Adds verbose output for debugging.
```

**New changes add --debug flag:**

```markdown
# ✅ Update existing (same feature area)
---
"@monochange/cli": minor
---

#### add --verbose and --debug flags

Adds verbose and debug output options:
- `--verbose` — Show detailed progress information
- `--debug` — Show internal state and timing data
```

### Example 4: Lifecycle - Distinct features

**Scenario:** Two validation features in the same package.

```markdown
# Existing changeset
---
"@monochange/core": minor
---

#### add config validation

Validates monochange.toml syntax.
```

**New lockfile validation feature:**

```markdown
# ✅ Create NEW changeset (don't update existing)
---
"@monochange/core": minor
---

#### add lockfile validation

Validates Cargo.lock and package-lock.json consistency.
```

Even though both are "validation", they target different files and should be tracked separately for granular release notes.

### Example 5: Breaking change (separate changeset)

**Diff:**

```diff
- pub struct WorkflowDefinition {
-     pub name: String,
-     pub steps: Vec<WorkflowStep>,
- }

+ pub struct CliCommandDefinition {
+     pub name: String,
+     pub steps: Vec<CliStep>,
+     pub inputs: Vec<CliInput>,
+ }
```

**Generated changeset (separate from other changes):**

````markdown
---
"@monochange/config": major
---

#### rename `WorkflowDefinition` to `CliCommandDefinition`

**Breaking change** — The configuration structure for CLI commands has been renamed and restructured.

**Before:**

```rust
use monochange_config::WorkflowDefinition;

let cmd: WorkflowDefinition = config.workflows[0].clone();
for step in &cmd.steps { ... }
```
`````

**After:**

```rust
use monochange_config::CliCommandDefinition;

let cmd: CliCommandDefinition = config.cli[0].clone();
for step in &cmd.steps { ... }
for input in &cmd.inputs { ... }  // New: input definitions
```

**Migration:**

1. Replace all `WorkflowDefinition` imports with `CliCommandDefinition`
2. Update config file references from `workflows` to `cli`
3. Review step definitions — some fields may have changed

`````
## Edge cases

### Mixed artifact types in one package

Some packages contain both library and binary targets. Analyze based on the primary usage:

- If `lib.rs` exists → treat as library (public API focus)
- If only `main.rs` exists → treat based on content (CLI vs. app)

Create separate changesets for lib vs. bin changes if both exist:

```markdown
---
"@monochange/cli": minor  # Library changes
---

#### add `ReleasePlan` serialization support

...

---
"@monochange/cli": patch  # Binary changes
---

#### improve error message for missing config file

...
```

### Propagated changes

When a package is auto-patched due to dependency updates:

1. Skip creating a changeset if only version bump (no code changes)
2. Create minimal changeset if there are meaningful changes:

```markdown
---
"@monochange/dependent": patch
---

#### update dependency on `@monochange/core`

Updated to use latest `@monochange/core` v2.1.0 with the new `ChangelogFormat` API. No direct API changes in this package.
```

### Configuration-only changes

Changes to `monochange.toml` or other config files:

````markdown
---
"@monochange/workspace": patch
---

#### add per-package changelog format override

Adds support for `[package.<name>.changelog]` configuration section, allowing individual packages to override the default changelog format.

**Before:**

All packages used the workspace default format.

**After:**

```toml
[defaults.changelog]
format = "keep_a_changelog"

[package.core.changelog]
format = "monochange" # Override for this package only
```
`````

### Multiple features in one PR

When a PR contains multiple unrelated features:

1. Analyze each feature separately
2. Create distinct changesets for each
3. Avoid grouping unrelated changes

```markdown
# ✅ Good: Separate changesets for separate features

---

## "@monochange/core": minor

#### add file diff preview

...

---

## "@monochange/core": minor

#### add changelog format detection

...

---

## "@monochange/cli": patch

#### improve error messages

...
```

## Validation checklist

Before finalizing changesets, verify:

- [ ] Each changeset targets exactly one package (or logical group)
- [ ] Breaking changes have their own changeset with migration guide
- [ ] Before/after examples are syntactically valid for the ecosystem
- [ ] Summary describes user impact, not implementation detail
- [ ] Details explain why the change matters and how to use it
- [ ] Artifact type was correctly identified (lib/app/cli)
- [ ] Grouped changes are truly related (same feature/area)
- [ ] No unrelated changes are bundled together
- [ ] Propagated changes are appropriately marked or skipped
- [ ] **Existing changesets were reviewed for staleness**
- [ ] **Lifecycle actions (create/update/remove) are appropriate**
- [ ] **Granular approach: err on the side of creating new changesets**

## Integration with existing rules

This document extends and works alongside:

- **[changeset-quality.md](changeset-quality.md)** — Content standards and examples
- **[product-rules.md](product-rules.md)** — Architecture and crate organization
- **[coding-style.md](coding-style.md)** — Code formatting and structure

Always cross-reference when generating changesets to ensure quality.

## Key lifecycle principles

1. **Err on the side of creating new changesets** — It's easier to consolidate than split
2. **Remove stale changesets promptly** — Don't release notes for removed features
3. **Update existing only for same feature expansion** — Keep related changes together
4. **Create separate for distinct features** — Even if they touch the same package
5. **Review existing changesets before creating new ones** — Always check for conflicts

Remember: **Granularity is preferred. When in doubt, create a new changeset.**
