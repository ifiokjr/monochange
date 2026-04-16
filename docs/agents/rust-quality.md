# Rust quality and safety

Use this guide when writing or refactoring Rust in monochange. It internalizes the highest-value Rust rules that agents should apply by default in this repository.

## Always

- `unsafe_code` is denied.
- `unstable_features` is denied.
- Keep formatting and clippy checks passing.
- Run `fix:all` after Rust changes, then run the relevant validation commands.
- Return `Result` for expected failures instead of panicking.
- Do not use `.unwrap()` in production code.
- Use `.expect(...)` only for invariant violations or impossible states, and make the message specific.
- Prefer explicit error context over bare `.expect(...)` or context-free error propagation.
- Document public items with `///` and add `# Errors`, `# Panics`, or `# Safety` sections when they apply.
- Update docs, tests, fixtures, and snapshots when behavior changes.

## Ownership and borrowing

- Prefer borrowing over cloning.
- Make every clone explicit and easy to justify.
- Accept borrowed inputs in public APIs:
  - use `&str` instead of `&String`
  - use `&[T]` instead of `&Vec<T>`
  - use `impl AsRef<T>` or `impl Into<T>` only when that flexibility meaningfully improves the API
- Move large values instead of cloning them.
- Reuse allocations when possible with `clear`, `clone_from`, `with_capacity`, or similar patterns.
- Use shared ownership deliberately:
  - `Rc<T>` for single-threaded shared ownership
  - `Arc<T>` for cross-thread shared ownership
  - `RefCell<T>`, `Mutex<T>`, or `RwLock<T>` only when interior mutability is actually required

## Error handling

- Prefer crate-specific error types over `Box<dyn Error>`.
- Use `thiserror` for library-style error types.
- Use `anyhow` only at application boundaries where typed errors are not useful.
- Propagate errors with `?`.
- Add context with `.context(...)` or `.with_context(...)` when a failure would otherwise be hard to diagnose.
- Use `#[from]` and `#[source]` when building composable error enums.
- Keep error messages lowercase and omit trailing punctuation.
- Add a `# Errors` section to docs for fallible public functions.

## API design

- Prefer validated domain types and enums over stringly typed APIs.
- Introduce newtypes for identifiers and other semantically distinct values.
- Parse and validate data at the boundary, then pass validated types through the system.
- Use builder patterns when construction has many optional or dependent fields.
- Add `#[must_use]` to important return values, especially builders and meaningful `Result`-returning helpers.
- Implement `From` instead of `Into`.
- Keep visibility as narrow as possible with `pub(crate)` and `pub(super)` until wider exposure is necessary.

## Async and concurrency

- Never hold a `Mutex` or `RwLock` guard across `.await`.
- Clone or extract the needed data before awaiting.
- Use Tokio-native APIs in async code, such as `tokio::fs` instead of `std::fs`.
- Use `tokio::join!`, `tokio::try_join!`, and `tokio::select!` when they make concurrent intent clearer.
- Use `spawn_blocking` for blocking or CPU-heavy work that would otherwise stall the async runtime.
- Prefer bounded channels when building queues or pipelines so backpressure stays explicit.
- Use cancellation-aware patterns for long-lived tasks.

## Performance and allocation discipline

- Favor iterators over manual indexing unless indexing is measurably clearer or faster.
- Avoid intermediate `collect()` calls when the pipeline can stay lazy.
- Pre-allocate with `with_capacity()` when the size is known or cheap to estimate.
- Reuse buffers and collections inside loops instead of reallocating them.
- Avoid `format!()` in hot paths when `write!()` or direct pushes are sufficient.
- Profile before making complex optimizations, but still avoid obvious clone and allocation anti-patterns.
- If a type is performance-sensitive, keep its layout compact and consider boxing large enum variants.

## Style and naming

- Follow standard Rust naming conventions:
  - `UpperCamelCase` for types and enum variants
  - `snake_case` for functions, methods, modules, and variables
  - `SCREAMING_SNAKE_CASE` for constants
- Treat acronyms as words in Rust identifiers, such as `Uuid` instead of `UUID`.
- Prefer simple, flat control flow with early returns.
- Add comments to explain why code exists or why a non-obvious tradeoff was chosen, not to narrate obvious steps.
- Follow [`coding-style.md`](coding-style.md) for visual layout, whitespace, grouping, and extraction guidance.

## Tests and documentation

- Use `#[cfg(test)] mod tests` for unit tests and `tests/` for integration coverage.
- Write focused tests with descriptive names.
- Keep doc examples runnable and prefer `?` over `.unwrap()` in docs.
- Prefer traits and dependency injection patterns that keep code testable without over-abstracting.

## Avoid

- `.unwrap()` in production code
- panic-based handling for expected failures
- `&String` and `&Vec<T>` in APIs when `&str` and `&[T]` work
- holding locks across `.await`
- stringly typed interfaces when enums or newtypes express intent better
- unnecessary clones, intermediate allocations, and eager `collect()` calls
- comments that restate obvious code instead of explaining the reason for a choice

## Review checklist for agents

Before finishing Rust changes, verify that:

- ownership is clear and clones are justified
- public APIs borrow where possible and use strong types
- expected failures return `Result` with useful context
- async code does not hold locks across `.await`
- obvious allocation and iterator anti-patterns are avoided
- docs and tests were updated when behavior changed
- formatting, clippy, and relevant validation commands pass
