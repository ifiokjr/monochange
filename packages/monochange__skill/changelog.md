# Changelog

## Unreleased

### Changed

- Rewrote the skill package around the current monochange CLI/tool harness.
- Documented verified built-in commands, step commands, MCP tools, user-defined command behavior, and all current CLI step types.
- Replaced obsolete examples with current `monochange.toml`, changeset, release-preview, and publishing workflow examples.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-12)

### Breaking Change

#### require CLI steps to opt in to inherited command inputs

> **Breaking change** — CLI step inputs are now explicit. Command-level inputs no longer automatically appear in every configured CLI step.

A configured step now receives only the inputs listed in that step's `inputs` field. This removes ambiguous behavior where a command-level flag could unexpectedly shadow a step-specific input with the same name.

**Before:** every step implicitly saw all command inputs, even with no step-level `inputs` entry:

```toml
[cli.release]
inputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]
steps = [{ type = "PrepareRelease" }]
```

**After:** inherit command inputs explicitly with the array shorthand:

```toml
[cli.release]
inputs = [{ name = "format", type = "choice", choices = ["text", "json"], default = "text" }]
steps = [{ type = "PrepareRelease", inputs = ["format"] }]
```

Map overrides still work for fixed or templated step values:

```toml
steps = [
	{ type = "PrepareRelease", inputs = ["format"] },
	{ type = "PublishRelease", inputs = { format = "json", draft = "{{ inputs.draft }}" } },
]
```

Migration path: review custom `[cli.<command>]` definitions and add `inputs = ["name"]` to every step that needs a command-level input. Built-in default CLI commands and generated templates have been updated to declare their inherited inputs explicitly.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #467](https://github.com/monochange/monochange/pull/467) _Introduced in:_ [`ce4712f`](https://github.com/monochange/monochange/commit/ce4712f2890e0636c368b056db756df32f4cf769)

### Added

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f)

### Fixed

#### Rewrite monochange skill guidance

The monochange skill package now documents the current CLI/tool harness, verified built-in commands, step commands, MCP tools, custom `monochange.toml` workflows, and package versioning examples. The command guide also includes a generated inventory checked by `cargo xtask skill commands check` to prevent future CLI drift.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #463](https://github.com/monochange/monochange/pull/463) _Introduced in:_ [`0f3d15c`](https://github.com/monochange/monochange/commit/0f3d15c38b15124a9bb96ed4c73829602e34e838)
