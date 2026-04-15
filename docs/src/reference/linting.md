# Linting policy

monochange keeps linting strict enough to catch correctness and panic hazards, but not so strict that contributors spend all their time fighting style-only noise.

<!-- {=lintingPolicyReference} -->

Use this guide when the task is to explain, apply, or update monochange lint policy.

This reference reflects the current workspace lint configuration in the repository `Cargo.toml` plus the crate-level `#![forbid(clippy::indexing_slicing)]` declarations used across the Rust crates.

## Daily linting workflow

For normal repo work:

```bash
devenv shell fix:all
devenv shell lint:all
```

For documentation synchronization checks:

```bash
devenv shell docs:check
```

Use `docs:update` after editing shared `.templates/` content.

## How to read the policy

monochange uses a mix of:

- **workspace rust lints** — compiler-level safety and hygiene rules
- **workspace clippy groups** — broad quality buckets like correctness and performance
- **targeted clippy overrides** — rules we intentionally deny, warn, or allow
- **crate-level forbids** — stricter local rules when a specific panic pattern is unacceptable

The goal is not "never write interesting code." The goal is to avoid correctness bugs, avoid panic-prone indexing patterns, stay portable across Rust editions, and keep only the pedantic warnings that actually improve the codebase.

## Workspace rust lints

### `rust_2021_compatibility = warn`

**Why:** catches patterns that behave differently across editions.

**When to care:** when writing macros, pattern matches, or syntax that might become edition-sensitive.

**Without the rule:** edition migration issues can accumulate silently.

**With the rule:** you get an early warning before the next edition change becomes painful.

### `rust_2024_compatibility = warn`

**Why:** keeps the codebase ready for Rust 2024 semantics.

**When to care:** when introducing syntax or macro usage that may change in the 2024 edition.

**Without the rule:** future upgrades become a large cleanup project.

**With the rule:** new code is nudged toward edition-safe patterns now.

### `unsafe_code = deny`

**Why:** monochange should not rely on unchecked memory operations for release-planning logic.

**When to use:** almost always for business logic, config parsing, changelog generation, and CLI orchestration.

**Without the rule:**

```rust
unsafe {
    std::ptr::read(ptr)
}
```

Unsafe blocks can slip in and become maintenance hotspots.

**With the rule:** prefer safe standard-library APIs:

```rust
let first = values.first().copied();
```

### `unstable_features = deny`

**Why:** published tooling should compile on stable Rust.

**When to use:** for libraries and CLIs that must stay portable for contributors and CI.

**Without the rule:** nightly-only features can leak into the codebase.

**With the rule:** the code stays stable-channel compatible.

### `unused_extern_crates = warn`

**Why:** dead extern declarations add noise and make dependencies harder to audit.

**Without the rule:** old compatibility imports linger.

**With the rule:** unused declarations are cleaned up quickly.

### `unused_import_braces = warn`

**Why:** removes unnecessary syntax noise.

**Without the rule:**

```rust
use std::fmt;
```

**With the rule:**

```rust
use std::fmt;
```

### `unused_lifetimes = warn`

**Why:** unused lifetimes usually mean an API signature is more complex than necessary.

**Without the rule:**

```rust
fn name<'a>(value: &str) -> &str {
	value
}
```

**With the rule:**

```rust
fn name(value: &str) -> &str {
	value
}
```

### `unused_macro_rules = warn`

**Why:** dead macro arms are easy to forget and hard to test.

**Without the rule:** macro definitions keep stale branches.

**With the rule:** only exercised macro rules survive.

### `unused_qualifications = warn`

**Why:** fully qualifying names that are already in scope hurts readability.

**Without the rule:**

```rust
use std::path::PathBuf;

let path = std::path::PathBuf::new();
```

**With the rule:**

```rust
use std::path::PathBuf;

let path = PathBuf::new();
```

### `variant_size_differences = warn`

**Why:** very uneven enum variants can cause surprising memory bloat.

**Without the rule:** a single large variant can inflate every enum value.

**With the rule:** you consider boxing or reshaping the large variant.

### `edition_2024_expr_fragment_specifier = allow`

**Why it is allowed:** this lint is intentionally relaxed to avoid noisy churn while macro-related edition support settles.

**When the allowance is appropriate:** when the code is otherwise clear and no migration risk is introduced.

**Without the allowance:** the repo would force extra macro cleanups that do not improve the current product behavior.

## Workspace clippy groups

### `clippy::correctness = deny`

**Why:** correctness issues are the highest-risk category.

