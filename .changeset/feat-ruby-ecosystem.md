---
monochange: minor
monochange_core: minor
monochange_config: minor
---

#### add Ruby ecosystem support

monochange now discovers and manages Ruby gems from `.gemspec` files with version constants in `version.rb`.

**Configuration:**

```toml
[defaults]
package_type = "ruby"

[package.core]
path = "gems/core"
versioned_files = [
	{ path = "lib/core/version.rb", regex = 'VERSION\s*=\s*"(?<version>\d+\.\d+\.\d+)"' },
]

[ecosystems.ruby]
enabled = true
```

**What it discovers:**

- Gems by scanning for `.gemspec` files
- Version constants from `lib/<gem_name>/version.rb` using `VERSION = "x.y.z"` pattern
- Runtime dependencies from `add_dependency` and `add_runtime_dependency`
- Development dependencies from `add_development_dependency`

**Version management:**

- Reads VERSION constants from `version.rb` files (supports both single and double quotes)
- Updates VERSION constants with new version strings while preserving quote style
- Falls back to `lib/version.rb` and recursive search when the standard path doesn't exist

**Lockfile commands:**

- Infers `bundle lock --update` when `Gemfile.lock` exists
- Configurable via `[ecosystems.ruby].lockfile_commands`
