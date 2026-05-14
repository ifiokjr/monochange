---
monochange: test
---

# Fix flaky `reuses_prepared_release_artifact_for_versions` test

The `execute_cli_command_with_options_reuses_prepared_release_artifact_for_versions` test (and the related `plans_publish_rate_limits_from_prepared_release_artifact` and `reports_invalid_versions_output_formats` tests) operated on the real repository workspace root without holding the `TEST_ENV_LOCK`. When other test threads modified workspace files concurrently, the `git status` snapshot captured at artifact save time could differ from the snapshot taken at validation time, causing an intermittent "workspace status no longer matches the saved prepared release" error.

All three tests now acquire `TEST_ENV_LOCK` before reading the workspace, serialising them against other tests that modify git state.
