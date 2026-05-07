# monochange_schema

`monochange_schema` owns durable JSON schema versioning and migration metadata for monochange artifacts that are written to disk or embedded in git history.

The schema version written to durable artifacts is `v`, a `major.minor` string derived from this crate's package version. Patch releases of this crate do not change `v`.

Version `0.1` is the first public durable schema version. Future readers will migrate older known artifacts in memory and never rewrite immutable history as a side effect of reading. Newer unsupported versions fail loudly so older CLIs do not partially interpret future records.

Durable release-record payloads are described by `schemas/release-record.schema.json`, also published with the documentation site at <https://monochange.github.io/monochange/schemas/release-record.schema.json>.

Migration edges are also published as machine-readable JSON in `schemas/migration-changelog.json`.
