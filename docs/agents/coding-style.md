# Coding style

This guide defines the code aesthetics and layout standards for the monochange codebase. It focuses on **how code looks** rather than what it does—whitespace placement, comment positioning, code simplification patterns, and visual organization to maximize readability.

> **Note**: This guide does not dictate which functions, methods, or language features to use. Those decisions are documented in other guides (e.g., [Rust quality and safety](rust-quality.md), [Architecture rules](architecture.md)). This guide is purely about the _visual presentation_ of code.

## Philosophy

**Simple code is better than complex code.**

Given two implementations that achieve the same goal, choose the one that:

- Has fewer lines
- Has less nesting
- Requires less mental effort to follow
- Is easier to explain in words

> **Exception**: When security or performance requires complexity

When you must introduce complexity for security or performance reasons, **always add a comment explaining why**:

```rust
// Security: We must validate the signature before parsing
// to prevent malformed input from causing panic or undefined behavior
if !is_valid_signature(input) {
    return Err(Error::InvalidSignature);
}
```

## Core principles

### Whitespace is semantics

Blank lines are not just for decoration—they separate concepts and give the reader time to breathe.

**Where to place blank lines:**

1. **Before control flow statements**: Add blank lines before `if`, `match`, `for`, `while`, etc.
2. **Between logical groups**: Group related operations, then separate groups with blank lines
3. **After complex declarations**: Long variable declarations deserve breathing room
4. **Before return statements**: Unless it's the very next line after a short operation

```rust
// Good: Whitespace separates concerns
fn process_order(order: Order) -> Result<Receipt, Error> {
	// Group 1: Validation
	if order.items.is_empty() {
		return Err(Error::EmptyOrder);
	}

	if !order.payment_method.is_valid() {
		return Err(Error::InvalidPayment);
	}

	// Group 2: Calculation
	let subtotal = calculate_subtotal(&order.items);
	let tax = calculate_tax(subtotal, order.region);
	let total = subtotal + tax;

	// Group 3: Payment processing
	let payment_result = process_payment(order.payment_method, total)?;

	if !payment_result.success {
		return Err(Error::PaymentFailed);
	}

	// Group 4: Finalization
	let receipt = Receipt {
		order_id: order.id,
		total,
		transaction_id: payment_result.id,
	};

	Ok(receipt)
}
```

### Variables at the top

Declare variables and constants at the start of functions or at the top of files when possible. This establishes the "state" for what's about to happen.

```rust
// Good: State is established upfront
fn configure_server(config: &Config) -> Server {
	// Configuration extraction
	let port = config.port;
	let timeout = config.timeout_secs;
	let max_connections = config.max_connections;

	// Security settings
	let require_tls = config.environment == Environment::Production;

	// Build and return
	Server::builder()
		.port(port)
		.timeout(timeout)
		.max_connections(max_connections)
		.tls(require_tls)
		.build()
}
```

**Exception**: When a variable's value depends on a prior computation, declare it near where it's computed.

### Early returns over deep nesting

**Indentation is an orange flag.** Treat deeply nested code as a code smell.

**The rule**: If you find yourself more than 2-3 levels deep, refactor.

**Strategy**: Guard clauses and early returns

```rust
// Avoid: Deep nesting
fn handle_request(req: Request) -> Response {
	if let Some(user) = req.user {
		if user.is_active {
			if user.has_permission("read") {
				if let Some(data) = fetch_data() {
					Response::ok(data)
				} else {
					Response::not_found()
				}
			} else {
				Response::forbidden()
			}
		} else {
			Response::unauthorized()
		}
	} else {
		Response::unauthorized()
	}
}

// Prefer: Early returns
fn handle_request(req: Request) -> Response {
	let user = req.user.ok_or_else(|| Response::unauthorized())?;

	if !user.is_active {
		return Response::unauthorized();
	}

	if !user.has_permission("read") {
		return Response::forbidden();
	}

	let data = fetch_data().ok_or_else(|| Response::not_found())?;

	Response::ok(data)
}
```

### Extraction over nesting

When you can't avoid complex logic, extract it into smaller functions.

```rust
// Avoid: Complex nested logic
fn process_data(data: Data) -> Result {
	if let Some(items) = data.items {
		for item in items {
			if item.is_active {
				if let Some(value) = item.value {
					if value > threshold {
						// 20 lines of complex processing...
					}
				}
			}
		}
	}
}

// Prefer: Extract into focused functions
fn process_data(data: Data) -> Result {
	let active_items = data.active_items()?;

	for item in active_items {
		if let Some(value) = item.significant_value(threshold) {
			process_significant_item(item, value)?;
		}
	}

	Ok(())
}

fn process_significant_item(item: &Item, value: Value) -> Result {
	// 20 lines of focused processing...
}
```

