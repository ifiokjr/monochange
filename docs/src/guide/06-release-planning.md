# Release planning

Create a change input file with the CLI:

```bash
mc changes add --root . --package crates/sdk_core --bump minor --reason "public API addition"
```

Or write one manually:

```toml
[[changes]]
package = "crates/sdk_core"
bump = "minor"
reason = "public API addition"
```

Optionally include Rust semver evidence:

```toml
[[changes]]
package = "crates/sdk_core"
reason = "breaking API change"
evidence = ["rust-semver:major:public API break detected"]
```

Generate a plan:

```bash
mc plan release --root . --changes changes.toml --format json
```

Planning rules in this milestone:

- direct changes default to `patch` when no explicit bump is supplied
- dependents default to the configured `parent_bump`
- Rust semver evidence can escalate both the changed crate and its dependents
- version-group synchronization runs before final output is rendered
