---
monochange: minor
monochange_config: minor
monochange_core: minor
monochange_github: minor
---

#### add typed pull-request changeset policy enforcement

The `EnforceChangesetPolicy` workflow step evaluates whether a pull request's changed files are covered by attached changesets, then posts a bot comment with the result:

```toml
[cli.verify]
[[cli.verify.inputs]]
name = "changed_path"
kind = "string_list"

[[cli.verify.inputs]]
name = "label"
kind = "string_list"

[[cli.verify.steps]]
type = "EnforceChangesetPolicy"
```

```bash
# in CI, pass changed files and PR labels as inputs
mc verify \
  --changed-path src/lib.rs \
  --changed-path crates/core/src/main.rs \
  --label "no changeset"
```

```json
{
	"status": "pass",
	"summary": "all changed packages are covered by a changeset",
	"affected_package_ids": ["core"],
	"changeset_paths": [".changeset/my-feature.md"]
}
```

Policy behaviour is configured through `[github.bot.changesets]`:

```toml
[github.bot.changesets]
enabled = true
skip_labels = ["no changeset", "docs only"]
```

When a skip label is present the step exits with `status = "skip"` instead of evaluating coverage. The `--format json` flag returns the full `ChangesetPolicyEvaluation` struct.

**`monochange_core`** adds `ChangesetPolicyEvaluation` and `ChangesetPolicyStatus`. **`monochange_github`** handles the bot-comment rendering.
