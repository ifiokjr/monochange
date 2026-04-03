---
monochange: patch
---

#### `mc init` generates fully annotated `monochange.toml`

Replace the plain `toml::to_string_pretty` serialization with a minijinja template file (`monochange.init.toml`) compiled into the binary via `include_str!`. The generated config documents every available option with inline comments, matching the style of the existing `monochange.toml` reference.
