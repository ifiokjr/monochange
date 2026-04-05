---
monochange: patch
monochange_config: patch
monochange_core: patch
monochange_github: patch
---

#### add package-aware changeset verification primitive and default verify command

`mc verify` is now a built-in default command that reports which packages are affected by a set of changed paths and whether each is covered by an attached changeset:

```bash
mc verify \
  --changed-paths src/lib.rs crates/core/src/main.rs \
  --format json
```

```json
{
	"status": "uncovered",
	"affected_package_ids": ["core"],
	"uncovered_package_ids": ["core"],
	"changeset_paths": []
}
```

Verification is configured directly under `[changesets]` rather than under `[github.bot.changesets]`, making it usable without any source-provider config:

```toml
[changesets.verify]
enabled = true
skip_labels = ["no changeset"]
```

The underlying step type was also unified: `VerifyChangesets` is the canonical name, and `EnforceChangesetPolicy` remains as a backward-compatible alias. Output now includes explicit `affected_packages` and `uncovered_packages` lists alongside the existing summary.

**`monochange_config`** gains the `ChangesetSettings.verify` field. **`monochange_core`** exposes `ChangesetVerificationSettings` and the updated `ChangesetPolicyEvaluation` with `affected_package_ids` and `uncovered_package_ids` fields.
