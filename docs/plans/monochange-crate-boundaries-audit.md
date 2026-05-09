# monochange crate boundaries audit

## Current shape

The top-level `monochange` crate is currently much more than a thin CLI/composition layer. It has about 56k lines under `crates/monochange/src`, compared with 1k-5k lines in most ecosystem and provider crates. The largest modules are:

- `crates/monochange/src/package_publish.rs` (~5.7k lines, down from ~7k)
- `crates/monochange_publish/src/lib.rs` (~1.7k lines, extracted in #397 and expanded through Phase 2)
- `crates/monochange/src/cli_runtime.rs` (~6k lines)
- `crates/monochange/src/workspace_ops.rs` (~3.3k lines)
- `crates/monochange/src/changelog.rs` (~2.9k lines)
- `crates/monochange/src/release_artifacts.rs` (~2.3k lines)
- `crates/monochange/src/__tests.rs` (~13.4k lines), `release_record.rs` (~570 lines), `publish_readiness.rs`, `publish_rate_limits.rs`, `versioned_files.rs`, `changesets.rs`, `changeset_policy.rs`

The top-level crate depends on every ecosystem crate and every provider crate. That is acceptable for final composition, but it is also where many ecosystem-specific and provider-specific decisions are implemented. The result is that logic that should be owned by `monochange_npm`, `monochange_cargo`, `monochange_github`, etc. is mixed into `monochange`.

## Target architecture

`monochange` should become a thin application crate:

- parse CLI arguments and map them to use cases
- construct a registry of ecosystem adapters and source-provider adapters
- call planner/release/publish services from library crates
- render user-facing CLI/MCP output
- contain almost no ecosystem-specific or provider-specific business rules

A healthy dependency direction would be:

```text
monochange CLI/app
  -> monochange_workspace / monochange_release / monochange_publish / monochange_changelog
    -> monochange_core + monochange_config + monochange_graph + monochange_semver
    -> ecosystem crates through traits/registries
    -> provider crates through traits/registries
```

Provider and ecosystem crates should not depend on `monochange`; service crates should depend on `monochange_core` abstractions and receive adapters through traits instead of hardcoding each concrete crate.

## Highest-value split candidates

### 1. Move publish orchestration out of `monochange`

**Current files**

- `crates/monochange/src/package_publish.rs`
- `crates/monochange/src/publish_bootstrap.rs`
- `crates/monochange/src/publish_rate_limits.rs`
- `crates/monochange/src/publish_readiness.rs`
- `crates/monochange/src/trust_capabilities.rs`

**Issue**

`package_publish.rs` owns too many concepts at once: publish request construction, dependency ordering, resume artifacts, registry existence checks, placeholder package generation, ecosystem command construction, trusted publishing setup, npm/GitHub trust details, Cargo metadata checks, Go proxy behavior, and registry HTTP clients.

Examples of ecosystem/provider leakage in the top-level crate:

- `npm token` environment rejection still lives in `package_publish.rs`; npm trusted-publishing command construction and npm placeholder manifest generation are moving into `monochange_npm`
- Cargo placeholder manifest generation in `package_publish.rs` (Cargo publish readiness blockers now live in `monochange_cargo`)
- Dart, JSR, Python, Go placeholder manifest generation in `package_publish.rs`
- top-level orchestration that still hardcodes ecosystem/provider trust setup instead of calling publish/provider adapters
- trusted publishing capability matrix now lives in `monochange_publish`, while GitHub workflow/environment trust context now lives in `monochange_github`

**Recommendation**

Create `monochange_publish` for the generic publish use case and move generic types/functions there:

- `PublishRequest`
- `PackagePublishReport`
- resume/report artifact read/write/merge
- dependency-ordering of publish requests
- rate-limit planning/enforcement
- readiness/bootstrap report models and validation

Then introduce per-ecosystem publish adapters, preferably in existing ecosystem crates:

- `monochange_npm`: npm/pnpm publish command construction, npm placeholder manifest, npm trusted-publishing CLI commands, npm token-env checks, npm registry existence checks
- `monochange_cargo`: Cargo publish command construction, Cargo placeholder manifest, Cargo manifest readiness blockers, crates.io existence checks
- `monochange_dart`: Dart publish command and placeholder manifest
- `monochange_deno`: JSR publish command, placeholder manifest, JSR existence checks
- `monochange_python`: Python publish command, placeholder manifest, PyPI existence checks
- `monochange_go`: Go pseudo-publish/tag/proxy checks and placeholder manifest

Provider-specific trust context should move to provider crates:

- `monochange_github`: resolve GitHub Actions workflow/job/environment trust context and verify trusted publishing identity prerequisites
- `monochange_gitlab`/`monochange_gitea`: future provider-specific trust context or explicit unsupported capability responses

**Phase 1 (completed in #397):** `monochange_publish` now exists and owns:

- Data models: `PublishRequest`, `PackagePublishReport`, `PackagePublishOutcome`, `TrustedPublishingOutcome`, `TrustedPublishingStatus`
- Command builders: `build_publish_command` with per-ecosystem dispatch (npm, pnpm, Cargo, Dart, JSR, Python, Go)
- Go helpers: `go_module_path`, `go_proxy_version`, `go_module_tag_name`
- Trust/capability: `detect_trusted_publishing_identity`, `TrustedPublishingIdentity`, `RegistryTrustCapabilities`, `ProviderRegistryTrustCapability`, `trusted_publishing_capability_message*`
- CI providers: `CiProviderKind`

**Phase 2: Remaining leakages from `monochange` into `monochange_publish`**

| Still in `monochange/src/package_publish.rs`                  | Target               | Status       |
| ------------------------------------------------------------- | -------------------- | ------------ |
| `RegistryEndpoints` + `registry_client()`                     | `monochange_publish` | Done in #404 |
| `package_can_be_published()` (registry HTTP existence checks) | `monochange_publish` | Done in #404 |
| `CommandExecutor` trait + `ProcessCommandExecutor`            | `monochange_publish` | Done in #409 |
| Resume/artifact read/write/merge                              | `monochange_publish` | Done in #412 |
| Dependency ordering of publish requests                       | `monochange_publish` | Done in #412 |
| `cargo_publish_readiness_blockers`                            | `monochange_cargo`   | Done in #413 |
| GitHub trust context resolution and verification              | `monochange_github`  | Done in #417 |
| npm trusted-publishing command helpers                        | `monochange_npm`     | In #419      |
| npm placeholder manifest generation                           | `monochange_npm`     | In #419      |

**Phase 3: Introduce `PublishAdapter` trait**

Each ecosystem crate should implement a registry-agnostic trait so `monochange_publish` stops hardcoding match arms:

```rust
trait PublishAdapter {
	fn registry_kind(&self) -> RegistryKind;
	fn build_publish_command(
		&self,
		request: &PublishRequest,
		mode: PublishMode,
	) -> Result<CommandSpec>;
	fn build_placeholder(&self, request: &PublishRequest, dir: &Path) -> Result<()>;
	fn version_exists(
		&self,
		request: &PublishRequest,
		transport: &dyn RegistryTransport,
	) -> Result<bool>;
	fn readiness_blockers(&self, request: &PublishRequest) -> Result<Vec<String>>;
}

trait TrustedPublishingAdapter {
	fn plan_trust(
		&self,
		request: &PublishRequest,
		context: &TrustContext,
	) -> Result<TrustedPublishingOutcome>;
}
```

The top-level `monochange` crate should only register adapters and call `monochange_publish::run(...)`.

**Migration order**

1. Move `RegistryEndpoints`, `registry_client`, resume/artifact logic, dependency ordering, and `CommandExecutor` into `monochange_publish`. ✅
2. Extract Cargo readiness blockers into `monochange_cargo`. ✅
3. Extract GitHub trust context into `monochange_github`. ✅
4. Move npm trusted-publishing command helpers and npm placeholder manifest generation into `monochange_npm`.
5. Extract remaining placeholder manifest generation per ecosystem, continuing with JSR/Deno placeholders in `monochange_deno`.
6. Extract registry existence checks per ecosystem or route them through publish adapters.
7. Delete top-level ecosystem/provider match arms from `package_publish.rs` and reduce it to CLI glue or remove it entirely.

### 2. Move workspace discovery/planning orchestration out of `monochange`

**Current files**

- `crates/monochange/src/workspace_ops.rs`
- `crates/monochange/src/versioned_files.rs`
- `crates/monochange/src/changesets.rs`
- `crates/monochange/src/changeset_policy.rs`

**Issue**

`workspace_ops.rs` mixes initial config generation, package discovery across all ecosystems, changelog target resolution, changeset file rendering, release planning, versioned file updates, lockfile command execution, source-enrichment, and prepare-release file-diff generation.

Concrete leakage:

- `discover_packages` hardcodes calls to `discover_cargo_packages`, `discover_npm_packages`, `discover_deno_packages`, `discover_dart_packages`, `discover_python_packages`, `discover_go_modules`.
- configured package loading hardcodes `load_configured_*_package` for each ecosystem.
- lockfile update command inference and execution are mixed with release preparation.
- `versioned_files.rs` has a generic `VersionedFileKind` and update pipeline, but ecosystem-specific kind support already exists in ecosystem crates; the boundary is split.
- `monochange_config` also depends on all ecosystem/provider crates just to validate supported versioned files and source capabilities. That makes config parsing non-minimal and couples configuration to concrete adapters.

**Recommendation**

Create `monochange_workspace` for workspace-level orchestration:

- discovery orchestration
- configured package materialization
- lockfile command planning
- release preparation use case
- changeset discovery/loading context orchestration

Introduce an adapter registry instead of hardcoded match arms:

```rust
trait EcosystemAdapter {
	fn ecosystem(&self) -> EcosystemType;
	fn discover(&self, root: &Path) -> Result<Vec<PackageRecord>>;
	fn load_configured(&self, root: &Path, definition: &PackageDefinition)
	-> Result<PackageRecord>;
	fn supported_versioned_file_kind(&self, path: &Path) -> Option<VersionedFileKind>;
	fn default_lockfile_commands(&self, package: &PackageRecord) -> Vec<LockfileCommandDefinition>;
}
```

The top-level `monochange` crate would build:

```rust
let ecosystems = EcosystemRegistry::new()
    .with(monochange_cargo::adapter())
    .with(monochange_npm::adapter())
    .with(monochange_deno::adapter())
    .with(monochange_dart::adapter())
    .with(monochange_python::adapter())
    .with(monochange_go::adapter());
```

`monochange_config` should stop depending on concrete ecosystem/provider crates. It should validate configuration against registries supplied by the caller or against simple metadata tables from `monochange_core`. This is a major separation improvement because config parsing becomes independent of optional compiled-in adapters.

**Migration order**

1. Extract adapter traits and registry structs into `monochange_core` or a small `monochange_adapter` crate.
2. Add adapter constructors to existing ecosystem crates.
3. Move discovery orchestration from `workspace_ops.rs` into `monochange_workspace`.
4. Move changeset loading/planning orchestration from `changesets.rs`/`workspace_ops.rs` into `monochange_release` or `monochange_workspace`.
5. Make `monochange_config` accept provider/ecosystem capability registries instead of importing concrete crates.
6. Keep `monochange::discover_workspace`, `plan_release`, and `prepare_release` as compatibility wrappers that call the new crates.

### 3. Move changelog/release-notes generation to a dedicated crate

**Current files**

- `crates/monochange/src/changelog.rs`
- parts of `crates/monochange/src/release_artifacts.rs`
- release-note-related types in `monochange_core`

**Issue**

`changelog.rs` is a large, domain-heavy module in the top-level crate. It owns:

- changelog update selection and deduplication
- package/group release-note aggregation
- group changelog filtering
- actor/review/issue label rendering
- Jinja template rendering
- release-note document construction
- markdown section rendering
- initial changelog headers

This logic is not CLI-specific and not orchestration-only. It is reusable library behavior and should be tested at its own boundary.

**Recommendation**

Create `monochange_changelog` or `monochange_release_notes` and move the whole release-note/changelog engine there. The top-level crate should call:

```rust
let updates = monochange_changelog::build_changelog_updates(context)?;
```

Move from `release_artifacts.rs` as well:

- title template rendering (`TitleRenderContext`, `effective_title_template`, default title helpers)
- release manifest construction that depends on changelog rendering
- release commit message/body rendering if it is domain output rather than CLI output

Keep terminal coloring and display formatting in `monochange` or a separate CLI-output crate.

**Migration order**

1. Move `changelog.rs` unchanged into `monochange_changelog` with dependencies on `monochange_core`, `minijinja`, and `chrono` as needed.
2. Export a narrow `build_changelog_updates` API.
3. Move release-title helpers from `release_artifacts.rs` if they are shared by changelog and provider release construction.
4. Replace `pub(crate) use changelog::*` in `monochange::lib` with calls to `monochange_changelog`.

### 4. Split release artifacts from CLI presentation

**Current file**

- `crates/monochange/src/release_artifacts.rs`

**Issue**

`release_artifacts.rs` currently mixes several unrelated layers:

- release target and publication target construction
- manifest file update construction
- ecosystem-specific manifest updates for Cargo/npm/Deno/Dart
- atomic file writes
- unified diff rendering and ANSI coloring
- discovery report text/json rendering
- release manifest/release record construction
- provider release request construction and publish calls
- release commit/git operations

This module is a boundary smell: data preparation, filesystem mutation, provider publishing, and CLI rendering all live together.

**Recommendation**

Split into at least three crates/modules:

1. `monochange_release`: release target construction, release manifest construction, release record construction, provider request construction.
2. `monochange_versioning`: versioned file update planning/application, with ecosystem adapters providing format-specific updates.
3. `monochange_cli_output` or keep in app crate: discovery report rendering, diff coloring, terminal markdown rendering.

Ecosystem-specific manifest update functions should move to their ecosystem crates:

- `build_cargo_manifest_updates` -> `monochange_cargo`
- `build_npm_manifest_updates` -> `monochange_npm`
- `build_deno_manifest_updates` -> `monochange_deno`
- `build_dart_manifest_updates` -> `monochange_dart`

The generic release artifact service should ask each package's ecosystem adapter for file updates.

### 5. Move provider orchestration out of `monochange`

**Current files**

- `crates/monochange/src/hosted_sources.rs`
- `crates/monochange/src/release_record.rs`
- provider-related parts of `crates/monochange/src/release_artifacts.rs`
- provider validation/capability dispatch in `crates/monochange_config/src/lib.rs`

**Issue**

There are already provider crates (`monochange_github`, `monochange_gitlab`, `monochange_gitea`) plus a shared `monochange_hosting` crate, but the top-level crate still dispatches concrete providers and owns important release-record orchestration.

`hosted_sources.rs` is thin, but `release_record.rs` and `release_artifacts.rs` contain provider-facing orchestration: source release requests, change requests, release retargeting, tag sync, pull-request body/path construction, and hosted source adapter selection.

**Recommendation**

Create `monochange_sources` or expand `monochange_hosting` to include provider orchestration that is not specific to one provider:

- source adapter registry
- release request/change request orchestration
- release retarget planning/execution against `SourceProviderAdapter`
- source capabilities lookup
- provider configuration validation dispatch

The top-level crate should not match on `SourceProvider` except when constructing the adapter registry.

`monochange_config` should also stop importing provider crates for `source_capabilities()` and `validate_source_configuration()`. Instead:

- either `monochange_core` contains static provider capability metadata,
- or config validation takes a `SourceProviderRegistry` supplied by `monochange`.

### 6. Separate CLI runtime engine from command implementation

**Current file**

- `crates/monochange/src/cli_runtime.rs`

**Issue**

`cli_runtime.rs` is a second application inside the application. It owns command execution, template evaluation, conditional step evaluation, process streaming, telemetry, publish/report rendering, release output rendering, markdown terminal rendering, and built-in step implementations.

This is partly legitimate app code, but it should not also encode publish CI snippets and publish report formatting in the same module that executes process steps.

**Recommendation**

Create `monochange_cli_runtime` only if it is intended as a reusable library; otherwise split internally:

- `cli_runtime::engine`: resolves inputs, evaluates `when`, executes steps, streams command output
- `cli_runtime::templates`: builds template context and interpolation
- `cli_runtime::builtins`: maps built-in step names to service calls
- `cli_runtime::render`: CLI/markdown/json result rendering
- `cli_runtime::ci`: GitHub/GitLab publish batch snippet rendering, or move this to `monochange_publish`

The service calls should go through `monochange_workspace`, `monochange_release`, `monochange_publish`, and provider registries rather than calling large functions from `monochange` directly.

### 7. Move init/config-generation logic out of workspace release operations

**Current file**

- `crates/monochange/src/workspace_ops.rs`

**Issue**

`init_workspace`, `populate_workspace`, `render_annotated_init_config`, GitHub workflow generation, and TOML rendering are unrelated to discovery/release preparation. They are project scaffolding concerns.

**Recommendation**

Create `monochange_init` or a `workspace_init` module/crate:

- remote URL parsing
- annotated `monochange.toml` generation
- default CLI command rendering
- initial GitHub workflow generation

This would make the workspace/release planning crate more focused and avoid pulling init scaffolding into library users that only need discovery/release planning.

### 8. Move migration/subagent/skill features out of core app library

**Current files**

- `crates/monochange/src/migration_audit.rs`
- `crates/monochange/src/skill.rs`
- `crates/monochange/src/subagents.rs`
- MCP exposure in `crates/monochange/src/mcp.rs`

**Issue**

These are CLI product features, not release-planning domain logic. Keeping them in the same crate as release planning makes `monochange` hard to reason about as a library.

**Recommendation**

Keep them behind CLI modules or move to optional crates:

- `monochange_assistant` for skill/subagent/MCP assistant workflows
- `monochange_migration` for migration audit helpers

This is lower priority than publish/workspace/release extraction, but it helps keep the top-level app thin.

## Proposed new crate map

High priority:

- `monochange_workspace`: discovery orchestration, configured package loading, lockfile command planning, prepare-release orchestration.
- `monochange_release`: changeset-to-release-plan orchestration, release targets, release manifests, release records, provider request construction.
- `monochange_changelog`: changelog/release-notes engine.
- `monochange_publish`: publish request/report/resume/rate-limit/readiness/bootstrap orchestration.
- `monochange_adapter` or core traits in `monochange_core`: ecosystem/provider adapter traits and registries.

Medium priority:

- `monochange_versioning`: generic versioned-file update engine, with ecosystem-specific update implementations in ecosystem crates.
- `monochange_sources` or expanded `monochange_hosting`: provider registry and provider orchestration.
- `monochange_init`: project initialization/scaffolding.

Optional/lower priority:

- `monochange_cli_runtime`: reusable workflow engine if intended to be embedded outside the CLI.
- `monochange_cli_output`: terminal/markdown/json rendering helpers.
- `monochange_assistant`: MCP, skill, subagent helpers.

## Dependency corrections

The biggest dependency smell is `monochange_config` depending on all ecosystems and providers. Config parsing should be near the bottom of the graph, not a composition layer. It currently imports concrete crates for:

- supported versioned file kinds by ecosystem
- source provider capabilities
- source provider configuration validation

Fix by adding capability/validation registries:

```rust
pub struct ValidationContext<'a> {
	pub ecosystems: &'a dyn EcosystemCapabilities,
	pub sources: &'a dyn SourceCapabilities,
}
```

or by moving static metadata into `monochange_core` if it has no heavy dependencies.

After this correction, `monochange_config` should depend only on `monochange_core` plus parsing/lint helpers.

## Suggested incremental plan

1. **Introduce adapter traits and registries** in `monochange_core` or `monochange_adapter` without moving behavior.
2. **Add adapter constructors** to ecosystem/provider crates and wire them in `monochange`.
3. **Extract `monochange_changelog`** first. It is high value and mostly pure/domain logic.
4. **Extract `monochange_publish` report/resume/rate-limit/readiness types** before moving commands. This reduces the largest top-level file safely.
5. **Move ecosystem publish command/placeholder/version-exists functions** into ecosystem crates one ecosystem at a time.
6. **Extract `monochange_workspace` discovery orchestration** and replace hardcoded match arms with registry calls.
7. **Move provider orchestration** into `monochange_hosting`/`monochange_sources` and remove provider dispatch from config.
8. **Split `release_artifacts.rs`** into release domain, versioning, and CLI output.
9. **Reduce `monochange::lib` to facade wrappers** and keep existing public API compatibility until downstream callers can migrate.

## What `monochange` should look like at the end

The top-level crate should mostly contain:

- `main.rs` / CLI entry
- CLI argument construction and command routing
- adapter registry construction under feature flags
- facade compatibility functions:
  - `discover_workspace(...)`
  - `plan_release(...)`
  - `prepare_release(...)`
  - `run_with_args(...)`
- user-facing output rendering that is truly CLI-specific

It should not contain npm/Cargo/Dart/Python/Go/JSR publish commands, manifest update implementations, registry version checks, provider trust-context details, or changelog/release-note engines.

## Implementation progress

### 2026-05-07: publish capability/model extraction

Started the publish split with a low-risk boundary extraction:

- Added `crates/monochange_publish`.
- Moved trusted-publishing CI identity detection and registry/provider capability matrix out of `crates/monochange/src/trust_capabilities.rs`.
- Moved publish report/status/request data models out of `crates/monochange/src/package_publish.rs`.
- Moved built-in publish command construction for npm/pnpm, Cargo, Dart/Flutter, JSR, PyPI, and Go proxy tagging out of `crates/monochange/src/package_publish.rs`.
- Kept `crates/monochange/src/package_publish.rs` as the current orchestration implementation, but it now imports/re-exports the publish domain models and command builders from `monochange_publish`.

This keeps existing callers stable while establishing the crate that later registry checks, readiness, rate-limit, and resume logic can move into incrementally.

### 2026-05-08: Phase 2 publish boundary progress

Continued reducing `crates/monochange/src/package_publish.rs` after the initial `monochange_publish` split:

- #404 moved registry endpoint/client infrastructure and registry publishability checks into `monochange_publish`.
- #409 moved `CommandExecutor`, `ProcessCommandExecutor`, and command rendering helpers into `monochange_publish`.
- #412 moved publish resume/report artifact handling and publish dependency ordering into `monochange_publish`.
- #413 moved Cargo publish readiness blockers and Cargo workspace manifest helpers into `monochange_cargo`.
- #417 moved GitHub trust context resolution, workflow/job/environment parsing, trust-list matching, and GitHub identity verification into `monochange_github`.

Current `package_publish.rs` is about 5.7k lines. #419 moves npm trusted-publishing command construction (`build_npm_trust_list_command`, `build_npm_trust_command`, `render_npm_trust_command`, `append_npm_trust_environment_arg`, and shared npm CLI wrapping) plus npm placeholder `package.json` generation into `monochange_npm`, while keeping the current top-level trusted-publishing and placeholder orchestration in `monochange` until adapter traits exist.

### 2026-05-08: file-based release record changes

#408 added file-based release record history and deduplication helpers. This reinforces the release-boundary recommendation: `release_artifacts.rs`, `release_record.rs`, and the very large `crates/monochange/src/__tests.rs` now contain more release-record filesystem/history behavior that should eventually move into a focused `monochange_release` or release-record service crate rather than remaining in the app crate.

### 2026-05-08: ecosystem constants and validation moved down

The latest `origin/main` moved ecosystem constants and versioned-file validation out of `monochange_core` and delegated them to ecosystem crates. That change reinforces the same boundary direction as the publish work: ecosystem-owned file formats and validation should stay in `monochange_cargo`, `monochange_npm`, `monochange_dart`, `monochange_deno`, `monochange_python`, and `monochange_go`, while the top-level crate should compose those crates rather than reimplementing their rules.

### 2026-05-08: Go placeholder boundary follow-up

After the npm helper extraction, the next focused follow-up moves Go placeholder `go.mod` generation into `monochange_go`. This keeps Go module bootstrap formatting with the Go ecosystem adapter while `package_publish.rs` still owns the temporary-directory orchestration until `PublishAdapter` exists.

### 2026-05-09: JSR placeholder boundary follow-up

After the npm and Go placeholder extractions, the next focused follow-up moves JSR placeholder `deno.json` and `mod.ts` generation into `monochange_deno`. This keeps Deno/JSR package scaffold rules with the Deno ecosystem adapter while `package_publish.rs` still owns temporary-directory orchestration until `PublishAdapter` exists.
