---
monochange_analysis: test
monochange_config: test
monochange_core: test
monochange_hosting: test
---

# Add mutation regression coverage

Add cargo-mutants regression coverage for `monochange_analysis`, `monochange_config`, `monochange_core`, and `monochange_hosting`.

- `monochange_analysis`: add tests for `latest_workspace_release_tag` to ignore namespaced tags, `snapshot_files_from_working_tree` and `read_text_file_from_git_object` for medium/exact-size files, `detect_raw_pr_environment` to reject non-PR events across CI providers, `default_branch_name` with origin HEAD symbolic ref, `get_merge_base` returning actual merge base, and `ChangeFrame::changed_files` distinguishing working directory from staged-only.
- `monochange_config`: add fixture-backed tests and proptests for ecosystem versioned-file inheritance, explicit group bump inference, and changelog validation defaults to close mutation gaps in release-planning configuration loading.
- `monochange_core`: add fixture-backed tests for discovery filtering around parent `.git` directories outside the workspace root and block-comment stripping edge cases in JSON helper logic.
- `monochange_hosting`: add HTTP-mock tests for error paths in `get_json`, `get_optional_json`, `post_json`, `put_json`, and `patch_json` to kill mutants on status-code checks. Update `Cargo.toml` to include `src/*.rs` in the distribution manifest so `__tests.rs` builds correctly with `httpmock` as a dev-dependency.
