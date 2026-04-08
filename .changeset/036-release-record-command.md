---
monochange: minor
monochange_core: minor
---

#### add `mc release-record` for commit-embedded release history inspection

monochange can now inspect the durable `ReleaseRecord` stored in release commit bodies.

Before, there was no built-in way to resolve a release record from a tag or descendant commit:

```bash
mc release-record --from v1.2.3
# unknown command
```

After, you can inspect the discovered record in text or JSON:

```bash
mc release-record --from v1.2.3
mc release-record --from HEAD --format json
```

The new command:

- resolves the supplied ref to a commit
- walks first-parent ancestry until it finds a monochange release record
- reports the release-record commit, distance, targets, packages, and provider identity
- fails loudly if it encounters a malformed release record on the path
