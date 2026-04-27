## [0.3.0](https://github.com/monochange/monochange/releases/tag/v0.3.0) (2026-04-27)

### Testing

#### test

Add cargo-mutants regression coverage for `monochange_analysis`, `monochange_config`, `monochange_core`, and `monochange_hosting`.

- `monochange_analysis`: add tests for `latest_workspace_release_tag` to ignore namespaced tags, `snapshot_files_from_working_tree` and `read_text_file_from_git_object` for medium/exact-size files, `detect_raw_pr_environment` to reject non-PR events across CI providers, `default_branch_name` with origin HEAD symbolic ref, `get_merge_base` returning actual merge base, and `ChangeFrame::changed_files` distinguishing working directory from staged-only.
- `monochange_config`: add fixture-backed tests and proptests for ecosystem versioned-file inheritance, explicit group bump inference, and changelog validation defaults to close mutation gaps in release-planning configuration loading.
- `monochange_core`: add fixture-backed tests for discovery filtering around parent `.git` directories outside the workspace root and block-comment stripping edge cases in JSON helper logic.
- `monochange_hosting`: add HTTP-mock tests for error paths in `get_json`, `get_optional_json`, `post_json`, `put_json`, and `patch_json` to kill mutants on status-code checks. Update `Cargo.toml` to include `src/*.rs` in the distribution manifest so `__tests.rs` builds correctly with `httpmock` as a dev-dependency.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #277](https://github.com/monochange/monochange/pull/277) _Introduced in:_ [`4c411d9`](https://github.com/monochange/monochange/commit/4c411d9efe84aaefbe2231ac16e4065249fc2a06)

### Changed

#### Update repository references from `ifiokjr/monochange` to `monochange/monochange`.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #284](https://github.com/monochange/monochange/pull/284) _Introduced in:_ [`021a6cb`](https://github.com/monochange/monochange/commit/021a6cbc86f812a7879b211e83ced5074dccf740)

## [0.2.0](https://github.com/monochange/monochange/releases/tag/v0.2.0) (2026-04-21)

### Added

#### add semantic change analysis crate

Introduces a new `monochange_analysis` crate that provides intelligent, artifact-aware changeset generation for the monochange ecosystem.

**What it does:**

The crate analyzes git diffs and suggests granular changesets based on the type of code being changed:

- **Libraries**: Detects public API changes (new functions, types, traits)
- **Applications**: Identifies UI components, routes, and state changes
- **CLI tools**: Extracts command and flag modifications

**Key features:**

- **Change frame detection**: Automatically detects what to analyze based on git state (working directory, branches, PRs, CI/CD environments)
- **Artifact type classification**: Determines if a package is a library, application, CLI tool, or mixed artifact
- **Semantic extraction**: Three levels of analysis - basic (file-level), signature (function/type signatures), and semantic (full AST)
- **Adaptive grouping**: Configurable thresholds for grouping related changes vs. creating separate changesets

**Example usage:**

```rust
use monochange_analysis::{
    analyze_changes,
    ChangeFrame,
    AnalysisConfig,
    DetectionLevel,
};

// Auto-detect the change frame
let frame = ChangeFrame::detect(Path::new("."))?;

let config = AnalysisConfig {
    detection_level: DetectionLevel::Signature,
    ..Default::default()
};

let analysis = analyze_changes(Path::new("."), &frame, &config)?;

// Get suggested changesets per package
for (package_id, pkg) in &analysis.package_changes {
    for cs in &pkg.suggested_changesets {
        println!("{}: {}", package_id, cs.summary);
    }
}
```

**Supported CI/CD environments:**

- GitHub Actions
- GitLab CI
- CircleCI
- Travis CI
- Azure Pipelines
- Buildkite

This crate is the foundation for the new `mc analyze` command and MCP tools that help agents generate better changesets automatically.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #206](https://github.com/monochange/monochange/pull/206) _Introduced in:_ [`a417022`](https://github.com/monochange/monochange/commit/a417022f80f93d61add00b8087e0f80102a9fd52) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add per-crate Codecov coverage flags and crate-specific coverage badges

