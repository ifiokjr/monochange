---
monochange: minor
monochange_config: minor
monochange_core: minor
---

# Add initial changelog headers

Changelog targets now support `initial_header` in `[defaults.changelog]`, `[package.<id>.changelog]`, and `[group.<id>.changelog]`. monochange renders the header only when creating a changelog from empty content, preserving existing preambles on later releases.

When no custom header is configured, the selected changelog format provides a built-in default header.
