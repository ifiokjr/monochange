---
monochange: minor
monochange_config: minor
monochange_core: minor
---

#### Validate CLI step input overrides at config time

`mc validate` and all commands now reject unknown or mistyped input overrides on built-in CLI step definitions. `Command` steps still accept any input.

- Added `valid_input_names()` to `CliStepDefinition` — returns the exhaustive set of input names a built-in step consumes, or `None` for `Command` steps.
- Added `expected_input_kind()` to `CliStepDefinition` — returns the expected `CliInputKind` for a named input so type mismatches are caught early.
- Config validation now calls `validate_step_input_overrides()` for every step in every CLI command, surfacing clear error messages with valid-input lists.
