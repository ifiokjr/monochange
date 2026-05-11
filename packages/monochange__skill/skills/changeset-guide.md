# Changeset guide

## Before writing

- Inspect the diff.
- Read existing `.changeset/*.md` files.
- Inspect `monochange.toml` for package ids, groups, and changelog types.
- Decide whether this is create, update, merge, split, or delete.

## Targeting

Prefer package ids. Use group ids only for group-owned releases. If a dependent package is included only because another package changed, use `caused_by` and consider `bump: none`.

## Structure

Simple bump syntax:

```md
---
"package-id": patch
---

# Short user-facing summary

Explain the behavior change and impact.
```

Object syntax with a configured changelog type:

```md
---
"package-id":
  bump: patch
  type: fix
---

# Short user-facing summary

Explain the behavior change and impact.
```

## Review checklist

- Bump matches user impact.
- Heading is sentence case and no trailing full stop if project policy requires it.
- Breaking changes have migration instructions.
- Similar package notes are combined instead of duplicated.
- `mc validate` passes.
