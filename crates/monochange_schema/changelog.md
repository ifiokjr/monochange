# Changelog

All notable changes to this project will be documented in this file.

This changelog is managed by [monochange](https://github.com/monochange/monochange).

## monochange_schema [0.1.0](https://github.com/monochange/monochange/releases/tag/monochange_schema/v0.1.0) (2026-05-09)

### Breaking Change

#### Publish durable release schema contracts

Impact: release records now use the first public durable schema header, `v = "0.1"`, and monochange rejects missing, invalid, old, or future durable schema versions instead of reading unsafe historical shapes. The new `monochange_schema` crate owns schema version parsing, release-record wire validation, committed schema assets, and durable migration helpers.

Usage: editors can use the hosted configuration schema once GitHub Pages publishes the docs, or the raw GitHub fallback immediately. Durable release records now embed the public version field instead of the internal Rust-only `schemaVersion` field:

```json
{
	"v": "0.1",
	"kind": "monochange.releaseRecord"
}
```

The `monochange_schema` package remains independently versioned from the main release group. Its crate version starts at `0.0.0` on this branch, while this major changeset gives release planning the explicit signal to publish the first crate release without changing the durable public schema version `0.1`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #396](https://github.com/monochange/monochange/pull/396) _Introduced in:_ [`563ef83`](https://github.com/monochange/monochange/commit/563ef83fa21260518ae60c972240e2f0562e9bc2) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

### Fixed

#### Fix public schema version to derive from crate version

Impact: the `monochange_schema` crate version was accidentally bumped to `0.1.0` during release preparation. Reverted back to `0.0.0` and replaced the hardcoded public schema version `0.1` with a build-time generated constant derived from `CARGO_PKG_VERSION` (`major.minor`).

The `CURRENT_SCHEMA_VERSION_TEXT` constant is now embedded at compile time via `include_str!` from `SCHEMA_VERSION`, eliminating the `build.rs` dependency and ensuring `cargo publish` verification works correctly.

Usage: no external changes needed. Schema assets are regenerated via `schema:update` using the same generated version.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #399](https://github.com/monochange/monochange/pull/399) _Introduced in:_ [`62d1d90`](https://github.com/monochange/monochange/commit/62d1d90c0a0ec911cd4aea1452a8f710eb1d6072) _Last updated in:_ [`8c6a312`](https://github.com/monochange/monochange/commit/8c6a312f2d9e7477fd7901688d878c721ba41336)

#### Add Forgejo source provider

Add Forgejo as a hosted source provider for releases and release pull requests.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #401](https://github.com/monochange/monochange/pull/401) _Introduced in:_ [`86026ac`](https://github.com/monochange/monochange/commit/86026acb83e338fe8d07c200fb8e38693616b6e8)

#### Derive schema version from package metadata

The schema crate now derives the durable schema version from its Cargo package version's major/minor components at compile time, keeping the public schema version aligned without a build script.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #403](https://github.com/monochange/monochange/pull/403) _Introduced in:_ [`ca76476`](https://github.com/monochange/monochange/commit/ca7647611d09ac3a635d31eb4831e91e266f4797) _Last updated in:_ [`9b80c1b`](https://github.com/monochange/monochange/commit/9b80c1b67d31adf4d0ed9be90bae95a68d105d2a)

### Testing

#### Extract inline test modules into separate files

Move all inline `#[cfg(test)] mod tests { ... }` blocks out of source files into dedicated test files. This reduces source file sizes and keeps test code in a consistent `__tests/` directory structure next to the module it tests.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #416](https://github.com/monochange/monochange/pull/416) _Introduced in:_ [`3535c88`](https://github.com/monochange/monochange/commit/3535c887c46d66db2768377cb5f01406f6e9a8b6)

#### Normalize Rust unit test file layout

Move Rust unit tests into colocated `__tests__/` directories and name each file after the module under test with a `_tests.rs` suffix.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #428](https://github.com/monochange/monochange/pull/428) _Introduced in:_ [`b61cc3e`](https://github.com/monochange/monochange/commit/b61cc3e66989fd83ffb16a31568d2f46d7075216)

#### Improve readability of multiline JSON snapshots

Redact multiline string fields inside JSON snapshots and assert their contents separately so release-planning test snapshots remain readable without escaped newline sequences.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #398](https://github.com/monochange/monochange/pull/398) _Introduced in:_ [`458b671`](https://github.com/monochange/monochange/commit/458b671252f98a25628cd08a497792149370386d)

## monochange_schema [0.1.1](https://github.com/monochange/monochange/releases/tag/monochange_schema/v0.1.1) (2026-05-10)

### Added

#### Migrate JSON Schema generation from hand-tuned templates to schemars

Schema assets (`monochange.schema.json` and `release-record.schema.json`) are now generated from the Rust type tree via the `schemars` crate, eliminating manual drift between source types and committed schemas.

###### Added

- `schema` feature on `monochange_core` and `monochange_config` gating `schemars`.
- `JsonSchema` derives on `ReleaseRecord`, `RawWorkspaceConfiguration`, and their transitive types.
- `monochange_core::schema` and `monochange_config::schema` modules providing `release_record()` and `workspace_configuration()` schema generation functions.
- `xtask` binary crate providing `schema update` and `schema check` subcommands, with a `cargo xtask` alias.

###### Changed

- `devenv.nix` `schema:update` / `schema:check` now invoke `cargo xtask schema update` and `cargo xtask schema check`.
- `$defs` keys use camelCase names (e.g. `packageDefinition`) via `#[schemars(rename)]` attributes.
- Release-record `schemaVersion` and `kind` emit `const` constraints instead of `default`.

###### Removed

- `scripts/schema-assets.sh` shell script.
- `schemas/templates/*.schema.template.json` template files.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #438](https://github.com/monochange/monochange/pull/438) _Introduced in:_ [`d0676f0`](https://github.com/monochange/monochange/commit/d0676f067299fb4db38cc748dcbb619ab7532a49)

### Fixed

#### Add `always_run` primitive to CLI steps and group/ecosystem filters to `PublishPackages`

##### `always_run` primitive

A new `always_run` boolean field is available on every CLI step definition. When `always_run: true`, the step continues to execute even when a previous step in the same command has failed.

This enables composable dry-run workflows such as:

```toml
[[cli.publish-dry-run]]
name = "publish-dry-run"
help_text = "Preview publishing without side effects"
steps = [
	{ type = "PrepareRelease", name = "prepare", inputs = { allow_empty_changesets = "true" } },
	{ type = "PublishPackages", name = "publish", always_run = true, inputs = { resume = ".monochange/local/previous-result.json" } },
]
```

Running `mc publish-dry-run --dry-run` will always execute the `PublishPackages` step regardless of whether `PrepareRelease` succeeds, because `PublishPackages` is marked `always_run = true`.

###### Behavior

- When a step fails and later steps have `always_run: true`, those steps still execute.
- Non-`always_run` steps after a failure are skipped.
- The overall command still returns the first error after all `always_run` steps finish.

##### `PublishPackages` filters

`PublishPackages` now accepts two new step inputs:

- `--group <group-id>` — resolves a group from the workspace configuration and publishes all packages in that group.
- `--ecosystem <ecosystem>` — filters publication targets to a specific ecosystem (`cargo`, `npm`, `deno`, `dart`, `flutter`, `python`, or `go`).

Both inputs can be repeated:

```bash
mc publish --group sdk --group apps --ecosystem npm --ecosystem cargo
```

Groups are resolved to their member packages before ecosystem filtering is applied.

##### Dry-run guards

`PublishPackages` now skips the following side-effecting operations when `--dry-run` is active:

- `release_branch_policy::verify_release_ref_for_publish`
- `publish_rate_limits::enforce_publish_rate_limits`
- writing the publish report artifact to disk

##### Per-command `dry_run` field

CLI command definitions now support a `dry_run` boolean field. When `dry_run = true`, the command always executes in dry-run mode regardless of whether `--dry-run` is passed on the CLI. This enables built-in preview commands such as:

```toml
[cli.publish-check]
help_text = "Validate the release and preview package publishing in dry-run mode"
dry_run = true
steps = [
	{ type = "PrepareRelease", name = "plan release" },
	{ type = "PublishPackages", name = "publish packages dry run" },
]
```

Running `mc publish-check` (without `--dry-run`) will still run in dry-run mode because the command definition sets `dry_run = true`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #426](https://github.com/monochange/monochange/pull/426) _Introduced in:_ [`6ea6236`](https://github.com/monochange/monochange/commit/6ea623624e36d795edd531ae72080a2e9c3fb86a)

#### allow boolean and numeric literals in `CliInputDefinition.default`

The JSON schema for `monochange.toml` `[cli.*.inputs]` previously rejected boolean and numeric defaults, even though the Rust deserializer already accepted them correctly.

**Before:**

```toml
[[cli.release-pr.inputs]]
name = "no_verify"
type = "boolean"
default = true # jsonschema error: "true is not of types \"null\", \"string\""
```

**After:**

The `default` field in `CliInputDefinition` now accepts `string | boolean | integer | number | null` in the generated schema. TOML like the snippet above validates cleanly, and numeric defaults such as `default = 42` are also accepted.

The internal `CliInputDefault` enum gained `Integer(i64)` and `Number(f64)` variants, and the `schemars` derive now generates a multi-type `anyOf` schema for the `default` property.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #445](https://github.com/monochange/monochange/pull/445) _Introduced in:_ [`1d42ece`](https://github.com/monochange/monochange/commit/1d42ece77ceda58cd44ce67749c5faa5d4ec8314)

#### Release PR formatting, schema version, and publish batch ordering

1. Format generated `.monochange/releases/` manifests via `dprint fmt` in `[cli.release-pr]`.
2. Derive expected schema versions in snapshots and tests from the actual `Cargo.toml` version instead of hardcoding `0.0`.
3. Topologically sort publish requests by both runtime and development dependencies before batching so dependencies are published before dependents.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #436](https://github.com/monochange/monochange/pull/436) _Introduced in:_ [`ea78ccc`](https://github.com/monochange/monochange/commit/ea78ccc844318b0645010f15fcf60b9d8ea6a58c) _Related issues:_ [#434](https://github.com/monochange/monochange/issues/434)

#### Replace release record `groupVersion` with `versions`

Release records now include a `versions` map keyed by released package or group id, and no longer write the redundant `groupVersion` field.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #450](https://github.com/monochange/monochange/pull/450) _Introduced in:_ [`375bc19`](https://github.com/monochange/monochange/commit/375bc19dc69c125ffbd944d016b16ebc1c8cb7c5)

### Documentation

#### Document `CommitRelease.update_release_json` option

Add comprehensive documentation for the `update_release_json` step-level input on `CommitRelease`:

- Document the input in the `CommitRelease` CLI step reference with type, default, and description
- Explain semantic JSON comparison (formatting-only differences such as indentation or key ordering are ignored)
- Add a new composition example showing how to combine `dprint fmt` formatting with `CommitRelease` using `update_release_json = true`
- Add a new common-mistake entry about running formatters between `PrepareRelease` and `CommitRelease` without setting the input
- Document the field in the configuration guide's workflow variables section
- Regenerate JSON Schema assets to include the new `update_release_json` field in `CommitRelease` step definitions

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #443](https://github.com/monochange/monochange/pull/443) _Introduced in:_ [`4b8cc5a`](https://github.com/monochange/monochange/commit/4b8cc5a25644ab3623177c08bd7904c649ea67a0)

## monochange_schema [0.2.0](https://github.com/monochange/monochange/releases/tag/monochange_schema/v0.2.0) (2026-05-14)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #467](https://github.com/monochange/monochange/pull/467) _Introduced in:_ [`ce4712f`](https://github.com/monochange/monochange/commit/ce4712f2890e0636c368b056db756df32f4cf769) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

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

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #479](https://github.com/monochange/monochange/pull/479) _Introduced in:_ [`d9adff8`](https://github.com/monochange/monochange/commit/d9adff8fb396df908e335d2a6688aa729abb5f4d) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#476](https://github.com/monochange/monochange/issues/476)

#### keep release-record discovery compatible across schema upgrades

Merged release commits that embed an older public release-record `schemaVersion` can now be read by newer monochange binaries. This lets commands such as:

```bash
mc step:release-record --from HEAD --format json
```

recognize an existing release commit after monochange itself has moved to a newer schema version, instead of reporting the older record as unsupported.

The GitHub Actions CI workflow now also includes a pull-request `release-records` preflight. For normal PRs, it creates the same local release commit used by the release test/lint preflights and verifies that `mc step:release-record` can read it and confirm that the generated commit is the resolved release-record commit.

Generated current and versioned release-record artifact fixtures are also checked into the schema crate. Schema checks ensure the current fixtures are regenerated during release planning, and integration tests load every checked-in release-record artifact through the real parser/migration path.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #491](https://github.com/monochange/monochange/pull/491) _Introduced in:_ [`914a6e8`](https://github.com/monochange/monochange/commit/914a6e88d4bcf31249d467914f0a3a3b240d931a) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

### Added

#### Configurable publish-order dependency fields

Add configurable ecosystem-specific dependency fields for package publish ordering across npm, Cargo, Deno, Dart/Flutter, Python, and Go.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #472](https://github.com/monochange/monochange/pull/472) _Introduced in:_ [`0d9cf46`](https://github.com/monochange/monochange/commit/0d9cf461a05057b61efa987d361ebd27d800dbdb) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8) _Closed issues:_ [#465](https://github.com/monochange/monochange/issues/465)

### Fixed

#### Add configuration schema artifact variants

Generate populated `monochange` configuration artifact fixtures alongside the release-record fixtures so schema consumers have stable `current` and versioned examples for the configuration schema.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #493](https://github.com/monochange/monochange/pull/493) _Introduced in:_ [`28f457c`](https://github.com/monochange/monochange/commit/28f457ca883cb6e00d33bcb78fe63bf639b6b308) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Publish schema crate update

Publish a schema crate update so dependent crates can resolve the required schema version during publication.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #460](https://github.com/monochange/monochange/pull/460) _Introduced in:_ [`3dc84b2`](https://github.com/monochange/monochange/commit/3dc84b292762117f03692b1f37278f5ab9001e96) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### Generate versioned schema artifacts in release PRs

Release PR schema updates now write versioned schema files and artifact fixtures for the schema crate.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #507](https://github.com/monochange/monochange/pull/507) _Introduced in:_ [`8297398`](https://github.com/monochange/monochange/commit/82973985b38897025eaaa8ad9d0497278dc6b374)

#### Add lints to the monochange config schema

Allow the top-level `[lints]` table in generated `monochange.toml` JSON Schema assets. The lint configuration schema is intentionally permissive so all current and future lint rule shapes are accepted by editors and TOML language servers.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #462](https://github.com/monochange/monochange/pull/462) _Introduced in:_ [`953bb8c`](https://github.com/monochange/monochange/commit/953bb8c6e3532a31621c47c6e0b71eaa684771fc) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)

#### harden release-record schema migrations

Release-record artifacts published with schema `0.0` are now treated as the legacy `v`-field shape and migrate through an explicit `0.0 -> 0.1 -> current` path. This keeps older commit-embedded records readable while preserving the future-version rejection behavior for artifacts newer than the current binary understands.

For example, callers can continue to load a legacy record like:

```json
{
	"v": "0.0",
	"kind": "monochange.releaseRecord",
	"createdAt": "2026-04-06T12:00:00Z",
	"command": "release-pr",
	"releaseTargets": [],
	"releasedPackages": [],
	"changedFiles": []
}
```

and receive the current `schemaVersion` field after migration. The release schema preflight now also checks committed schema assets, Rust migration edges, generated docs copies, and active schema changesets so release PRs fail before publishing inconsistent schema artifacts.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #500](https://github.com/monochange/monochange/pull/500) _Introduced in:_ [`bd56420`](https://github.com/monochange/monochange/commit/bd564204b786961371b0ac1bad21071ebe5fe90c) _Last updated in:_ [`a485823`](https://github.com/monochange/monochange/commit/a485823190fecfeebbef996c74ee63f241b6f7d8)
