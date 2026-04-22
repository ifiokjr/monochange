# Changeset quality

A changeset is not just a version bump marker — it is a permanent record of how the project changed from the perspective of the person using it. Write changesets so a user who has never seen the source code can understand exactly what moved, why it matters, and how to adapt.

## Required content

Every changeset body must include:

1. **A headline** (`#### short title`) describing what changed in user-facing terms.
2. **An impact summary** explaining why the change matters to users or callers.
3. **Usage examples** showing the change in context (see sections below).

A one-liner that only restates the PR title is not acceptable. The body must be detailed enough that a user reading the release notes can act on the information without consulting the source diff.

## CLI changes

For any change that adds, removes, or modifies a CLI command or flag:

- Show the **exact command invocations** before and after when the invocation itself changed.
- If the command stayed the same and only the result changed, show the command **once** and only show the changed output before/after.
- Do **not** print the same command, config snippet, or other example twice when the example itself did not change.
- Show **config snippets** (`monochange.toml`) when behaviour is driven by configuration.
- Show representative **output** (text or JSON) when the output shape changes.
- Use `# before` / `# after` comments when renaming flags or restructuring commands.

The goal is to highlight the differences, not duplicate unchanged context.

Example headline for a renamed flag:

> #### rename `--changed-path` to `--changed-paths`

Example body:

> **Before:**
>
> ```bash
> mc verify --changed-path src/lib.rs --changed-path crates/core/src/main.rs
> ```
>
> **After:**
>
> ```bash
> mc verify --changed-paths src/lib.rs crates/core/src/main.rs
> ```
>
> `--changed-path` is kept as a hidden alias for one release cycle.

When the invocation is unchanged but the output changes, prefer a structure like this:

> #### update `mc publish-plan --format json` batch filtering
>
> Command:
>
> ```bash
> mc publish-plan --format json
> ```
>
> **Before (output):**
>
> ```json
> { "publishRateLimits": { "batches": ["private", "public"] } }
> ```
>
> **After (output):**
>
> ```json
> { "publishRateLimits": { "batches": ["public"] } }
> ```
>
> Do not repeat the same `mc publish-plan --format json` command in both sections.

When a command is **removed**, explain what users should do instead:

> #### remove `mc deploy` command
>
> Use your CI platform's native deployment triggers (e.g. a GitHub Actions `workflow_run` event on the release workflow) instead of the `Deploy` workflow step.

## Library API changes

For any change that adds, modifies, or removes a public type, function, or trait in a published crate:

- Show the **type signature or struct definition** before and after.
- Use `// before` / `// after` comments for inline diffs where a full block is too long.
- For renamed items, show the old name struck out and the new name.
- For removed items, show the replacement or migration path.

Example body for a renamed type:

> #### rename `WorkflowDefinition` to `CliCommandDefinition`
>
> **Before (`monochange_config`):**
>
> ```rust
> use monochange_config::WorkflowDefinition;
> let cmd: WorkflowDefinition = config.workflows[0].clone();
> ```
>
> **After:**
>
> ```rust
> use monochange_config::CliCommandDefinition;
> let cmd: CliCommandDefinition = config.cli[0].clone();
> ```

For new APIs, show a minimal but realistic usage example:

> #### add `ChangelogFormat` enum to `monochange_core`
>
> ```rust
> use monochange_core::ChangelogFormat;
>
> let fmt = ChangelogFormat::KeepAChangelog;
> assert_eq!(fmt.to_string(), "keep_a_changelog");
> ```

## Configuration changes

For new or changed `monochange.toml` keys, always show the TOML before and after when the TOML itself changed.

If the config snippet is identical before and after, do not duplicate it. Show the unchanged config once, then show the changed output or behaviour instead.

> **Before (no per-package format override):**
>
> ```toml
> [defaults.changelog]
> path = "{{ path }}/CHANGELOG.md"
> ```
>
> **After:**
>
> ```toml
> [defaults.changelog]
> path = "{{ path }}/CHANGELOG.md"
> format = "keep_a_changelog"
>
> [package.core.changelog]
> format = "monochange" # overrides the default for this package
> ```

## Breaking changes

Any change that requires callers to update their code, config, or workflows must:

- Open the body with a `> **Breaking change**` blockquote.
- List every removed or incompatibly changed item.
- Give a concrete migration path for each.

Example:

> **Breaking change** — `[[workflows]]` config is no longer accepted.
>
> Rename every `[[workflows]]` table to `[cli.<command>]` and move `[[workflows.steps]]` entries to `[[cli.<command>.steps]]`.

## GUI / app changes

For graphical or browser-based interfaces, embed a screenshot or screen recording link when one is available. If screenshots are not feasible, describe the visual change in enough detail that a user can identify the affected UI element and understand what it looks like now.

Example:

> #### add release summary panel to dashboard
>
> A collapsible **Release summary** card now appears at the top of the project page after a release run completes. It lists each published package, the new version, and a link to the corresponding changelog entry.
>
> ![Release summary panel](docs/screenshots/release-summary-panel.png)

## What counts as too short

Reject or expand a changeset if its body matches any of these patterns:

- A single sentence that only restates the headline.
- "Internal refactor with no user-visible changes" with no evidence.
- A list of file names or function names with no explanation of user impact.
- A PR title copy-pasted verbatim.

The bar is: _could a user who has never seen this repository understand what changed, whether it affects them, and what to do about it?_ If not, the changeset needs more detail.
