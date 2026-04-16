# Manifest linting with `mc check`

monochange can lint monorepo package manifests through `mc check`, using rules configured under `[lints]` in `monochange.toml`.

<!-- {=lintingPolicyReference} -->

Use this guide when the task is to configure or explain monochange's **manifest lint rules**.

These are the rules that run through **`mc check`** and are configured in `monochange.toml` under **`[ecosystems.<name>.lints]`**.

They are separate from Rust compiler or Clippy lints used to develop monochange itself.

## What `mc check` does

`mc check` runs two phases:

1. normal workspace validation, similar to `mc validate`
2. manifest lint rules for supported package ecosystems

Common commands:

```bash
mc check
mc check --fix
mc check --format json
```

Use `--fix` when you want monochange to apply auto-fixes where a rule supports them.

## Where lint rules live

Configure rules per ecosystem in `monochange.toml`:

```toml
[ecosystems.cargo.lints]
"cargo/dependency-field-order" = "error"
"cargo/internal-dependency-workspace" = "error"
"cargo/required-package-fields" = { level = "warning", fields = ["description", "license", "repository"] }
"cargo/sorted-dependencies" = "warning"
"cargo/unlisted-package-private" = { level = "warning", fix = true }

[ecosystems.npm.lints]
"npm/workspace-protocol" = "error"
"npm/sorted-dependencies" = "warning"
"npm/required-package-fields" = { level = "warning", fields = ["description", "repository", "license"] }
"npm/root-no-prod-deps" = "error"
"npm/no-duplicate-dependencies" = "error"
"npm/unlisted-package-private" = { level = "warning", fix = true }
```

Rule configuration supports two forms:

- simple severity: `"rule-id" = "error"`, `"warning"`, or `"off"`
- detailed config: `{ level = "error", ...rule_specific_options }`

## Current rule coverage

Today, built-in manifest lint rules exist for:

- **Cargo** manifests (`Cargo.toml`)
- **npm-family** manifests (`package.json`)

monochange already wires lint configuration through `configuration.cargo.lints`, `configuration.npm.lints`, `configuration.deno.lints`, and `configuration.dart.lints`, but the current built-in rule sets are implemented for Cargo and npm manifests.

## Cargo manifest lint rules

### `cargo/dependency-field-order`

**Why:** keeps inline dependency tables visually consistent.

**What it checks:** preferred key order inside dependency tables:

1. `workspace` or `version`
2. `default-features` / `default_features`
3. `features`
4. other keys like `optional`, `path`, `registry`, `package`, `git`, `branch`, `tag`, `rev`

**Without the rule:**

```toml
serde = { features = ["derive"], workspace = true }
```

**With the rule:**

```toml
serde = { workspace = true, features = ["derive"] }
```

**Useful option:**

- `fix` — defaults to `true`

### `cargo/internal-dependency-workspace`

**Why:** internal workspace dependencies should usually be declared through the workspace rather than carrying their own explicit version strings.

**Without the rule:**

```toml
[dependencies]
monochange_core = { path = "../monochange_core", version = "0.1.0" }
```

**With the rule:**

```toml
[dependencies]
monochange_core = { workspace = true }
```

**When to use it:** when the repository wants one workspace-owned version source for internal crates.

**Useful options:**

- `require_workspace` — defaults to `true`
- `fix` — defaults to `true`

### `cargo/required-package-fields`

**Why:** published crates should consistently carry the metadata your repository expects.

**Default required fields:**

- `description`
- `license`
- `repository`

**Without the rule:**

```toml
[package]
name = "example"
version = "0.1.0"
```

**With the rule:** monochange reports the missing fields so package metadata stays consistent.

**Useful option:**

- `fields` — replace the default required-field list

Example:

```toml
[ecosystems.cargo.lints]
"cargo/required-package-fields" = { level = "error", fields = ["description", "license"] }
```

### `cargo/sorted-dependencies`

**Why:** alphabetized dependency tables are easier to review and reduce noisy diffs.

**Without the rule:**

```toml
[dependencies]
zzzz = "1.0"
aaaa = "1.0"
mmmm = "1.0"
```

**With the rule:**

