# Changesets

Changesets are explicit release intent. They tell monochange which package or group should be considered for a version bump and what human-facing release note to render.

A good changeset answers three questions: what public behavior changed, who is affected, and how monochange should version the affected release target. It should not be a raw commit log or a list of touched files.

## CLI creation

Prefer the repository's configured workflow if present:

```bash
mc change --package @acme/api --bump minor --type feat --reason "Add webhook delivery filters"
```

If `mc change` is not configured, use the step command directly:

```bash
mc step:create-change-file --package @acme/api --bump minor --reason "Add webhook delivery filters"
```

Always run `mc validate` after creating or editing changesets.

The configured command may expose more inputs than the portable step command, such as `--type`, `--caused-by`, or repository-specific defaults. Check `mc help change` or the `[cli.change]` table before relying on a flag name.

## File shape

The frontmatter keys must be configured package ids or group ids. Quote ids that contain `@`, `/`, dots, or other punctuation so the YAML/TOML-like frontmatter is unambiguous.

Simple package-to-bump syntax:

```md
---
"@acme/api": minor
---

# Add webhook delivery filters

Users can now filter webhook deliveries by event type and delivery status.
```

When using configured changelog types, the type can be the target value when it maps to the desired default bump:

```md
---
"@acme/api": feat
---

# Add webhook delivery filters

Users can now filter webhook deliveries by event type and delivery status.
```

The shorthand above is compact, but only use it when the configured changelog type already implies the intended bump. If you need to override the bump, pin a version, or attach dependency context, switch to object syntax.

Use object syntax when you need `bump`, `type`, `version`, or `caused_by` together:

```md
---
"@acme/api":
  bump: minor
  type: feat
---

# Add webhook delivery filters

Users can now filter webhook deliveries by event type and delivery status.
```

Multiple targets are allowed when one user-facing change spans packages:

```md
---
"@acme/api":
  bump: patch
  type: fix
"@acme/ui":
  bump: patch
  type: fix
---

# Preserve dashboard filters after retrying requests

Both the API response and the UI retry flow now keep the same filter state.
```

Use explicit versions only when you need a specific version rather than semver bump calculation:

```md
---
"@acme/api":
  bump: minor
  type: feat
  version: "2.5.0"
---

# Stabilize webhook filter endpoints
```

## `caused_by`

Use `caused_by` when a package is affected because another package changed.

```md
---
"@acme/ui":
  bump: none
  type: none
  caused_by: ["@acme/api"]
---

# Rebuild UI package for API dependency metadata

No user-facing UI behavior changed.
```

`caused_by` can reference package ids or group ids. In CLI form, pass repeated `--caused-by <id>` flags when the configured workflow exposes that input.

Use `caused_by` to explain propagation instead of pretending a dependent package has its own feature or fix. This keeps changelogs honest while still preserving enough metadata for release planning and policy checks.

## Bump rules

- `major` — breaking API, CLI, protocol, data, or user workflow changes.
- `minor` — new user-facing functionality or behavior.
- `patch` — fixes and compatible improvements.
- `none` — documentation, tests, rebuilds, or dependency/context notes with no version impact.

Breaking changes should have their own changeset with migration guidance.

When in doubt, choose the bump based on the user's or integrator's experience, not on implementation size. A one-line removal from a public API is usually `major`; a large internal refactor can be `none` if no published behavior changes.

## Lifecycle rules

Before adding a new changeset:

1. Read existing `.changeset/*.md` files.
2. Decide whether to create, update, merge, or delete.
3. Target package ids unless a configured group is the real release owner.
4. Keep unrelated changes in separate files.
5. Combine packages only when the release note would be the same.
6. Validate with `mc validate`, `mc diagnostics --format json`, or `mc step:diagnose-changesets --format json`.

Delete or rewrite stale changesets when the code they describe is reverted before release. Merge near-duplicate changesets when several packages changed for the same outward behavior, but keep unrelated features separate even if they touched the same package.