monochange now uploads one Codecov coverage flag per public crate while keeping the existing workspace-wide upload.

**Before:**

- Codecov only received the overall workspace LCOV upload
- crate READMEs linked their coverage badge to the shared repository-wide Codecov page
- Codecov patch coverage enforced a 100% target for PR status checks

**After:**

- CI splits the workspace LCOV report into one upload per public crate using a Codecov flag named after the crate
- each published crate README now points its coverage badge at that crate’s own Codecov flag page, for example `?flag=monochange_core`
- the repository keeps the overall workspace coverage upload and lowers the Codecov patch coverage status target to 95%

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #255](https://github.com/monochange/monochange/pull/255) _Introduced in:_ [`26e13ff`](https://github.com/monochange/monochange/commit/26e13fff071e93dc32fe071a5771232c980ebd46) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

#### add ecosystem-specific semantic analysis for MCP changeset workflows

`monochange_analyze_changes` and `monochange_validate_changeset` now return real semantic analysis for Cargo, npm, Deno, and Dart/Flutter packages instead of placeholder results.

**Before (`monochange_analyze_changes`):**

```json
{
	"ok": true,
	"summary": "Analysis complete - review suggested changesets for each package",
	"analysis": {
		"package_changes": {}
	}
}
```

**After (`monochange_analyze_changes`):**

```json
{
	"ok": true,
	"summary": "Analyzed 1 package(s) and found 6 semantic change(s)",
	"analysis": {
		"packageAnalyses": {
			"core": {
				"semanticChanges": [
					{
						"category": "public_api",
						"kind": "modified",
						"itemPath": "greet"
					},
					{
						"category": "dependency",
						"kind": "added",
						"itemPath": "tracing"
					}
				]
			}
		}
	}
}
```

`monochange_validate_changeset` now validates authored changesets against that semantic diff and can flag stale symbol references or underspecified summaries across all supported ecosystems.

Current analyzer coverage includes:

- Cargo public Rust API diffs plus `Cargo.toml` dependency and manifest metadata changes
- npm-family JS/TS exported symbol diffs plus `package.json` exports, commands, dependency, and script changes
- Deno JS/TS exported symbol diffs plus `deno.json` exports, import aliases, task, and compiler-option changes
- Dart and Flutter public `lib/` API diffs plus `pubspec.yaml` executables, dependency, environment, and plugin-platform changes

**Before (`monochange_validate_changeset`):**

```json
{
	"ok": true,
	"valid": true,
	"issues": []
}
```

**After (`monochange_validate_changeset`):**

```json
{
	"ok": false,
	"valid": false,
	"lifecycle_status": "stale",
	"issues": [
		{
			"severity": "error",
			"message": "changeset references `OldGreeter` but that item was not found in the current semantic diff"
		}
	]
}
```

`monochange_core` now exposes shared semantic-analysis contracts and diff record types so ecosystem crates can own their analyzers without moving parser logic into the CLI crate.

`@monochange/skill` now documents the semantic-analysis-backed MCP workflows and the expanded cross-ecosystem validation guidance for assistant consumers.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #247](https://github.com/monochange/monochange/pull/247) _Introduced in:_ [`8c96c8f`](https://github.com/monochange/monochange/commit/8c96c8f0a3b9d44bf30148b5a83067d7ce3ab62b) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Closed issues:_ [#243](https://github.com/monochange/monochange/issues/243) _Related issues:_ [#244](https://github.com/monochange/monochange/issues/244)

#### improve npm and Deno semantic analysis with parser-backed JS/TS export extraction

`monochange_analyze_changes` and `monochange_validate_changeset` now use parser-backed JavaScript and TypeScript export extraction for npm and Deno packages instead of relying primarily on line-based export scanning.

This improves semantic diff accuracy for cases such as:

- multiline named export blocks
- namespace re-exports like `export * as toolkit from "./toolkit"`
- anonymous default exports
- more complex TypeScript and module export syntax

The MCP output shape stays the same, but the semantic evidence for npm and Deno packages is now more robust and closer to the actual module structure.

This work also extracts the shared JavaScript and TypeScript export-analysis logic into the new `monochange_ecmascript` crate, so npm and Deno keep their ecosystem-specific manifest analysis while reusing one parser-backed module analyzer.

