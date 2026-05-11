# Adoption checklist

Use this when adding monochange to an existing monorepo.

## Discovery

1. List package ecosystems: Cargo, npm, Deno, Dart/Flutter, Python, Go.
2. Identify package ids that should be release-managed.
3. Identify private packages or applications that should be excluded from releases.
4. Identify groups that must share a version.
5. Identify lockfiles and generated schemas that must refresh after version changes.
6. Identify current release/publish CI jobs and whether monochange should replace or feed them.

## Initial commands

```bash
mc init
mc validate
mc step:discover --format json
mc check
```

Edit the generated config rather than accepting it blindly.

## Migration questions

- Are packages independently versioned or grouped?
- Which packages get tags or provider releases?
- Which packages publish to public registries?
- Which packages are built-in publishable vs external publishable?
- Which changelog format should be used?
- Which package paths should require changesets in pull requests?
- Which user-defined workflow names should this repository expose?

## Minimal outcome

A good initial adoption has:

- Explicit `[package.*]` entries for managed packages.
- Optional `[group.*]` entries for synchronized versions.
- `[ecosystems.*]` settings for enabled ecosystems and lockfile/versioned-file behavior.
- `[changesets.affected]` if CI checks changeset coverage.
- `[lints]` if `mc check` should enforce manifest rules.
- `[cli.*]` workflows for common team commands.
