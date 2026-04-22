---
monochange: minor
---

feat: make CLI step inputs explicit-only

Steps no longer implicitly inherit all command-level inputs. Each step must now explicitly declare which inputs it consumes via its `inputs` map.

### Breaking changes

- Steps that previously relied on implicit input forwarding now require an explicit `inputs` block. For example, a `Discover` step that needs the `format` input must declare:

  ```toml
  [[cli.discover.steps]]
  type = "Discover"
  inputs = { format = "{{ inputs.format }}" }
  ```

- `format` and `ci` are no longer valid step-level inputs for steps that do not consume them directly. These should be declared as command-level inputs instead, and referenced from the step via template strings.

### Migration guide

Existing configurations using implicit forwarding should add `inputs` maps on each step with the exact inputs the step needs. Built-in default commands have been updated to include these forwardings automatically.

```toml
[[cli.discover.inputs]]
name = "format"
type = "choice"
choices = ["text", "json"]

[[cli.discover.steps]]
type = "Discover"
inputs = { format = "{{ inputs.format }}" }
```

### Runtime updates

- `resolve_step_inputs` starts with an empty `BTreeMap` instead of cloning `context.inputs`
- `resolve_command_output` reads `format` and `ci` directly from `context.inputs`
- `evaluate_cli_step_condition` merges `context.inputs` and `step_inputs`
- `cli_input_template_value` returns `Null` for empty input lists
- `parse_string_as_boolean` treats `'none'` as `false` for `when` conditions
