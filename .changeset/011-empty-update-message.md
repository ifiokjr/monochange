---
monochange: patch
monochange_core: patch
monochange_config: patch
---

#### add configurable fallback changelog messages for version-only updates

Add `empty_update_message` support to `[defaults]`, `[package.<id>]`, and `[group.<id>]` so grouped packages and shared changelogs can render readable fallback changelog entries when a release updates versions without direct release notes for that target. Package changelog fallback precedence is package → group → defaults → built-in message, and group changelog fallback precedence is group → defaults → built-in message.
