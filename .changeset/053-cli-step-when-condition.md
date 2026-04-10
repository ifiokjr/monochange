---
monochange: minor
monochange_core: minor
monochange_config: minor
---

#### add conditional execution support for all CLI steps

`when` conditions are now supported on all `CliStepDefinition` variants (not just `Command`) via a shared `when: "..."` field and runtime evaluation in the command dispatcher.

Before, only `Command`-specific behavior had first-class examples in docs, and users had to rely on wrapper commands for conditional behavior elsewhere. Now every step can be skipped by condition:

```toml
[[cli.release.steps]]
type = "Validate"
when = "{{ inputs.publish }}"
```

The `when` evaluator now also accepts common falsey values consistently for template-rendered conditions, including:

- `false`
- `""` (empty string)
- `0`

This release also updates runtime docs and the `mc init` template guidance so all CLI step reference pages reflect the new conditional behavior.
