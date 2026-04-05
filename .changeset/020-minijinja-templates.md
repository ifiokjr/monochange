---
monochange: minor
monochange_config: minor
---

#### migrate all variable interpolation to minijinja templates

All variable substitution in `monochange.toml` now uses Jinja2 syntax (`{{ variable }}`) instead of the previous `{variable}` or `$variable` forms. This applies to:

- `[defaults.changelog]` path templates
- `Command` step `run` strings
- release-note `change_templates` bodies

**Before:**

```toml
[defaults.changelog]
path = "{path}/CHANGELOG.md"

[[cli.release.steps]]
type = "Command"
run = "cargo publish --package $package"
dry_run = "echo would publish $package"
```

**After:**

```toml
[defaults.changelog]
path = "{{ path }}/CHANGELOG.md"

[[cli.release.steps]]
type = "Command"
run = "cargo publish --package {{ package }}"
dry_run_command = "echo would publish {{ package }}"
```

CLI input values are available as template variables in `Command` steps, enabling conditionals and filters:

```toml
run = "cargo publish {% if dry_run %}--dry-run{% endif %}"
run = "cargo build --features {{ features | join(',') }}"
```

**`monochange_config`** uses `minijinja` for all path-template expansion. The old single-pass `{placeholder}` substitution is removed.
