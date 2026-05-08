---
monochange: patch
monochange_publish: minor
---

# Move registry infrastructure from `monochange` into `monochange_publish`

This change relocates registry-facing utilities so the publish crate owns all HTTP transport and registry endpoint concerns:

- `RegistryEndpoints` – configurable registry base URLs with environment fallbacks
- `registry_client()` – shared blocking HTTP client with monochange user-agent
- `package_can_be_published()` – predicate that checks publish enablement and state
- `filter_pending_publish_requests()` – filters out already-published or external entries
- `filter_pending_publish_requests_with_transport()` – same with transport-aware checks
- `registry_version_exists()` – ecosystem-aware version existence probe
- `crates_io_version_exists()` – Crates.io API version lookup with index fallback
- `crates_io_index_version_exists()` – sparse-index version existence check
- `crates_io_index_entry_path()` – sparse-index path computation for a crate name

`monochange` now delegates to these via `monochange_publish` imports rather than owning the implementation. `publish_rate_limits.rs` also imports them from `monochange_publish` instead of `package_publish` directly.
