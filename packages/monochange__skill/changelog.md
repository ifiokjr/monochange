# Changelog

## Unreleased

### Changed

- Rewrote the skill package around the current monochange CLI/tool harness.
- Documented verified built-in commands, step commands, MCP tools, user-defined command behavior, and all current CLI step types.
- Replaced obsolete examples with current `monochange.toml`, changeset, release-preview, and publishing workflow examples.

## [0.5.0](https://github.com/monochange/monochange/releases/tag/v0.5.0) (2026-05-14)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #467](https://github.com/monochange/monochange/pull/467) _Introduced in:_ [`ce4712f`](https://github.com/monochange/monochange/commit/ce4712f2890e0636c368b056db756df32f4cf769) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16)

#### generate built-in release and validation step commands

> **Breaking change** — several hardcoded top-level commands now live under generated immutable `mc step:*` command names.

The release-record, publish-readiness, tag-release, placeholder-publish, and validation operations now share the generated step-command path used by the rest of the CLI step catalog. This keeps their help, schema metadata, docs, and automation examples consistent with configured workflow steps while preserving the distinction between binary commands, generated step commands, and optional user-defined `[cli.*]` workflow aliases.

**Before:** scripts could call these hardcoded top-level commands directly:

```bash
mc validate
mc release-record --from HEAD --format json
mc publish-readiness --from HEAD --output .monochange/readiness.json
mc tag-release --from HEAD
mc publish-bootstrap --from HEAD --output .monochange/bootstrap-result.json
```

**After:** call the generated step command names instead:

```bash
mc step:validate
mc step:release-record --from HEAD --format json
mc step:publish-readiness --from HEAD --output .monochange/readiness.json
mc step:tag-release --from HEAD
mc step:placeholder-publish --from HEAD --output .monochange/bootstrap-result.json
```

`mc init` also writes a smaller starter configuration. It no longer seeds redundant generated `[cli.*]` aliases for commands that already exist as immutable step commands.

**Before:** starter configs included workflow aliases for generated behavior:

```toml
[cli.validate]
steps = [{ type = "Validate" }]
```

**After:** starter configs rely on the generated command directly and reserve `[cli.*]` for repository-specific chains, custom inputs, or shell `Command` steps:

```bash
mc step:validate
```

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #479](https://github.com/monochange/monochange/pull/479) _Introduced in:_ [`d9adff8`](https://github.com/monochange/monochange/commit/d9adff8fb396df908e335d2a6688aa729abb5f4d) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16) _Closed issues:_ [#476](https://github.com/monochange/monochange/issues/476)

### Added

#### Configurable publish-order dependency fields

Add configurable ecosystem-specific dependency fields for package publish ordering across npm, Cargo, Deno, Dart/Flutter, Python, and Go.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #472](https://github.com/monochange/monochange/pull/472) _Introduced in:_ [`0d9cf46`](https://github.com/monochange/monochange/commit/0d9cf461a05057b61efa987d361ebd27d800dbdb) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16) _Closed issues:_ [#465](https://github.com/monochange/monochange/issues/465)

#### Publish all configured packages

Add a `--all` flag to the PublishPackages CLI step so migration workflows can publish every configured package, including packages that were not part of the prepared release record.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #461](https://github.com/monochange/monochange/pull/461) _Introduced in:_ [`3d956cd`](https://github.com/monochange/monochange/commit/3d956cd3e34747e088add98fe0358251f388782f) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16)

### Fixed

#### Add interactive CLI command wizard

Added `mc command`, an interactive dashboard for adding and editing `[cli.<name>]` commands in `monochange.toml`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #471](https://github.com/monochange/monochange/pull/471) _Introduced in:_ [`fea471c`](https://github.com/monochange/monochange/commit/fea471c4b67b618cde51eaacfd4e30742cfb0dc1) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16)

#### add release-record migration command

Add `mc migrate release-records` to rewrite persisted release records to the latest schema version, expose the release-record migration helper from core, and update the generated skill command inventory.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #500](https://github.com/monochange/monochange/pull/500) _Introduced in:_ [`bd56420`](https://github.com/monochange/monochange/commit/bd564204b786961371b0ac1bad21071ebe5fe90c)

#### Rewrite monochange skill guidance

The monochange skill package now documents the current CLI/tool harness, verified built-in commands, step commands, MCP tools, custom `monochange.toml` workflows, and package versioning examples. The command guide also includes a generated inventory checked by `cargo xtask skill commands check` to prevent future CLI drift.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #463](https://github.com/monochange/monochange/pull/463) _Introduced in:_ [`0f3d15c`](https://github.com/monochange/monochange/commit/0f3d15c38b15124a9bb96ed4c73829602e34e838) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16)

### Testing

#### Validate generated release commits in PR CI

Pull requests now run release-state test and lint preflights after creating a local release commit, while generated release PRs skip those extra preflights.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #477](https://github.com/monochange/monochange/pull/477) _Introduced in:_ [`a09020f`](https://github.com/monochange/monochange/commit/a09020f9282be207ace6f641b716c3c4004886af) _Last updated in:_ [`61254db`](https://github.com/monochange/monochange/commit/61254dbe1caf0d50030c544f10a5676a280e8d16)