### Comments explain why, not what

Comments should explain **why** code exists, not **what** it does (the code itself should be clear).

**Exception**: When security or performance requires non-obvious code, explain both what and why:

```rust
// Security: Constant-time comparison to prevent timing attacks
// We compare every byte regardless of mismatches to ensure
// the operation takes the same time regardless of where the
// first difference occurs
if !constant_time_eq(provided_hash, stored_hash) {
    return Err(Error::InvalidCredentials);
}
```

```rust
// Performance: Pre-allocate array to avoid reallocations
// This reduces GC pressure when processing large datasets
const results = new Array(estimatedSize);
```

### Documentation blocks

Every function, class, and module should have a documentation block explaining:

- **Purpose**: What does this do?
- **Why**: Why does this exist? What problem does it solve?

```rust
/// Validates a user session.
///
/// # Why this exists
/// Session validation is required before any privileged operation
/// to ensure the user is authenticated and their session hasn't expired.
///
/// # Security considerations
/// - This check must happen before any data access
/// - Session tokens are validated cryptographically
/// - Expired sessions are logged for security monitoring
fn validate_session(token: &str) -> Result<Session, Error> {
	// ...
}
```

## Language-specific guidelines

### Rust

For Rust code, the following patterns are encouraged:

- **Whitespace**: Add blank lines before control flow, between logical groups, after complex declarations
- **Early returns**: Use `let-else` syntax or `?` operator for flat structure
- **Variable grouping**: Group related `let` statements, separate from usage with blank lines
- **Section comments**: Use comments like `// === Configuration loading ===` to mark logical sections

**Always run the formatter and linter after editing:**

```bash
# 1. Format
rustfmt ./crates/monochange/src/lib.rs

# 2. Auto-fix what clippy can
rustclippy --fix -- ./crates/monochange/src/lib.rs

# 3. Check remaining issues
rustclippy -- ./crates/monochange/src/lib.rs
```

### TypeScript and JavaScript

- Use `prettier` for formatting
- Apply the same whitespace principles: blank lines before control flow, between logical groups
- Prefer early returns over deep nesting

### Markdown

- Use `dprint` for formatting documentation
- Add blank lines between sections
- Keep lines reasonably short for readability

## Integration with formatters

This style guide **complements**, not replaces, automated formatters:

- **Always use**: `rustfmt`, `prettier`, `dprint`, etc.
- **This guide covers**: Blank line placement, grouping, early return patterns, comment positioning
- **Formatters cover**: Indentation, trailing commas, spacing around operators, line length

Never fight the formatter on mechanical details. This guide addresses aesthetic choices that formatters don't make.

### Always run the formatter after editing

**Rule**: After editing any code file or markdown file, always run the project's formatter.

For this project:

- **dprint** - Universal formatter for many languages
- **rustfmt** - Rust code

Run the formatter on the specific files you edited. Auto-formatted code is essential for consistent codebases.

### Always run the linter and fix all issues

**Rule**: Run the project's linter after editing files.

**Warnings are errors**: Treat all linter warnings as errors.

- If a warning shouldn't exist, remove it from the lint settings
- Don't leave warnings in the codebase

**Auto-fix what you can**:

- Formatters auto-correct their own formatting issues
- Linters often have auto-fix for some issues: run the auto-fix first
- For remaining issues that require reasoning: fix them manually

## Summary

| Principle              | Rule                                            | Exception                                            |
| ---------------------- | ----------------------------------------------- | ---------------------------------------------------- |
| **Simplicity**         | Choose the simpler solution                     | When security or performance requires complexity     |
| **Whitespace**         | Blank lines before control flow, between groups | Short, tightly-coupled operations                    |
| **Variable placement** | Declare at top when possible                    | When value depends on prior computation              |
| **Nesting**            | Avoid more than 2-3 levels deep                 | When language idioms require it                      |
| **Comments**           | Explain why, not what                           | Security and performance require explanation of what |
| **Extraction**         | Break complex logic into small functions        | When it hurts performance                            |
| **Formatting**         | Run formatter after every edit                  | N/A - Always run it                                  |
| **Linting**            | Run linter after edits, fix all issues          | Only skip if linter is very slow                     |

**Remember**: Code is read far more often than it is written. Optimize for the reader.
