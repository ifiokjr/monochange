---
monochange_config: none
monochange_core: none
monochange_graph: none
monochange_semver: none
---

#### test

Add additional property-based tests across crates.

- `monochange_config`: proptests for `render_changelog_path_template` with `{{package_dir}}`, `{{package_name}}`, unknown placeholder passthrough, and idempotence. Remove unused `minijinja` dependency.
- `monochange_core`: proptests for `strip_json_comments` on string literal preservation, line/block comment removal, idempotence, and comment-free input preservation.
- `monochange_graph`: proptests for `trigger_priority` ordering, `propagation_is_suppressed` correctness, and `NormalizedGraph` reverse edge correctness.
- `monochange_semver`: proptests for `merge_severities` idempotence and Major severity absorbing all values.
- Fix clippy `needless_borrow` lint in `monochange_github` and `indexing_slicing` lint in `monochange_graph` proptests.
