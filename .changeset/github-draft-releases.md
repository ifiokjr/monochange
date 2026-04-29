---
monochange: minor
monochange_core: minor
---

# Publish GitHub releases through drafts

- Add a boolean `draft` input to the built-in `PublishRelease` step so CLI commands can create hosted releases as drafts while preserving `[source.releases].draft` defaults.
- Update release automation to create draft GitHub releases, run the asset upload workflow against those drafts, then publish the drafts after assets are attached.
- Add a global `--jq` filter for JSON-producing commands so automation can extract release tags and other fields directly from `--format json` output.
