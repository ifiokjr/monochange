---
monochange: minor
---

# Sort release targets and hash identity fields directly

`release_targets_hash` now sorts targets by `(id, kind, version)` before hashing and only feeds identity fields (`id`, `kind`, `version`) into the hasher. Operational flags (`tag`, `release`, `tag_name`, `version_format`, `members`) are excluded from the hash so that path identity matches release identity.

`ReleasePaths::from_manifest` computes the hash directly from the manifest without building the intermediate `ReleaseRecord`, and `write_release_record_file` now checks file existence before doing any expensive work.