**Without it:** real bugs can land as "just warnings."

**With it:** code that is likely wrong fails the lint pass.

### `clippy::suspicious = warn`

**Why:** suspicious constructs often compile but suggest a logic mistake.

**Without it:** subtle mistakes look legitimate.

**With it:** you get a review checkpoint before the bug becomes user-visible.

### `clippy::style = warn`

**Why:** style warnings keep code predictable and easier to scan.

**Without it:** equivalent patterns drift across the codebase.

**With it:** contributors converge on the same idioms.

### `clippy::complexity = warn`

**Why:** overly complex code is harder to review and easier to break.

**Without it:** nested or overly clever logic grows unnoticed.

**With it:** clippy nudges you toward extraction and simpler control flow.

**Example of what this pressure is trying to prevent:**

```rust
if should_release {
    if let Some(group) = group {
        if group.publish {
            if !group.members.is_empty() {
                publish(group);
            }
        }
    }
}
```

A flatter version is easier to review:

```rust
let Some(group) = group else {
    return;
};

if !should_release || !group.publish || group.members.is_empty() {
    return;
}

publish(group);
```

### `clippy::perf = warn`

**Why:** hot-path inefficiencies are easier to fix when caught early.

**Without it:** unnecessary allocations and slower patterns blend in.

**With it:** common performance footguns get surfaced during normal linting.

### `clippy::pedantic = warn`

**Why:** pedantic lints catch a lot of polish issues that improve API and code quality.

**Why not deny:** the group is intentionally broad and sometimes noisy.

**monochange approach:** enable the group, then explicitly allow the few rules where local readability or practicality matters more.

## Explicit clippy policy

### `blocks_in_conditions = allow`

**Why it is allowed:** small computed conditions can be clearer inline than as a throwaway binding.

**Without the allowance:**

```rust
if {
    let ready = state.is_ready();
    ready
} {
    run();
}
```

Clippy would complain even when the structure is readable.

**With the current policy:** this pattern is allowed, but extract it if the block becomes non-trivial.

### `cargo_common_metadata = allow`

**Why it is allowed:** workspace metadata is managed centrally, so per-crate metadata completeness is not always the right enforcement point.

**Without the allowance:** clippy would push repetitive metadata into every crate even when the workspace already provides it.

**With the current policy:** add metadata where it matters, but do not create boilerplate just to silence the lint.

### `cast_possible_truncation = allow`

**Why it is allowed:** some numeric conversions are deliberate and guarded by domain knowledge.

**Without the allowance:**

```rust
let byte = value as u8;
```

would warn every time, even when the value is known to fit.

**With the current policy:** the cast is permitted, but reviewers should still expect surrounding reasoning or bounds checks when truncation is not obviously safe.

### `cast_possible_wrap = allow`

**Why it is allowed:** signed/unsigned conversions sometimes reflect external protocol or storage requirements.

**Without the allowance:** every deliberate sign-changing cast becomes noise.

**With the current policy:** use the cast intentionally and document tricky cases.

### `cast_precision_loss = allow`

**Why it is allowed:** some reporting or ratio calculations intentionally trade precision for convenience.

**Without the allowance:** floating-point conversions generate warnings even in non-critical display logic.

**With the current policy:** precision-loss casts are allowed, but avoid them in semver, version, or identity logic where exactness matters.

### `cast_sign_loss = allow`

**Why it is allowed:** conversions to unsigned values are sometimes part of external API shaping.

**Without the allowance:** routine boundary conversions become noisy.

**With the current policy:** keep the cast local and obvious.

### `expl_impl_clone_on_copy = allow`

**Why it is allowed:** an explicit `Clone` impl on a `Copy` type can occasionally be clearer or more controlled than a derive.

**Without the allowance:** clippy would force a derive-only style.

**With the current policy:** explicit impls are allowed when there is a concrete reason, not as default habit.

### `items_after_statements = allow`

**Why it is allowed:** tests and small helper scopes sometimes read better when local items appear near their use.

**Without the allowance:**

```rust
fn test_case() {
	let input = sample();

	fn sample() -> &'static str {
		"ok"
	}

	assert_eq!(input, "ok");
}
```

would warn.

**With the current policy:** that layout is acceptable when it improves locality.

### `missing_errors_doc = allow`

**Why it is allowed:** internal functions are numerous, and forcing `# Errors` on all of them creates noisy docs.

**Without the allowance:** every fallible helper would need a doc section.

**With the current policy:** still document errors on public APIs and non-obvious behavior, but do not require boilerplate on every internal helper.

