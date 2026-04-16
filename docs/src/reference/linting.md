# Manifest linting with `mc check`

monochange can lint monorepo package manifests through `mc check`, using rules configured under `[lints]` in `monochange.toml`.

<!-- {=lintingPolicyReference} -->

Use this guide when the task is to configure or explain monochange's **manifest lint rules**.

These are the rules that run through **`mc check`** and are configured in `monochange.toml` under the top-level **`[lints]`** section.

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
mc lint list
mc lint explain cargo/recommended
```

Use `--fix` when you want monochange to apply auto-fixes where a rule supports them.

## Where lint rules live

Configure presets, global rules, and scoped overrides in the top-level `[lints]` section of `monochange.toml`:

```toml
[lints]
use = ["cargo/recommended", "npm/recommended", "dart/recommended"]

[lints.rules]
"cargo/internal-dependency-workspace" = "error"
"npm/workspace-protocol" = "error"
"dart/sdk-constraint-modern" = { level = "warning", minimum = "3.6.0", require_upper_bound = false }
"dart/no-unexpected-dependency-overrides" = { level = "warning", allow_for_private = true, allow_packages = ["app_shell"] }

[[lints.scopes]]
name = "published cargo packages"
match = { ecosystems = ["cargo"], managed = true, publishable = true }
rules = { "cargo/required-package-fields" = "error" }
```

Rule configuration supports two forms:

- simple severity: `"rule-id" = "error"`, `"warning"`, or `"off"`
- detailed config: `{ level = "error", ...rule_specific_options }`

## Current rule coverage

Today, built-in manifest lint rules exist for:

- **Cargo** manifests (`Cargo.toml`)
- **npm-family** manifests (`package.json`)
- **Dart / Flutter** manifests (`pubspec.yaml`)

Lint suites still live in ecosystem crates, but monochange routes all manifest lint configuration through the top-level `[lints]` section via preset selection, rule overrides, and scoped matches.

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
[lints.rules]
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

## Dart manifest lint rules

### `dart/sdk-constraint-present`

**Why:** every managed Dart package should declare the SDK range it expects rather than inheriting whatever the developer machine happens to provide.

**With the rule:** monochange reports any `pubspec.yaml` that omits `environment.sdk`.

### `dart/sdk-constraint-modern`

**Why:** old or overly broad SDK ranges quietly expand your support policy and make releases harder to reason about.

**Default policy:**

- minimum lower bound: `3.0.0`
- upper bound required by default

**Useful options:**

- `minimum` — override the minimum lower bound for your workspace
- `require_upper_bound` — set to `false` if your policy intentionally omits an upper bound

Example:

```toml
[lints.rules]
"dart/sdk-constraint-modern" = { level = "warning", minimum = "3.6.0", require_upper_bound = false }
```

### `dart/dependency-sorted`

**Why:** alphabetized `dependencies`, `dev_dependencies`, and `dependency_overrides` blocks reduce review noise and make Dart manifest diffs easier to scan.

**Useful option:**

- `fix` — defaults to `true`

### `dart/no-unexpected-dependency-overrides`

**Why:** `dependency_overrides` are sometimes necessary, but they should usually be limited to private packages or a small allow list of explicitly approved packages.

**Useful options:**

- `allow_for_private` — defaults to `true`
- `allow_packages` — list package names that may keep `dependency_overrides`

Example:

```toml
[lints.rules]
"dart/no-unexpected-dependency-overrides" = { level = "warning", allow_for_private = true, allow_packages = ["app_shell"] }
```

### Workspace-aware Dart rules

### `dart/internal-path-dependency-policy`

**Why:** monorepos usually want one consistent policy for how internal Dart packages reference each other.

**Default policy:** strict mode expects internal packages to use `path:` references.

**Useful option:**

- `mode` — choose `"path"` or `"hosted"`

Example:

```toml
[lints.rules]
"dart/internal-path-dependency-policy" = { level = "error", mode = "hosted" }
```

### `dart/workspace-internal-version-consistency`

**Why:** when workspace packages reference each other with hosted version ranges, those ranges should not drift away from the current workspace version.

**With the rule:** monochange compares internal dependency version references against the discovered workspace package version and reports mismatches.

### Flutter-only rules

### `dart/flutter-package-metadata-consistent`

**Why:** packages with a `flutter` section should declare the Flutter SDK dependency consistently so they are unmistakably Flutter packages.

**With the rule:** monochange requires `dependencies.flutter = { sdk = flutter }` in `pubspec.yaml` terms, expressed as the YAML mapping form.

### `dart/assets-sorted`

**Why:** stable ordering for `flutter.assets` and `flutter.fonts` reduces noisy diffs in Flutter packages.

**Useful option:**

- `fix` — defaults to `true`

### Dart presets

- `dart/recommended` enables metadata/publishability checks, `dart/sdk-constraint-present`, and `dart/dependency-sorted` as a warning.
- `dart/strict` adds `dart/sdk-constraint-modern`, `dart/no-unexpected-dependency-overrides`, `dart/internal-path-dependency-policy`, `dart/workspace-internal-version-consistency`, `dart/flutter-package-metadata-consistent`, and `dart/assets-sorted`, while promoting `dart/dependency-sorted` to an error.

Use `mc lint list` to inspect registered rules and presets, and `mc lint explain <id>` to understand a rule or preset before enabling it.

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