The monochange skill documentation now also teaches the new-package rule: the first changeset for a newly introduced published package or crate should use a `major` bump for that new package entry.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #250](https://github.com/monochange/monochange/pull/250) _Introduced in:_ [`0dd8460`](https://github.com/monochange/monochange/commit/0dd846060614b2de9d3b2dfb5c1337075774b167) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#247](https://github.com/monochange/monochange/issues/247), [#249](https://github.com/monochange/monochange/issues/249)

#### `monochange_analysis` can now return release-aware semantic analysis across three explicit frames:

- `release -> main`
- `main -> head`
- `release -> head`

This adds a first multi-frame API surface for issue #249, including explicit ref-based entry points plus automatic baseline resolution that uses the latest workspace-style release tag and the detected default branch.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #252](https://github.com/monochange/monochange/pull/252) _Introduced in:_ [`d1dce9d`](https://github.com/monochange/monochange/commit/d1dce9d880a1739253f5dccc3cd7cc73431b2b41) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2) _Related issues:_ [#249](https://github.com/monochange/monochange/issues/249)

### Refactor

#### align crate docs and readability with the workspace style guide

This pass improves readability and documentation consistency across the workspace without changing release behavior or public APIs.

**What changed:**

- extracted shared crate-level docs into `.templates/crates.t.md` and reused them from Rust `lib.rs` module docs and crate readmes
- added missing readmes and module docs for `monochange_analysis`, `monochange_hosting`, and `monochange_test_helpers`
- rewrote a few nested control-flow sections into flatter early-return or `match`-based forms in `monochange`, `monochange_config`, `monochange_gitea`, `monochange_gitlab`, `monochange_npm`, and the shared test helpers
- replaced duplicated fixture-copy helpers in `monochange_cargo` and `monochange_core` tests with the shared `monochange_test_helpers::copy_directory` utility

**Before:**

```rust
if let Some(existing_pr) = &existing {
    if content_matches {
        // skipped response
    } else {
        // update response
    }
} else {
    // create response
}
```

**After:**

```rust
match existing {
    Some(existing_pr) if content_matches => {
        // skipped response
    }
    Some(existing_pr) => {
        // update response
    }
    None => {
        // create response
    }
}
```

The result is more consistent crate documentation, less duplicated prose, and flatter control flow in a few high-traffic code paths.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #224](https://github.com/monochange/monochange/pull/224) _Introduced in:_ [`d0f76ed`](https://github.com/monochange/monochange/commit/d0f76ed56fa18e0ca9d9ec20fa9e44d413014db7) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)

### Testing

#### add core linting types

Add `monochange_core::lint` module with the foundational types for the linting system:

- `LintSeverity` (Off, Warning, Error) — rule severity levels
- `LintCategory` (Style, Correctness, Performance, Suspicious, BestPractice) — rule classification
- `LintRule` — rule definition with id, name, description, and autofixable flag
- `LintResult`, `LintLocation` — individual findings with file location and byte spans
- `LintFix`, `LintEdit` — autofix suggestions with span-based replacements
- `LintRuleConfig` — flexible configuration supporting simple severity or detailed options
- `LintReport` — aggregated results with error/warning counts
- `LintContext` — rule input with workspace root, manifest path, and file contents
- `LintRuleRunner` trait — executable rule interface with `rule()`, `applies_to()`, and `run()`
- `LintRuleRegistry` — rule registration and discovery

Also adds `lints` field to `EcosystemSettings` for per-ecosystem lint configuration and `Lint` variant to `CliStepDefinition` with `format`, `fix`, and `ecosystem` inputs.

> _Owner:_ [@ifiokjr](https://github.com/ifiokjr) _Review:_ [PR #207](https://github.com/monochange/monochange/pull/207) _Introduced in:_ [`a650862`](https://github.com/monochange/monochange/commit/a650862f2dc69b6538f6403bab5b66079f9c1304) _Last updated in:_ [`2bd10ab`](https://github.com/monochange/monochange/commit/2bd10abcd34e0eca9f75cebdfafdf6347dc84ca2)