```toml
[dependencies]
aaaa = "1.0"
mmmm = "1.0"
zzzz = "1.0"
```

**Useful option:**

- `fix` — defaults to `true`

### `cargo/unlisted-package-private`

**Why:** a Cargo package that is not listed in `monochange.toml` should not be accidentally publishable.

**Without the rule:** an unmanaged crate can remain publicly publishable by accident.

**With the rule:** monochange requires either:

- adding the package to `monochange.toml`, or
- marking it private with `publish = false`

**Without the rule:**

```toml
[package]
name = "experimental-crate"
version = "0.1.0"
```

**With the rule:**

```toml
[package]
name = "experimental-crate"
version = "0.1.0"
publish = false
```

**Useful option:**

- `fix` — defaults to `true`

## npm-family manifest lint rules

### `npm/workspace-protocol`

**Why:** internal workspace dependencies should use the `workspace:` protocol so local workspace intent is explicit.

**Without the rule:**

```json
{
	"dependencies": {
		"@acme/shared": "^1.2.0"
	}
}
```

**With the rule:**

```json
{
	"dependencies": {
		"@acme/shared": "workspace:*"
	}
}
```

**When to use it:** npm, pnpm, and Bun workspaces where internal packages should not drift to plain registry ranges.

**Useful options:**

- `require_for_private` — defaults to `false`
- `fix` — defaults to `true`

### `npm/sorted-dependencies`

**Why:** alphabetized dependency sections reduce review noise and make package diffs easier to scan.

**Without the rule:**

```json
{
	"dependencies": {
		"zod": "^4.0.0",
		"chalk": "^5.0.0"
	}
}
```

**With the rule:**

```json
{
	"dependencies": {
		"chalk": "^5.0.0",
		"zod": "^4.0.0"
	}
}
```

**Useful option:**

- `fix` — defaults to `true`

### `npm/required-package-fields`

**Why:** package metadata should stay consistent across publishable npm packages.

**Default required fields:**

- `description`
- `repository`
- `license`

**Without the rule:**

```json
{
	"name": "@acme/app",
	"version": "1.0.0"
}
```

**With the rule:** monochange reports the missing metadata fields.

**Useful option:**

- `fields` — replace the default required-field list

### `npm/root-no-prod-deps`

**Why:** the workspace root `package.json` is usually orchestration-only and should keep runtime dependencies out of the root package.

**Without the rule:**

```json
{
	"dependencies": {
		"react": "^19.0.0"
	}
}
```

**With the rule:** move those to `devDependencies` when the root package is only a workspace manager.

**Useful option:**

- `fix` — defaults to `true`

### `npm/no-duplicate-dependencies`

**Why:** the same dependency should not appear in multiple dependency sections unless the repository has a very deliberate reason.

**Without the rule:**

```json
{
	"dependencies": {
		"typescript": "^5.0.0"
	},
	"devDependencies": {
		"typescript": "^5.0.0"
	}
}
```

**With the rule:** monochange reports the duplicate and can suggest removing the redundant non-dev entry when appropriate.

**Useful option:**

- `fix` — defaults to `true`

### `npm/unlisted-package-private`

**Why:** a package not declared in `monochange.toml` should not remain publishable by accident.

**Without the rule:** an unmanaged package can still look publishable.

**With the rule:** monochange requires either:

- adding the package to `monochange.toml`, or
- marking it private in `package.json`

**Without the rule:**

```json
{
	"name": "@acme/experimental",
	"version": "0.1.0"
}
```

**With the rule:**

```json
{
	"name": "@acme/experimental",
	"private": true,
	"version": "0.1.0"
}
```

**Useful option:**

- `fix` — defaults to `true`

## What `mc check` looks like in practice

Use plain text for local review:

```bash
mc check
```

Apply safe auto-fixes where possible:

```bash
mc check --fix
```

Use JSON for CI or MCP-style tooling:

```bash
mc check --format json
```

`mc check` fails when lint errors are present, so it is appropriate for CI gates.

## Recommended workflow

For repository work:

```bash
mc validate
mc check
mc release --dry-run --diff
```

If you changed shared docs too:

```bash
devenv shell docs:check
```

<!-- {/lintingPolicyReference} -->