### `missing_panics_doc = allow`

**Why it is allowed:** similar to `missing_errors_doc`, this avoids boilerplate on internal helpers.

**Without the allowance:** even intentionally internal panic paths need formal docs.

**With the current policy:** public or surprising panic behavior should still be documented deliberately.

### `module_name_repetitions = allow`

**Why it is allowed:** crate boundaries and domain naming sometimes make repetition the clearest choice.

**Without the allowance:**

```rust
mod release_record;
struct ReleaseRecord;
```

can trigger a warning even though the names are clear.

**With the current policy:** choose clarity over lint golf.

### `must_use_candidate = allow`

**Why it is allowed:** clippy suggests `#[must_use]` very aggressively.

**Without the allowance:** many private helpers would get noisy suggestions.

**With the current policy:** apply `#[must_use]` intentionally on public APIs, builders, and values where dropping the result is genuinely a bug.

### `no_effect_underscore_binding = allow`

**Why it is allowed:** intentionally ignored intermediate values sometimes help document intent in tests or command setup.

**Without the allowance:** underscore-prefixed bindings can still warn even when they make the code easier to follow.

**With the current policy:** use them sparingly and only when they clarify intent.

### `tabs-in-doc-comments = allow`

**Why it is allowed:** command output, tables, or copied terminal content may legitimately contain tabs.

**Without the allowance:** documentation cleanup would fight preserved examples.

**With the current policy:** tabs are acceptable in docs when they preserve exact formatting.

### `too_many_lines = allow`

**Why it is allowed:** some orchestration functions, renderers, or test modules are large for domain reasons.

**Without the allowance:** contributors would spend time splitting code purely to satisfy an arbitrary line limit.

**With the current policy:** long functions are allowed, but extraction is still preferred when it improves comprehension.

### `wildcard_dependencies = deny`

**Why:** published tools should not depend on unconstrained crate versions.

**Without the rule:**

```toml
serde = "*"
```

can make builds non-reproducible and difficult to audit.

**With the rule:** dependencies must be explicitly versioned.

### `wildcard_imports = allow`

**Why it is allowed:** some test modules and highly local scopes read better with a wildcard import.

**Without the allowance:**

```rust
use super::*;
```

would warn in common test layouts.

**With the current policy:** wildcard imports remain acceptable in narrow scopes, especially tests.

## Crate-level forbid: `clippy::indexing_slicing`

Most monochange Rust crates start with:

```rust
#![forbid(clippy::indexing_slicing)]
```

**Why:** indexing and slicing can panic, and monochange spends a lot of time parsing external files, manifests, and user input.

**Without the rule:**

```rust
let first = values[0];
let suffix = &text[1..];
```

These compile, but they panic on malformed or short input.

**With the rule:** prefer checked access:

```rust
let first = values.first().copied();
let suffix = text.get(1..);
```

**Another example in manifest parsing:**

```rust
let version = package_json["version"].as_str();
```

looks compact but assumes the key exists and the JSON shape is right. Checked access makes the failure mode explicit:

```rust
let version = package_json
    .get("version")
    .and_then(|value| value.as_str());
```

**When to use this stricter rule:** parsing, config loading, release planning, changelog rendering, and any code that handles external repository state.

## What "with and without linting" looks like in practice

### Without the monochange lint posture

- unsafe or nightly-only code can sneak in
- panic-prone indexing is easier to miss
- wildcard dependencies can weaken reproducibility
- edition migration issues accumulate quietly
- pedantic improvements never surface

### With the current monochange lint posture

- correctness issues fail fast
- suspicious, style, complexity, performance, and pedantic issues show up in review
- some noisy lints are intentionally relaxed where the team prefers readability or lower boilerplate
- panic-prone indexing is blocked at crate level

## When to add a local `#[allow(...)]`

A local allow is acceptable when:

- the lint is technically correct but the preferred alternative is harder to read
- the code is constrained by a protocol, generated shape, or test pattern
- you can explain the exception in one sentence

Example:

```rust
#[allow(clippy::too_many_arguments)]
fn build_release_payload(
	owner: &str,
	repo: &str,
	version: &str,
	tag: &str,
	notes: &str,
	draft: bool,
	prerelease: bool,
) {
	// ...
}
```

Use this sparingly. If the function can be improved with a struct or builder, prefer that.

## Recommended validation loop after edits

```bash
devenv shell fix:all
devenv shell lint:all
mc validate
```

If you changed shared docs too:

```bash
devenv shell docs:check
```

<!-- {/lintingPolicyReference} -->
